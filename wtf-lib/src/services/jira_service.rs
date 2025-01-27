use crate::client::jira_client::JiraClient;
use crate::models::data::{Board, Issue, Sprint, Worklog};
use crate::services::worklogs_service::WorklogsService;
use crate::storage::database::{GenericDatabase, DATABASE};
use chrono::{DateTime, Duration, Utc};
use lazy_static::lazy_static;
use log::{debug, error};
use once_cell::sync::Lazy;
use regex::Regex;
use std::error::Error;
use std::sync::Arc;

static ISSUES_DATABASE: Lazy<Arc<GenericDatabase<Issue>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "issues").expect("could not initialize issues database"),
    )
});

lazy_static! {
    static ref JIRA_CARD_IDENTIFIER: Regex = Regex::new(r"([a-zA-Z]+-[0-9]+)").unwrap();
}

pub struct IssueService;

impl IssueService {
    pub fn save_issue(issue: &Issue) {
        ISSUES_DATABASE.insert(issue).unwrap();
    }

    pub fn save_all_issues(issues: Vec<Issue>) {
        ISSUES_DATABASE.save_all(issues).unwrap();
    }

    pub fn get_all_issues() -> Vec<Issue> {
        ISSUES_DATABASE.get_all().unwrap()
    }

    pub fn get_by_key(key: &str) -> Option<Issue> {
        ISSUES_DATABASE.get(key).unwrap()
    }

    pub async fn add_time(
        issue_key: &str,
        duration: Duration,
        start: DateTime<Utc>,
        comment: Option<String>,
    ) -> Result<Option<Worklog>, Box<dyn Error>> {
        let jira_client = JiraClient::create();
        match jira_client
            .add_time_to_issue(issue_key, duration, start, comment)
            .await
        {
            Ok(jira_worklog) => {
                if let Some(jira_worklog) = jira_worklog {
                    let worklog = jira_worklog.into_worklog();
                    WorklogsService::save_worklog(worklog.clone());
                    return Ok(Some(worklog));
                }
                return Ok(None);
            }
            Err(e) => error!("an error occurred while adding time to issue: {:?}", e),
        }
        Err(format!(
            "an error occurred while adding time to issue: {:?}",
            issue_key
        )
        .into())
    }

    pub async fn delete_worklog(issue_key: &str, worklog_id: &str) {
        let jira_client = JiraClient::create();
        debug!(
            "deleting worklog '{}' for issue '{}' (KEY)",
            worklog_id, issue_key
        );
        match jira_client.delete_worklog(issue_key, worklog_id).await {
            Ok(()) => {
                WorklogsService::remove_worklog(worklog_id);
                debug!(
                    "worklog '{}' deleted from Jira and local database",
                    worklog_id
                );
            }
            Err(e) => {
                // Check if it's a 404 - worklog doesn't exist in Jira (already deleted or never existed)
                // Or 400 - permission denied (not your worklog, shouldn't be in our DB)
                let err_str = format!("{:?}", e);
                if err_str.contains("404")
                    || err_str.contains("Not Found")
                    || err_str.contains("400")
                    || err_str.contains("autorisation")
                {
                    debug!(
                        "worklog '{}' error from Jira ({}), removing from local database anyway",
                        worklog_id, err_str
                    );
                    WorklogsService::remove_worklog(worklog_id);
                } else {
                    error!(
                        "Failed to delete worklog '{}' from issue '{}': {:?}",
                        worklog_id, issue_key, e
                    );
                }
            }
        }
    }
}

static BOARD_DATABASE: Lazy<Arc<GenericDatabase<Board>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "boards").expect("could not initialize board database"),
    )
});
pub struct BoardService;

impl BoardService {
    pub fn save_board(board: &Board) {
        BOARD_DATABASE.insert(board).unwrap();
    }

    pub fn get_all_boards() -> Vec<Board> {
        BOARD_DATABASE.get_all().unwrap()
    }

    pub fn get_by_id(id: &str) -> Option<Board> {
        BOARD_DATABASE.get(id).unwrap()
    }
}

static SPRINT_DATABASE: Lazy<Arc<GenericDatabase<Sprint>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "sprints").expect("could not initialize sprint database"),
    )
});

pub struct SprintService;

impl SprintService {
    pub fn get_sprint(sprint_id: &str) -> Result<Option<Sprint>, Box<dyn Error + Send + Sync>> {
        SPRINT_DATABASE.get(sprint_id)
    }

    pub fn get_sprint_by_id(id: &str) -> Option<Sprint> {
        SPRINT_DATABASE.get(id).unwrap()
    }

    pub fn save_sprint(sprint: &Sprint) {
        SPRINT_DATABASE.insert(sprint).unwrap();
    }

    pub fn save_all_sprints(sprints: Vec<Sprint>) {
        SPRINT_DATABASE.save_all(sprints).unwrap();
    }
}

pub struct JiraService;

impl JiraService {
    pub fn get_available_sprints() -> Vec<Sprint> {
        let sprints = SPRINT_DATABASE.get_all().unwrap();
        sprints
    }

    pub fn get_followed_sprint() -> Vec<Sprint> {
        let sprints: Vec<Sprint> = SPRINT_DATABASE
            .get_all()
            .unwrap()
            .iter()
            .filter(|s| s.followed)
            .cloned()
            .collect();
        sprints
    }

    pub fn follow_sprint(sprint_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        match SPRINT_DATABASE.get(sprint_id)? {
            Some(mut sprint) => {
                if !sprint.followed {
                    sprint.followed = true;
                    SprintService::save_sprint(&sprint);
                    Ok(())
                } else {
                    Err("Sprint already followed")?
                }
            }
            None => Err("Sprint not found")?,
        }
    }

    pub fn unfollow_sprint(sprint_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        match SPRINT_DATABASE.get(sprint_id)? {
            Some(mut sprint) => {
                if sprint.followed {
                    sprint.followed = false;
                    SprintService::save_sprint(&sprint);
                    Ok(())
                } else {
                    Err("Sprint not followed")?
                }
            }
            None => Err("Sprint not found")?,
        }
    }

    pub fn get_available_boards() -> Result<Vec<Board>, Box<dyn Error + Send + Sync>> {
        BOARD_DATABASE.get_all()
    }

    pub fn get_followed_boards() -> Result<Vec<Board>, Box<dyn Error + Send + Sync>> {
        Ok(BOARD_DATABASE
            .get_all()?
            .iter()
            .filter(|b| b.followed)
            .cloned()
            .collect())
    }

    pub fn follow_board(board_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        match BOARD_DATABASE.get(board_id)? {
            Some(mut db_board) => {
                db_board.followed = true;
                BOARD_DATABASE.insert(&db_board)
            }
            None => Err(format!("Board '{}' not found", board_id))?,
        }
    }

    pub fn unfollow_board(board_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        match BOARD_DATABASE.get(board_id)? {
            Some(mut db_board) => {
                db_board.followed = false;
                BOARD_DATABASE.insert(&db_board)
            }
            None => Err(format!("Board '{}' not found", board_id))?,
        }
    }

    pub async fn get_issue_by_key(key: &str) -> Option<Issue> {
        debug!("check issue with key '{}'", key);
        match ISSUES_DATABASE
            .get_all()
            .unwrap_or_default()
            .iter()
            .find(|i| i.key == key)
            .cloned()
        {
            Some(issue) => Some(issue),
            None => {
                debug!("no issue found in database, checking remotely...");
                if let Ok(issue) = JiraClient::create().get_issue(key).await {
                    let issue_to_store = issue.into();
                    ISSUES_DATABASE.insert(&issue_to_store).unwrap();
                    return Some(issue_to_store);
                }
                None
            }
        }
    }
}

pub fn has_jira_identifier(s: &str) -> bool {
    JIRA_CARD_IDENTIFIER.is_match(s)
}

pub fn get_jira_identifier(s: &str) -> Option<String> {
    JIRA_CARD_IDENTIFIER
        .captures(s)?
        .get(1)
        .map(|x| x.as_str().to_string().to_uppercase())
}

pub fn get_jira_identifiers(s: &str) -> Vec<String> {
    JIRA_CARD_IDENTIFIER
        .find_iter(s)
        .map(|x| x.as_str().to_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jira_identifier_detection() {
        assert!(has_jira_identifier("etech-123"));
        assert!(has_jira_identifier("ETECH-123"));
        assert!(has_jira_identifier("eTech-123"));
        assert!(has_jira_identifier("the card is etech-123"));
        assert!(has_jira_identifier("the card is etech-123 use it wisely"));
        assert!(has_jira_identifier("plw-1"));

        assert_eq!(has_jira_identifier("etech123"), false);
        assert_eq!(
            has_jira_identifier("etech is under - rated with 123"),
            false
        );
        assert_eq!(has_jira_identifier("nothing the see here"), false);
    }

    #[test]
    fn test_jira_get_identifier() {
        assert_eq!(get_jira_identifier("etech-123").unwrap(), "ETECH-123");
        assert_eq!(
            get_jira_identifier("some text with etech-123 card").unwrap(),
            "ETECH-123"
        );
        assert_eq!(
            get_jira_identifier("some text with etech-123 card").unwrap(),
            "ETECH-123"
        );
    }

    #[test]
    fn test_get_jira_identifiers() {
        let candidates =
            get_jira_identifiers("blabla etech-123, plw-14 and etech 345 not this one");
        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&"ETECH-123".to_string()));
        assert!(candidates.contains(&"PLW-14".to_string()));
    }
}
