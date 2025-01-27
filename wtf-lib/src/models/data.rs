use crate::models::jira::JiraBoard;
use crate::services::jira_service::get_jira_identifiers;
use crate::storage::database::Identifiable;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use google_calendar3::api::EventAttendee;
use log::warn;
use rrule::{RRuleSet, Tz};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

macro_rules! generate_md5_id {
    ($($arg:expr),*) => {{
        let formatted = vec![$(format!("{}", $arg)),*].join("-");
        let digest = md5::compute(formatted);
        format!("{:x}", digest)[..8].to_string()
    }};
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Sprint {
    pub id: usize,
    pub name: String,
    pub state: SprintState,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub followed: bool,
    pub workdays: i64,
}

impl Identifiable for Sprint {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum SprintState {
    Active,
    Closed,
    Future,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Board {
    pub id: usize,
    pub name: String,
    pub board_type: BoardType,
    pub followed: bool,
    pub project_name: Option<String>,
}

impl Board {
    pub fn from_jira(jira_board: JiraBoard) -> Self {
        Self {
            id: jira_board.id.clone(),
            name: jira_board.name.clone(),
            board_type: BoardType::from_str(&jira_board.r#type),
            followed: false,
            project_name: jira_board.location.map(|l| l.project_name),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum BoardType {
    Scrum,
    Kanban,
    Simple,
    Unknown,
}

impl BoardType {
    pub fn from_str(str: &str) -> BoardType {
        match str {
            "scrum" => BoardType::Scrum,
            "kanban" => BoardType::Kanban,
            "simple" => BoardType::Simple,
            _ => BoardType::Unknown,
        }
    }
}

impl Identifiable for Board {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Worklog {
    pub id: String,
    pub author: String,
    pub created: DateTime<Utc>,
    pub time_spent: String,
    pub time_spent_seconds: u64,
    pub comment: Option<String>,
    pub issue_id: String,
    pub started: DateTime<Utc>,
}

impl Identifiable for Worklog {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub summary: String,
    pub status: String,
    pub created: DateTime<Utc>,
}

impl Identifiable for Issue {
    fn get_id(&self) -> String {
        self.key.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Absence {
    pub id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Identifiable for Absence {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Meeting {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub attendees: Option<Vec<Attendee>>,
    pub jira_link: Option<String>,
    #[serde(default)]
    pub recurrence: Option<Vec<String>>,
    pub logs: HashMap<NaiveDate, String>,
    #[serde(default)]
    pub my_response_status: Option<String>,
}

impl Identifiable for Meeting {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

impl Meeting {
    pub fn get_jira_candidates(&self) -> Vec<String> {
        let mut candidates: Vec<String> = vec![];
        candidates.extend(
            self.attendees
                .as_ref()
                .map(|list| {
                    list.iter()
                        .filter_map(|attendee| attendee.comment.clone())
                        .flat_map(|c| get_jira_identifiers(&c))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
        );
        if let Some(title) = self.title.clone() {
            candidates.extend(get_jira_identifiers(title.as_str()));
        }
        if let Some(description) = self.description.clone() {
            candidates.extend(get_jira_identifiers(description.as_str()));
        }
        candidates
    }

    pub fn is_on_day(&self, target_date: NaiveDate) -> bool {
        let start_of_day = Utc.from_utc_datetime(&target_date.and_hms_opt(0, 0, 0).unwrap());
        let end_of_day = Utc.from_utc_datetime(&target_date.and_hms_opt(23, 59, 59).unwrap());

        self.is_between(start_of_day, end_of_day)
    }

    fn get_recurrence_rule(&self) -> Option<RRuleSet> {
        let Some(rules) = self.recurrence.clone() else {
            return None;
        };
        let rrule_str = format!(
            "DTSTART:{}\n{}",
            self.start.format("%Y%m%dT%H%M%SZ"),
            rules.join("\n")
        );

        match rrule_str.parse() {
            Ok(result) => Some(result),
            Err(e) => {
                warn!(
                    "Failed to parse recurrence rule for meeting '{}' (ID: {}): {}. Rule: {}",
                    self.title.as_deref().unwrap_or("Unknown"),
                    self.id,
                    e,
                    rrule_str
                );
                None
            }
        }
    }

    pub fn get_start_for_day(&self, target_date: NaiveDate) -> Option<DateTime<Utc>> {
        let start_of_day = Utc.from_utc_datetime(&target_date.and_hms_opt(0, 0, 0).unwrap());
        let end_of_day = Utc.from_utc_datetime(&target_date.and_hms_opt(23, 59, 59).unwrap());
        if let Some(rrule) = self.get_recurrence_rule() {
            let start_tz = start_of_day.with_timezone(&Tz::UTC);
            let end_tz = end_of_day.with_timezone(&Tz::UTC);

            let occurrences = rrule.after(start_tz).before(end_tz).all(1);
            return occurrences
                .dates
                .into_iter()
                .next()
                .map(|dt| dt.with_timezone(&Utc));
        } else if start_of_day.date_naive() <= target_date && target_date <= end_of_day.date_naive()
        {
            return Some(start_of_day);
        }
        None
    }

    pub fn is_between(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> bool {
        if (self.start >= start && self.start <= end) || (self.end >= start && self.end <= end) {
            return true;
        }

        if let Some(rrule) = self.get_recurrence_rule() {
            let start_tz = start.with_timezone(&Tz::from(Utc));
            let end_tz = end.with_timezone(&Tz::from(Utc));

            let rrule = rrule.after(start_tz).before(end_tz);
            let result = rrule.all(100);
            if !result.dates.is_empty() {
                return true;
            }
        }

        false
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Attendee {
    pub display_name: Option<String>,
    pub comment: Option<String>,
    pub email: Option<String>,
}

impl Attendee {
    pub fn from_google(attendee: &EventAttendee) -> Attendee {
        Attendee {
            display_name: attendee.display_name.clone(),
            comment: attendee.comment.clone(),
            email: attendee.email.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalWorklog {
    pub id: String,
    pub comment: String,
    pub time_spent_seconds: i64,
    pub issue_id: String,
    pub status: LocalWorklogState,
    pub started: DateTime<Utc>,
    pub meeting_id: Option<String>, //FIXME: handle this kind of ref in a better way. We'll not have an Option<entity_id> for every possible cases
    pub worklog_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocalWorklogState {
    Created,
    Staged,
    Pushed,
}

impl Identifiable for LocalWorklog {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalWorklogHistory {
    pub id: String,
    pub date: DateTime<Utc>,
    pub local_worklogs_id: Vec<String>,
}

impl Identifiable for LocalWorklogHistory {
    fn get_id(&self) -> String {
        self.id.to_string()
    }
}

impl LocalWorklogHistory {
    pub fn new(date: DateTime<Utc>, local_worklogs_id: Vec<String>) -> LocalWorklogHistory {
        let id = generate_md5_id!(date, local_worklogs_id.join(","));
        LocalWorklogHistory {
            id,
            date,
            local_worklogs_id,
        }
    }
}

// GitHub Event models
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitHubEvent {
    pub id: String,
    pub event_type: String,
    pub repo: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub jira_issues: String, // Comma-separated
    pub date: NaiveDate,     // For quick date queries
}

impl Identifiable for GitHubEvent {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

impl GitHubEvent {
    /// Get Jira issues as a vector
    pub fn get_jira_issues(&self) -> Vec<String> {
        if self.jira_issues.is_empty() {
            Vec::new()
        } else {
            self.jira_issues
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitHubSession {
    pub id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_seconds: i64,
    pub repo: String,
    pub description: String,
    pub jira_issues: String, // Comma-separated
    pub event_ids: String,   // Comma-separated event IDs
    pub date: NaiveDate,     // For quick date queries
}

impl Identifiable for GitHubSession {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

impl GitHubSession {
    pub fn new(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        duration_seconds: i64,
        repo: String,
        description: String,
        jira_issues: Vec<String>,
        event_ids: Vec<String>,
    ) -> Self {
        let id = generate_md5_id!(repo, start_time, end_time);
        let date = start_time.date_naive();

        Self {
            id,
            start_time,
            end_time,
            duration_seconds,
            repo,
            description,
            jira_issues: jira_issues.join(","),
            event_ids: event_ids.join(","),
            date,
        }
    }

    pub fn get_jira_issues(&self) -> Vec<String> {
        if self.jira_issues.is_empty() {
            Vec::new()
        } else {
            self.jira_issues.split(',').map(|s| s.to_string()).collect()
        }
    }

    pub fn get_event_ids(&self) -> Vec<String> {
        if self.event_ids.is_empty() {
            Vec::new()
        } else {
            self.event_ids.split(',').map(|s| s.to_string()).collect()
        }
    }

    /// Get duration in hours
    pub fn duration_hours(&self) -> f64 {
        self.duration_seconds as f64 / 3600.0
    }
}
