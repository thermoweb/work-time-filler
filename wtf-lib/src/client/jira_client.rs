use crate::client::paginated::PaginatedFetcher;
use crate::client::paginated_issues::PaginatedIssues;
use crate::config::{Config, JiraConfig};
use crate::models::jira::JiraError::ApiError;
use crate::models::jira::{JiraBoard, JiraError, JiraIssue, JiraSprint, JiraWorklog};
use base64::engine::general_purpose;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use log::{debug, error, trace};
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
            start.format("%Y-%m-%d").to_string()
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
            start.format("%Y-%m-%d").to_string()
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
                eprintln!("Error fetching worklogs: {}", e);
                return vec![];
            }
        };

        let mut worklogs: Vec<JiraWorklog> = fetcher.collect();

        // Get current user email for filtering (JQL might not work on this endpoint)
        let config = Config::load().expect("Failed to load config");
        let current_user_email = config.jira.username.clone();

        // Filter to only include worklogs by current user
        worklogs.retain(|w| w.author.email_address == current_user_email);

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
            .unwrap();
        if response.status().is_success() {
            trace!("response : {:?}", response);
            debug!("Time logged successfully on issue {}", issue_key);
            match response.headers().get("location") {
                Some(location) => {
                    trace!("location : {:?}", location);
                    let worklog = self.get_worklog(location.to_str().unwrap()).await;
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
            .unwrap();

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

    pub async fn get_worklog(&self, url: &str) -> Option<JiraWorklog> {
        debug!("Fetching worklog from {}", url);
        let response = self
            .client
            .get(url.to_string())
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .unwrap();

        if response.status().is_success() {
            trace!("Response : {:?}", response);
            let worklog: JiraWorklog = response.json().await.unwrap();
            return Some(worklog);
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
            .unwrap();

        if response.status().is_success() {
            debug!("getting issue from jira with key: {}", issue_id);
            let issue: JiraIssue = response.json().await.unwrap();
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
        let fetcher: PaginatedFetcher<JiraSprint> = PaginatedFetcher::new(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            endpoint,
            |start_at| format!("startAt={}", start_at),
        );
        Ok(fetcher)
    }

    pub async fn get_all_boards(&self) -> Result<PaginatedFetcher<'_, JiraBoard>, JiraError> {
        let fetcher: PaginatedFetcher<JiraBoard> = PaginatedFetcher::new(
            &self.client,
            self.base_url.clone(),
            self.auth_header.clone(),
            "/rest/agile/1.0/board?".to_string(),
            |start_at| format!("startAt={}", start_at),
        );
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
pub async fn get_worklogs_for_issue(issue: JiraIssue) -> Vec<JiraWorklog> {
    let client = JiraClient::create();
    client.get_issue_worklogs(issue).await
}
