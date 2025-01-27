use crate::models::data::{Issue, Worklog};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, Hash, PartialEq)]
pub struct JiraIssue {
    pub id: String,
    pub key: String,
    pub fields: JiraFields,
}

impl fmt::Display for JiraIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} - ({})",
            self.key, self.fields.summary, self.fields.status.name
        )
    }
}

impl JiraIssue {
    pub fn into(self) -> Issue {
        Issue {
            key: self.key.clone(),
            id: self.id,
            created: self.fields.created,
            status: self.fields.status.name,
            summary: self.fields.summary,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
pub struct JiraFields {
    pub summary: String,
    pub status: JiraStatus,
    pub created: DateTime<Utc>,
    pub worklogs: Option<Vec<JiraWorklog>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
pub struct JiraStatus {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklog {
    pub id: String,
    pub author: JiraAuthor,
    pub created: DateTime<Utc>,
    pub time_spent: String,
    pub time_spent_seconds: u64,
    pub comment: Option<JiraComment>,
    pub issue_id: String,
    pub started: DateTime<Utc>,
}

impl fmt::Display for JiraWorklog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted_comment = if let Some(comment) = &self.comment {
            format!(": {}", format_comment(comment))
        } else {
            "".to_string()
        };
        write!(
            f,
            "{} spent {} on {} ({}){}",
            self.author.display_name,
            self.time_spent,
            self.issue_id,
            self.started,
            formatted_comment
        )
    }
}

impl JiraWorklog {
    pub fn into_worklog(&self) -> Worklog {
        Worklog {
            id: self.id.clone(),
            author: self.author.email_address.clone(),
            created: self.created,
            time_spent: self.time_spent.clone(),
            time_spent_seconds: self.time_spent_seconds.clone(),
            comment: self.comment.clone().map(|jc| format_comment(&jc)),
            issue_id: self.issue_id.to_string(),
            started: self.started,
        }
    }
}

pub fn format_comment(comment: &JiraComment) -> String {
    match comment {
        JiraComment::Full {
            r#type: _type,
            content,
            version: _version,
        } => content
            .iter()
            .filter_map(|content| {
                content.content.as_ref().map(|texts| {
                    texts
                        .iter()
                        .filter_map(|text| text.text.as_ref())
                        .cloned()
                        .collect::<Vec<String>>()
                        .join(" ")
                })
            })
            .collect::<Vec<String>>()
            .join("."),
        JiraComment::Text(text) => String::from(text),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
#[serde(untagged)]
pub enum JiraComment {
    Full {
        r#type: String,
        version: u64,
        content: Vec<JiraContent>,
    },
    Text(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
pub struct JiraContent {
    pub r#type: String,
    pub content: Option<Vec<JiraText>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
pub struct JiraText {
    pub r#type: String,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JiraUpdatedWorklogsResponse {
    pub values: Vec<JiraUpdatedWorklog>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraUpdatedWorklog {
    pub worklog_id: usize,
    pub updated_time: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JiraAuthor {
    pub display_name: String,
    pub email_address: String,
}

#[derive(Debug)]
pub enum JiraError {
    ApiError(String),
    DeserializeError(String),
    RequestError(reqwest::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JiraSprint {
    pub name: String,
    pub state: String,
    pub id: usize,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub original_board_id: Option<usize>,
}

impl fmt::Display for JiraSprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}][{}] {}", self.id, self.state, self.name)?;

        let format_date = |date: &DateTime<Utc>| date.format("%d-%m-%Y %H:%M").to_string();

        let dates = match (self.start_date, self.end_date) {
            (Some(start), Some(end)) => {
                format!(" ({} - {})", format_date(&start), format_date(&end))
            }
            (Some(start), None) => format!(" ({})", format_date(&start)),
            (None, Some(end)) => format!(" ({})", format_date(&end)),
            (None, None) => String::new(),
        };

        write!(f, "{}", dates)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JiraBoard {
    pub id: usize,
    pub name: String,
    pub r#type: String,
    pub location: Option<JiraBoardLocation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JiraBoardLocation {
    pub project_id: usize,
    pub project_name: String,
}

impl fmt::Display for JiraBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} - ({})", self.id, self.name, self.r#type)
    }
}
