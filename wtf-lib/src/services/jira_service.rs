use indicatif::{ProgressIterator, ProgressStyle};
use log::{debug, info};
use crate::client::jira_client::JiraClient;
use crate::config::{Config, JiraConfig};
use crate::models::jira::{JiraError, JiraIssue, JiraSprint, JiraWorklog};

pub struct JiraService {
    jira_client: JiraClient,
}

impl JiraService {
    pub fn new(jira_config: &JiraConfig) -> Self {
        let jira_client = JiraClient::new(jira_config);
        Self { jira_client }
    }

    pub async fn get_worklogs(&self, username: String) -> Result<Vec<JiraWorklog>, JiraError> {
        let issues = self.find_issues().await?;
        let mut all_worklogs = Vec::new();
        for issue in issues {
            let worklogs = self.jira_client.get_issue_worklogs(issue).await?;
            all_worklogs.extend(
                worklogs
                    .iter()
                    .filter(|w| w.author.email_address == username)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
        Ok(all_worklogs)
    }

    async fn find_issues(&self) -> Result<Vec<JiraIssue>, JiraError> {
        let jql = "worklogAuthor = currentUser()";
        let issues = self.jira_client.get_all_issues_with_jql(jql).await?;

        Ok(issues)
    }
}
