use crate::client::paginated::PaginatedFetcher;
use crate::client::paginated_issues::PaginatedIssues;
use crate::config::{Config, JiraConfig};
use crate::models::jira::JiraError::ApiError;
use crate::models::jira::{
    JiraAuthor, JiraBoard, JiraError, JiraIssue, JiraMyself, JiraSprint, JiraWorklog,
};
use base64::engine::general_purpose;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use log::{debug, error, trace};
use once_cell::sync::OnceCell;
use reqwest::Client;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct JiraClient {
    base_url: String,
    client: Client,
    auth_header: String,
}

impl JiraClient {
    pub fn create() -> Self {
        let config = match Config::load() {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        };
        Self::new(&config.jira)
    }

    fn new(config: &JiraConfig) -> Self {
        let credentials = format!("{}:{}", config.username, config.api_token.reveal());
        let encoded_credentials = general_purpose::STANDARD.encode(credentials);
        let auth_header = format!("Basic {}", encoded_credentials);
        JiraClient {
            base_url: config.base_url.clone(),
            client: Client::new(),
            auth_header,
        }
    }

    pub async fn get_project_issues(
        &self,
        project_name: &str,
        start_date: Option<DateTime<Utc>>,
    ) -> Result<PaginatedIssues<'_>, JiraError> {
        let start = start_date.unwrap_or(Utc::now());
        let jql = format!(
            "project='{}' and createdDate >= '{}'",
            project_name,
            start.format("%Y-%m-%d")
        );
        let fetcher = self.get_issue_fetcher(jql).await?;
        Ok(fetcher)
    }

    pub async fn get_project_backlog_issues(
        &self,
        project_name: &str,
        start_date: Option<DateTime<Utc>>,
    ) -> Result<PaginatedIssues<'_>, JiraError> {
        let start = start_date.unwrap_or(Utc::now());
        let jql = format!(
            "project='{}' and sprint is EMPTY and createdDate >= '{}'",
            project_name,
            start.format("%Y-%m-%d")
        );
        let fetcher = self.get_issue_fetcher(jql).await?;
        Ok(fetcher)
    }

    async fn get_issue_fetcher(&self, jql: String) -> Result<PaginatedIssues<'_>, JiraError> {
        let fetcher = PaginatedIssues::initialize(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            jql,
        )
        .await
        .map_err(|e| ApiError(e.to_string()))?;
        Ok(fetcher)
    }

    pub async fn get_all_issues_v2(
        &self,
        sprint_id: &str,
    ) -> Result<PaginatedIssues<'_>, JiraError> {
        let jql = format!("sprint={}", sprint_id);
        let fetcher = self.get_issue_fetcher(jql).await?;
        Ok(fetcher)
    }

    pub async fn get_issue_worklogs(&self, issue: JiraIssue) -> Vec<JiraWorklog> {
        let jql = "worklogAuthor=currentUser()";
        let endpoint = format!("/rest/api/3/issue/{}/worklog?jql={}&", issue.key, jql);

        let fetcher = match PaginatedFetcher::initialize(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            endpoint,
            |start_at| format!("startAt={}", start_at),
        )
        .await
        {
            Ok(f) => f,
            Err(e) => {
                error!("Error fetching worklogs of {}: {}", issue.key, e);
                return vec![];
            }
        };

        let mut worklogs: Vec<JiraWorklog> = fetcher.collect();

        // Get current user email for filtering (JQL might not work on this endpoint)
        let current_user_email = match Config::load() {
            Ok(config) => config.jira.username.clone(),
            Err(e) => {
                error!("Failed to load config for worklog filtering: {}", e);
                return vec![];
            }
        };

        // Filter to only include worklogs by current user. Jira Cloud may hide
        // `emailAddress` (profile visibility), so match on accountId as well.
        let current_account_id = self.current_account_id().await;
        worklogs.retain(|w| {
            is_current_user(
                &w.author,
                &current_user_email,
                current_account_id.as_deref(),
            )
        });

        // Normalize the author email so downstream code (stored `Worklog.author`)
        // keeps matching the configured username even when Jira hides it.
        for worklog in &mut worklogs {
            worklog
                .author
                .email_address
                .get_or_insert_with(|| current_user_email.clone());
        }

        // IMPORTANT: Set the issue_id to the issue key for each worklog
        // Jira API doesn't always return issue_id, but we need it for deletion
        for worklog in &mut worklogs {
            worklog.issue_id = issue.key.clone();
        }

        worklogs
    }

    pub async fn add_time_to_issue(
        &self,
        issue_key: &str,
        duration: Duration,
        start: DateTime<Utc>,
        comment: Option<String>,
    ) -> Result<Option<JiraWorklog>, JiraError> {
        let url = format!(
            "{}/rest/api/latest/issue/{}/worklog",
            &self.base_url, issue_key
        );
        let worklog = Worklog {
            time_spent: duration.num_seconds(),
            started: start.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string(),
            comment: comment.unwrap_or("wtf".to_string()),
        };
        let response = self
            .client
            .post(&url)
            .header("Authorization", &self.auth_header)
            .json(&worklog)
            .send()
            .await
            .map_err(|e| ApiError(e.to_string()))?;
        if response.status().is_success() {
            trace!("response : {:?}", response);
            debug!("Time logged successfully on issue {}", issue_key);
            match response.headers().get("location") {
                Some(location) => {
                    trace!("location : {:?}", location);
                    let location_str = location
                        .to_str()
                        .map_err(|e| ApiError(format!("Invalid location header: {}", e)))?;
                    let worklog = self.get_worklog(location_str).await;
                    debug!("Worklog: {:?}", worklog);
                    return Ok(worklog);
                }
                _ => error!("Error location : {:?}", response),
            }
            Ok(None)
        } else {
            Err(ApiError(response.status().to_string()))
        }
    }

    pub async fn delete_worklog(&self, issue_key: &str, worklog_id: &str) -> Result<(), JiraError> {
        let url = format!(
            "{}/rest/api/3/issue/{}/worklog/{}",
            &self.base_url, issue_key, worklog_id
        );
        debug!("DELETE URL: {}", url);
        let response = self
            .client
            .delete(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        let status = response.status();
        debug!("DELETE response status: {}", status);

        if status.is_success() {
            debug!(
                "Successfully deleted worklog {} from issue {}",
                worklog_id, issue_key
            );
            Ok(())
        } else {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "No body".to_string());
            error!("DELETE failed with status {}: {}", status, error_body);
            Err(ApiError(format!("{} - {}", status, error_body)))
        }
    }

    /// Account id of the authenticated user (`/rest/api/3/myself`), cached for the process.
    pub async fn current_account_id(&self) -> Option<String> {
        static ACCOUNT_ID: OnceCell<String> = OnceCell::new();
        if let Some(id) = ACCOUNT_ID.get() {
            return Some(id.clone());
        }

        let url = format!("{}/rest/api/3/myself", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| error!("Error fetching current Jira user: {}", e))
            .ok()?;
        if !response.status().is_success() {
            error!(
                "Error fetching current Jira user: HTTP {}",
                response.status()
            );
            return None;
        }
        let myself: JiraMyself = response
            .json()
            .await
            .map_err(|e| error!("Error deserializing current Jira user: {}", e))
            .ok()?;
        Some(ACCOUNT_ID.get_or_init(|| myself.account_id).clone())
    }

    pub async fn get_worklog(&self, url: &str) -> Option<JiraWorklog> {
        debug!("Fetching worklog from {}", url);
        let response = match self
            .client
            .get(url.to_string())
            .header("Authorization", &self.auth_header)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Error fetching worklog from {}: {}", url, e);
                return None;
            }
        };

        if response.status().is_success() {
            trace!("Response : {:?}", response);
            match response.json().await {
                Ok(worklog) => return Some(worklog),
                Err(e) => error!("Error deserializing worklog: {}", e),
            }
        }
        None
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<JiraIssue, JiraError> {
        let url = format!("{}/rest/api/3/issue/{}", self.base_url, issue_id);
        debug!("url: {}", url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        if response.status().is_success() {
            debug!("getting issue from jira with key: {}", issue_id);
            let issue: JiraIssue = response
                .json()
                .await
                .map_err(|e| ApiError(format!("Failed to deserialize issue: {}", e)))?;
            Ok(issue)
        } else {
            debug!("getting error from jira with key: {:?}", response);
            Err(ApiError(response.status().to_string()))
        }
    }

    pub async fn get_all_sprint(
        &self,
        board_id: usize,
    ) -> Result<PaginatedFetcher<'_, JiraSprint>, JiraError> {
        debug!("fetching all sprints");
        let endpoint = format!("/rest/agile/latest/board/{}/sprint?", board_id);
        let fetcher: PaginatedFetcher<JiraSprint> = PaginatedFetcher::initialize(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            endpoint,
            |start_at| format!("startAt={}", start_at),
        )
        .await
        .map_err(|e| ApiError(e.to_string()))?;
        Ok(fetcher)
    }

    pub async fn get_all_boards(&self) -> Result<PaginatedFetcher<'_, JiraBoard>, JiraError> {
        let fetcher: PaginatedFetcher<JiraBoard> = PaginatedFetcher::initialize(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            "/rest/agile/1.0/board?".to_string(),
            |start_at| format!("startAt={}", start_at),
        )
        .await
        .map_err(|e| ApiError(e.to_string()))?;
        Ok(fetcher)
    }

    pub async fn get_worklogs_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<JiraWorklog>, JiraError> {
        let start_date = start.format("%Y-%m-%d").to_string();
        let end_date = end.format("%Y-%m-%d").to_string();
        let jql = format!("worklogAuthor=currentUser() and worklogDate >= {start_date} and worklogDate <= {end_date}");
        let fetcher: PaginatedIssues = PaginatedIssues::initialize(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            jql,
        )
        .await
        .map_err(|e| ApiError(e.to_string()))?;
        let mut worklogs = Vec::new();
        let issues: Vec<JiraIssue> = fetcher.collect();
        debug!("issues logged: {}", issues.len());
        for issue in issues {
            debug!("issue: {:?}", issue);
            debug!("Fetching worklogs for issue: {}", issue.key);
            let issue_worklogs = get_worklogs_for_issue(issue).await;
            let worklogs_to_add: Vec<JiraWorklog> = issue_worklogs
                .iter()
                .filter(|w| {
                    w.started.date_naive() <= end.date_naive()
                        && w.started.date_naive() >= start.date_naive()
                })
                .cloned()
                .collect();
            debug!("{} worklogs in current page", worklogs_to_add.len());
            worklogs.extend(worklogs_to_add);
        }
        debug!("{} worklogs between {start} and {end}", worklogs.len());
        Ok(worklogs)
    }

    pub async fn get_worklogs_of_day(
        &self,
        date: DateTime<Utc>,
    ) -> Result<Vec<JiraWorklog>, JiraError> {
        self.get_worklogs_between(date, date).await
    }
}

#[derive(Serialize)]
struct Worklog {
    #[serde(rename = "timeSpentSeconds")]
    time_spent: i64,
    started: String,
    comment: String,
}

//FIXME: maybe implements a cache here
fn is_current_user(author: &JiraAuthor, email: &str, account_id: Option<&str>) -> bool {
    match (account_id, author.account_id.as_deref()) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => author.email_address.as_deref() == Some(email),
    }
}

pub async fn get_worklogs_for_issue(issue: JiraIssue) -> Vec<JiraWorklog> {
    let client = JiraClient::create();
    client.get_issue_worklogs(issue).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn author(account_id: Option<&str>, email: Option<&str>) -> JiraAuthor {
        JiraAuthor {
            account_id: account_id.map(String::from),
            display_name: "Someone".to_string(),
            email_address: email.map(String::from),
        }
    }

    #[test]
    fn current_user_matched_by_account_id_when_email_hidden() {
        let me = author(Some("me"), None);
        let other = author(Some("other"), None);
        assert!(is_current_user(&me, "me@example.com", Some("me")));
        assert!(!is_current_user(&other, "me@example.com", Some("me")));
    }

    #[test]
    fn current_user_falls_back_to_email_without_account_id() {
        let me = author(Some("me"), Some("me@example.com"));
        let other = author(Some("other"), Some("other@example.com"));
        assert!(is_current_user(&me, "me@example.com", None));
        assert!(!is_current_user(&other, "me@example.com", None));
        assert!(!is_current_user(
            &author(Some("me"), None),
            "me@example.com",
            None
        ));
    }
}
