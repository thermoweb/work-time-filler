use crate::client::github_client::{GitHubClient, GitHubEvent as APIGitHubEvent};
use crate::config::Config;
use crate::models::data::{GitHubEvent, GitHubSession, Sprint};
use crate::storage::database::{GenericDatabase, DATABASE};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use log::{debug, info, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;

static GITHUB_EVENTS_DB: Lazy<Arc<GenericDatabase<GitHubEvent>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "github_events")
            .expect("could not initialize github events database"),
    )
});

static GITHUB_SESSIONS_DB: Lazy<Arc<GenericDatabase<GitHubSession>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "github_sessions")
            .expect("could not initialize github sessions database"),
    )
});

pub struct GitHubService;

impl GitHubService {
    /// Check if GitHub CLI is available and configured
    pub fn is_configured() -> bool {
        GitHubClient::is_available()
    }

    /// Save a GitHub event to the database
    pub fn save_event(event: &GitHubEvent) {
        if let Err(e) = GITHUB_EVENTS_DB.insert(event) {
            warn!("Failed to save GitHub event '{}': {}", event.id, e);
        }
    }

    /// Save a GitHub session to the database
    pub fn save_session(session: &GitHubSession) {
        if let Err(e) = GITHUB_SESSIONS_DB.insert(session) {
            warn!("Failed to save GitHub session '{}': {}", session.id, e);
        }
    }

    /// Get all GitHub sessions
    pub fn get_all_sessions() -> Result<Vec<GitHubSession>, String> {
        GITHUB_SESSIONS_DB
            .get_all()
            .map_err(|e| format!("Failed to get sessions: {}", e))
    }

    /// Get GitHub sessions for a specific date
    pub fn get_sessions_by_date(date: NaiveDate) -> Result<Vec<GitHubSession>, String> {
        let all = GITHUB_SESSIONS_DB
            .get_all()
            .map_err(|e| format!("Failed to get sessions: {}", e))?;

        Ok(all.into_iter().filter(|s| s.date == date).collect())
    }

    /// Get GitHub sessions for a date range
    pub fn get_sessions_by_date_range(
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<GitHubSession>, String> {
        let all = GITHUB_SESSIONS_DB
            .get_all()
            .map_err(|e| format!("Failed to get sessions: {}", e))?;

        Ok(all
            .into_iter()
            .filter(|s| s.date >= from && s.date <= to)
            .collect())
    }

    /// Get all GitHub events
    pub fn get_all_events() -> Result<Vec<GitHubEvent>, String> {
        GITHUB_EVENTS_DB
            .get_all()
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// Get GitHub events for a specific date
    pub fn get_events_by_date(date: NaiveDate) -> Result<Vec<GitHubEvent>, String> {
        let all = GITHUB_EVENTS_DB
            .get_all()
            .map_err(|e| format!("Failed to get events: {}", e))?;

        Ok(all.into_iter().filter(|e| e.date == date).collect())
    }

    /// Fetch GitHub events for all followed sprints (backward compatibility - don't save to DB)
    pub fn fetch_events_for_sprints(sprints: &[Sprint]) -> Result<Vec<APIGitHubEvent>, String> {
        if !Self::is_configured() {
            return Err("GitHub CLI is not installed or configured".to_string());
        }

        let username = GitHubClient::get_username()?;
        let mut all_api_events = Vec::new();

        for sprint in sprints {
            let sprint_start = match sprint.start {
                Some(start) if start <= Utc::now() => start,
                _ => continue,
            };

            let sprint_end = sprint.end.unwrap_or_else(|| Utc::now());

            match GitHubClient::fetch_events(&username, sprint_start, sprint_end) {
                Ok(events) => {
                    all_api_events.extend(events);
                }
                Err(e) => {
                    warn!("Failed to fetch events for sprint {}: {}", sprint.name, e);
                }
            }
        }

        // Deduplicate events by ID
        let mut seen = std::collections::HashSet::new();
        all_api_events.retain(|e| seen.insert(e.id.clone()));

        // Apply organisation filter if configured
        if let Ok(config) = Config::load() {
            if let Some(ref org) = config.github.organisation {
                let prefix = format!("{}/", org);
                all_api_events.retain(|e| e.repo.name.starts_with(&prefix));
                debug!("After org filter '{}': {} events", org, all_api_events.len());
            }
        }

        Ok(all_api_events)
    }

    /// Fetch GitHub events for all followed sprints and save to database
    pub fn sync_events_for_sprints(sprints: &[Sprint]) -> Result<(usize, usize), String> {
        if !Self::is_configured() {
            return Err("GitHub CLI is not installed or configured. Please install gh CLI and run 'gh auth login'".to_string());
        }

        let username = GitHubClient::get_username()?;
        info!("Fetching GitHub events for user: {}", username);
        info!("Note: GitHub API only stores the last 90 days of events");

        let mut all_api_events = Vec::new();

        for sprint in sprints {
            // Skip future sprints or sprints without start date
            let sprint_start = match sprint.start {
                Some(start) if start <= Utc::now() => start,
                _ => continue,
            };

            let sprint_end = sprint.end.unwrap_or_else(|| Utc::now());

            debug!(
                "Fetching events for sprint {} ({} to {})",
                sprint.name, sprint_start, sprint_end
            );

            match GitHubClient::fetch_events(&username, sprint_start, sprint_end) {
                Ok(events) => {
                    info!("Found {} events for sprint {}", events.len(), sprint.name);
                    all_api_events.extend(events);
                }
                Err(e) => {
                    warn!("Failed to fetch events for sprint {}: {}", sprint.name, e);
                }
            }
        }

        // Deduplicate events by ID
        let mut seen = std::collections::HashSet::new();
        all_api_events.retain(|e| seen.insert(e.id.clone()));

        info!("Total GitHub events fetched: {}", all_api_events.len());

        // Apply organisation filter if configured
        if let Ok(config) = Config::load() {
            if let Some(ref org) = config.github.organisation {
                let prefix = format!("{}/", org);
                all_api_events.retain(|e| e.repo.name.starts_with(&prefix));
                info!("After org filter '{}': {} events remain", org, all_api_events.len());
            }
        }

        // Convert and save to database
        let mut events_saved = 0;
        for api_event in &all_api_events {
            let jira_issues = GitHubClient::extract_jira_issues(api_event);
            let description = GitHubClient::extract_description(api_event);

            let db_event = GitHubEvent {
                id: api_event.id.clone(),
                event_type: api_event.event_type.clone(),
                repo: api_event.repo.name.clone(),
                timestamp: api_event.created_at,
                description,
                jira_issues: jira_issues.join(","),
                date: api_event.created_at.date_naive(),
            };
            Self::save_event(&db_event);
            events_saved += 1;
        }

        // Calculate and save sessions
        let sessions = Self::calculate_and_save_sessions(&all_api_events);

        Ok((events_saved, sessions))
    }

    /// Calculate work sessions from API events and save to database
    fn calculate_and_save_sessions(api_events: &[APIGitHubEvent]) -> usize {
        let mut sessions_by_day: HashMap<String, Vec<TempSession>> = HashMap::new();

        for event in api_events {
            let date_key = event.created_at.format("%Y-%m-%d").to_string();

            let session = TempSession::new(
                event.id.clone(),
                event.event_type.clone(),
                event.created_at,
                event.repo.name.clone(),
                GitHubClient::extract_jira_issues(event),
                Self::get_event_description_from_api(event),
            );

            sessions_by_day
                .entry(date_key)
                .or_insert_with(Vec::new)
                .push(session);
        }

        // Merge nearby events into sessions (within 2 hours)
        let mut sessions_saved = 0;
        for (_date, sessions) in sessions_by_day.iter_mut() {
            sessions.sort_by(|a, b| a.start_time.cmp(&b.start_time));

            let mut merged: Vec<TempSession> = Vec::new();
            for session in sessions.drain(..) {
                if let Some(last) = merged.last_mut() {
                    // If within 2 hours, merge into existing session
                    if session.start_time.signed_duration_since(last.end_time) < Duration::hours(2)
                    {
                        last.end_time = session.end_time;
                        last.jira_issues.extend(session.jira_issues);
                        last.jira_issues.sort();
                        last.jira_issues.dedup();
                        last.event_ids.push(session.event_id);
                        if !session.description.is_empty() {
                            last.description
                                .push_str(&format!("; {}", session.description));
                        }
                        continue;
                    }
                }
                merged.push(session);
            }

            // Save merged sessions to database
            for temp_session in merged {
                let duration = temp_session
                    .end_time
                    .signed_duration_since(temp_session.start_time)
                    .num_seconds()
                    .max(15 * 60); // Minimum 15 minutes

                let db_session = GitHubSession::new(
                    temp_session.start_time,
                    temp_session.end_time,
                    duration,
                    temp_session.repo,
                    temp_session.description,
                    temp_session.jira_issues,
                    temp_session.event_ids,
                );

                Self::save_session(&db_session);
                sessions_saved += 1;
            }
        }

        sessions_saved
    }

    /// Group events by day and calculate work sessions (for backward compatibility)
    pub fn calculate_work_sessions(events: &[APIGitHubEvent]) -> HashMap<String, Vec<WorkSession>> {
        let mut sessions_by_day: HashMap<String, Vec<WorkSession>> = HashMap::new();

        for event in events {
            let date_key = event.created_at.format("%Y-%m-%d").to_string();

            let session = WorkSession {
                start_time: event.created_at,
                end_time: event.created_at,
                event_type: event.event_type.clone(),
                repo: event.repo.name.clone(),
                jira_issues: GitHubClient::extract_jira_issues(event),
                description: Self::get_event_description(event),
            };

            sessions_by_day
                .entry(date_key)
                .or_insert_with(Vec::new)
                .push(session);
        }

        // Merge nearby events into sessions (within 2 hours)
        for (_date, sessions) in sessions_by_day.iter_mut() {
            sessions.sort_by(|a, b| a.start_time.cmp(&b.start_time));

            let mut merged: Vec<WorkSession> = Vec::new();
            for session in sessions.drain(..) {
                if let Some(last) = merged.last_mut() {
                    // If within 2 hours, merge into existing session
                    if session.start_time.signed_duration_since(last.end_time) < Duration::hours(2)
                    {
                        last.end_time = session.end_time;
                        last.jira_issues.extend(session.jira_issues);
                        last.jira_issues.sort();
                        last.jira_issues.dedup();
                        if !session.description.is_empty() {
                            last.description
                                .push_str(&format!("; {}", session.description));
                        }
                        continue;
                    }
                }
                merged.push(session);
            }
            *sessions = merged;
        }

        sessions_by_day
    }

    /// Get a human-readable description of an event (for API events)
    fn get_event_description_from_api(event: &APIGitHubEvent) -> String {
        match event.event_type.as_str() {
            "PushEvent" => {
                if let Some(commits) = event.payload.get("commits").and_then(|c| c.as_array()) {
                    let count = commits.len();
                    if count == 1 {
                        if let Some(message) = commits[0].get("message").and_then(|m| m.as_str()) {
                            return format!("Pushed: {}", message.lines().next().unwrap_or(""));
                        }
                    }
                    return format!("Pushed {} commits", count);
                }
                "Code push".to_string()
            }
            "PullRequestEvent" => {
                if let Some(action) = event.payload.get("action").and_then(|a| a.as_str()) {
                    if let Some(pr) = event.payload.get("pull_request") {
                        if let Some(title) = pr.get("title").and_then(|t| t.as_str()) {
                            return format!("PR {}: {}", action, title);
                        }
                    }
                    return format!("PR {}", action);
                }
                "Pull request activity".to_string()
            }
            "PullRequestReviewEvent" | "PullRequestReviewCommentEvent" => "PR review".to_string(),
            "IssuesEvent" => {
                if let Some(action) = event.payload.get("action").and_then(|a| a.as_str()) {
                    return format!("Issue {}", action);
                }
                "Issue activity".to_string()
            }
            "IssueCommentEvent" => "Issue comment".to_string(),
            "CreateEvent" | "DeleteEvent" => {
                if let Some(ref_type) = event.payload.get("ref_type").and_then(|r| r.as_str()) {
                    return format!("Branch/tag {}", ref_type);
                }
                event.event_type.clone()
            }
            _ => event.event_type.clone(),
        }
    }

    /// Get a human-readable description of an event (deprecated - for backward compat)
    fn get_event_description(event: &APIGitHubEvent) -> String {
        Self::get_event_description_from_api(event)
    }
}

// Temporary session struct for merging
struct TempSession {
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    #[allow(dead_code)]
    event_type: String,
    repo: String,
    jira_issues: Vec<String>,
    description: String,
    event_id: String,
    event_ids: Vec<String>,
}

impl TempSession {
    fn new(
        event_id: String,
        event_type: String,
        start_time: DateTime<Utc>,
        repo: String,
        jira_issues: Vec<String>,
        description: String,
    ) -> Self {
        Self {
            start_time,
            end_time: start_time,
            event_type,
            repo,
            jira_issues,
            description,
            event_id: event_id.clone(),
            event_ids: vec![event_id],
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkSession {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub event_type: String,
    pub repo: String,
    pub jira_issues: Vec<String>,
    pub description: String,
}

impl WorkSession {
    /// Calculate duration in seconds (minimum 15 minutes)
    pub fn duration_seconds(&self) -> i64 {
        let duration = self
            .end_time
            .signed_duration_since(self.start_time)
            .num_seconds();
        // Minimum 15 minutes per session
        duration.max(15 * 60)
    }

    /// Get the most relevant Jira issue (first one found)
    pub fn primary_jira_issue(&self) -> Option<String> {
        self.jira_issues.first().cloned()
    }
}
