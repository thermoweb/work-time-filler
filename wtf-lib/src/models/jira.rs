use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JiraFields {
    pub summary: String,
    pub status: JiraStatus,
    pub created: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JiraStatus {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklog {
    pub author: JiraAuthor,
    pub created: DateTime<FixedOffset>,
    pub time_spent: String,
    pub time_spent_seconds: u64,
    pub comment: Option<JiraComment>,
    pub issue_id: String,
    pub started: DateTime<FixedOffset>,
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

fn format_comment(comment: &JiraComment) -> String {
    comment
        .content
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
        .join(".")
}

#[derive(Debug, Deserialize, Clone)]
pub struct JiraComment {
    pub r#type: String,
    pub version: u64,
    pub content: Vec<JiraContent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JiraContent {
    pub r#type: String,
    pub content: Option<Vec<JiraText>>,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
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
    pub start_date: Option<DateTime<FixedOffset>>,
    pub end_date: Option<DateTime<FixedOffset>>,
    pub original_board_id: Option<usize>,
}

impl fmt::Display for JiraSprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} - ({:?} - {:?})",
            self.id, self.name, self.start_date, self.end_date
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JiraBoard {
    pub id: usize,
    pub name: String,
    pub r#type: String,
}

impl fmt::Display for JiraBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} - ({})", self.id, self.name, self.r#type)
    }
}
