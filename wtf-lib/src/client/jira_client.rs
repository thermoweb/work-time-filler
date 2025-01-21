use crate::client::paginated::{PaginatedFetcher, PaginatedResponse};
use crate::config::JiraConfig;
use crate::models::jira::{JiraBoard, JiraError, JiraIssue, JiraSprint, JiraWorklog};
use base64::engine::general_purpose;
use base64::Engine;
use log::debug;
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct JiraClient {
    base_url: String,
    client: Client,
    auth_header: String,
}

impl JiraClient {
    pub fn new(config: &JiraConfig) -> Self {
        let credentials = format!("{}:{}", config.username, config.api_token);
        let encoded_credentials = general_purpose::STANDARD.encode(credentials);
        let auth_header = format!("Basic {}", encoded_credentials);
        JiraClient {
            base_url: config.base_url.clone(),
            client: Client::new(),
            auth_header,
        }
    }

    pub async fn get_all_issues_with_jql(&self, jql: &str) -> Result<Vec<JiraIssue>, JiraError> {
        let issues: Vec<JiraIssue> = PaginatedResponse::fetch_all_items(
            &self.base_url,
            &self.client,
            &self.auth_header,
            "/rest/api/3/search?",
            |start_at| format!("jql={}&startAt={}", urlencoding::encode(jql), start_at),
            None,
        )
        .await?;
        Ok(issues)
    }

    pub async fn get_all_issues(
        &self,
        sprint: &JiraSprint,
    ) -> Result<PaginatedFetcher<JiraIssue>, JiraError> {
        let jql = format!("sprint={}", sprint.id);
        let endpoint = format!("/rest/api/3/search?jql={}&", urlencoding::encode(&*jql));
        debug!("endpoint: {}", endpoint);
        let fetcher: PaginatedFetcher<JiraIssue> = PaginatedFetcher::new(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            endpoint,
            |start_at| format!("startAt={}", start_at),
        );
        Ok(fetcher)
    }

    pub async fn get_issue_worklogs(
        &self,
        issue: JiraIssue,
    ) -> Result<Vec<JiraWorklog>, JiraError> {
        let jql = "worklogAuthor=currentUser()";
        let worklogs: Vec<JiraWorklog> = PaginatedResponse::fetch_all_items(
            &self.base_url,
            &self.client,
            &self.auth_header,
            format!("/rest/api/3/issue/{}/worklog?", issue.key).as_str(),
            |start_at| format!("jql={}&startAt={}", urlencoding::encode(jql), start_at),
            None,
        )
        .await?;
        Ok(worklogs)
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<JiraIssue, JiraError> {
        let url = format!("{}/rest/api/3/issue/{}", self.base_url, issue_id);
        let response = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .unwrap();

        if response.status().is_success() {
            let issue: JiraIssue = response.json().await.unwrap();
            Ok(issue)
        } else {
            Err(JiraError::ApiError(response.status().to_string()))
        }
    }

    pub async fn get_all_sprint(
        &self,
        board_id: usize,
    ) -> Result<PaginatedFetcher<JiraSprint>, JiraError> {
        let endpoint = format!("/rest/agile/latest/board/{}/sprint?", board_id);
        let fetcher: PaginatedFetcher<JiraSprint> = PaginatedFetcher::new(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            endpoint,
            |start_at| format!("startAt={}", start_at),
        );
        Ok(fetcher)
        // let sprints = fetcher.fetch_all().await?;
    }

    pub async fn get_all_boards(&self) -> Result<Vec<JiraBoard>, JiraError> {
        let boards: Vec<JiraBoard> = PaginatedResponse::fetch_all_items(
            &self.base_url,
            &self.client,
            &self.auth_header,
            "/rest/agile/1.0/board?",
            |start_at| format!("startAt={}", start_at),
            None,
        )
        .await?;
        Ok(boards)
    }
}
