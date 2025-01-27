use crate::models::jira::JiraIssue;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssuesPage {
    pub issues: Vec<JiraIssue>,
    pub next_page_token: Option<String>,
    pub is_last: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssuesCount {
    pub count: usize,
}

pub struct PaginatedIssues<'a> {
    client: &'a reqwest::Client,
    base_url: String,
    auth_header: String,
    jql: String,
    next_page_token: Option<String>,
    current_items: Vec<JiraIssue>,
    finished: bool,
    pub total_items: usize,
    pub yielded_items: usize,
}

impl<'a> PaginatedIssues<'a> {
    pub async fn initialize(
        client: &'a reqwest::Client,
        base_url: String,
        auth_header: String,
        jql: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut fetcher = PaginatedIssues {
            client,
            base_url,
            auth_header,
            jql,
            next_page_token: None,
            current_items: Vec::new(),
            finished: false,
            total_items: 0,
            yielded_items: 0,
        };
        fetcher.fetch_page().await?;
        if !fetcher.finished {
            let total = fetcher.fetch_total().await?;
            fetcher.total_items = total;
        } else {
            fetcher.total_items = fetcher.current_items.len();
        }
        Ok(fetcher)
    }

    async fn fetch_page(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut url = format!(
            "{}//rest/api/latest/search/jql?jql={}&fields=created,summary,status",
            self.base_url, self.jql
        );

        if let Some(token) = &self.next_page_token {
            url.push_str(&format!("?nextPageToken={}", token));
        }

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header.clone())
            .send()
            .await?;
        let text = response.text().await?;
        let page: JiraIssuesPage = serde_json::from_str(&text)?;

        self.current_items.extend(page.issues);
        self.next_page_token = page.next_page_token;
        self.finished = page.is_last.unwrap_or(false);
        Ok(())
    }

    async fn fetch_total(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/{}",
            self.base_url, "/rest/api/3/search/approximate-count"
        );
        let body = serde_json::json!({ "jql": self.jql });

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header.clone())
            .json(&body)
            .send()
            .await?;
        let text = response.text().await?;
        let count: JiraIssuesCount = serde_json::from_str(&text)?;

        Ok(count.count)
    }
}

impl<'a> Iterator for PaginatedIssues<'a> {
    type Item = JiraIssue;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.current_items.pop() {
            self.yielded_items += 1;
            return Some(item);
        }

        if !self.finished {
            futures::executor::block_on(self.fetch_page()).ok()?;

            if let Some(issue) = self.current_items.pop() {
                self.yielded_items += 1;
                return Some(issue);
            }
        }

        None
    }
}

impl<'a> ExactSizeIterator for PaginatedIssues<'a> {
    fn len(&self) -> usize {
        self.total_items
    }
}
