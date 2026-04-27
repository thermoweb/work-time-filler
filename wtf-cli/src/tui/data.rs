use chrono::{DateTime, NaiveDate, Utc};
use std::collections::{BTreeSet, HashMap};
use wtf_lib::config::Config;
use wtf_lib::models::achievement::AchievementUnlock;
use wtf_lib::models::data::{
    GitHubEvent, GitHubSession, Issue, LocalWorklog, LocalWorklogHistory, Meeting, Sprint,
};
use wtf_lib::services::github_service::GitHubService;
use wtf_lib::services::jira_service::{IssueService, JiraService};
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::{LocalWorklogService, WorklogsService};

/// State of a Jira issue title lookup for the Settings color label display.
#[derive(Debug, Clone)]
pub enum IssueTitleState {
    Loading,
    Found(String),
    NotFound,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GitHubIssueValidation {
    Cached,
    Remote,
    Missing,
}

/// Activity for a single day
#[derive(Debug, Clone)]
pub struct DayActivity {
    pub date: NaiveDate,
    pub hours: f64,
    pub is_absence: bool,
}

/// Statistics about meetings (only pending count needed for dashboard)
#[derive(Debug, Clone)]
pub struct MeetingStats {
    pub pending: usize,
}

/// UI state for tabs (selections, filters, expansions)
#[derive(Debug, Clone)]
pub struct TabUiState {
    pub selected_sprint_index: usize,
    pub selected_meeting_index: usize,
    pub selected_worklog_index: usize,
    pub selected_github_session_index: usize,
    pub selected_history_index: usize,
    pub expanded_history_ids: std::collections::HashSet<String>,
    pub filter_unlinked_only: bool,
    pub filter_staged_only: bool,
    pub achievements_scroll_offset: usize,
    // Settings tab state
    pub settings_selected_field: usize,
    pub settings_editing: bool,
    pub settings_input_buffer: String,
    pub settings_show_sensitive: std::collections::HashSet<usize>,
    pub settings_dirty: bool,
    pub settings_status: Option<String>,
    /// Cache of resolved Jira issue titles for color label lookups.
    /// Key: issue ID (e.g. "PROJ-123"), Value: current lookup state.
    /// Missing from map = not yet requested; "notrack" is never added.
    pub settings_color_issue_titles: HashMap<String, IssueTitleState>,
}

impl Default for TabUiState {
    fn default() -> Self {
        Self {
            selected_sprint_index: 0,
            selected_meeting_index: 0,
            selected_worklog_index: 0,
            selected_github_session_index: 0,
            selected_history_index: 0,
            expanded_history_ids: std::collections::HashSet::new(),
            filter_unlinked_only: false,
            filter_staged_only: false,
            achievements_scroll_offset: 0,
            settings_selected_field: 0,
            settings_editing: false,
            settings_input_buffer: String::new(),
            settings_show_sensitive: std::collections::HashSet::new(),
            settings_dirty: false,
            settings_status: None,
            settings_color_issue_titles: HashMap::new(),
        }
    }
}

/// All data needed for dashboard display
#[derive(Debug, Clone)]
pub struct TuiData {
    pub all_sprints: Vec<Sprint>,
    pub all_meetings: Vec<Meeting>,
    pub untracked_meeting_ids: std::collections::HashSet<String>,
    pub all_worklogs: Vec<LocalWorklog>,
    /// Worklogs synced from Jira (used to surface entries not tracked locally)
    pub jira_worklogs: Vec<wtf_lib::models::data::Worklog>,
    pub worklog_history: Vec<LocalWorklogHistory>,
    pub github_sessions: Vec<GitHubSession>,
    pub github_events_by_id: HashMap<String, GitHubEvent>,
    pub github_issue_validations: HashMap<String, GitHubIssueValidation>,
    pub issues_by_key: HashMap<String, Issue>,
    pub meeting_stats: MeetingStats,
    pub sprint_activities: HashMap<usize, Vec<DayActivity>>,
    pub worklog_wall: Vec<DayActivity>,
    pub last_sync: DateTime<Utc>,
    pub daily_hours_limit: f64,
    pub config: Config,
    pub ui_state: TabUiState,
    pub unlocked_achievements: Vec<AchievementUnlock>,
}

impl TuiData {
    /// Collect all dashboard data from services
    pub fn collect() -> Self {
        Self::collect_with_ui_state(TabUiState::default())
    }

    /// Collect data while preserving existing UI state
    pub fn collect_with_ui_state(ui_state: TabUiState) -> Self {
        let config = Config::load().unwrap_or_default();
        let sprints = JiraService::production().get_followed_sprint();

        // Get meetings and GitHub sessions for followed sprints only
        let all_meetings = Self::get_meetings_for_sprints(&sprints);
        let github_sessions = Self::get_github_sessions_for_sprints(&sprints);
        let github_events = Self::get_github_events_for_sprints(&sprints);

        let all_worklogs = LocalWorklogService::production().get_all_local_worklogs();
        let jira_worklogs = WorklogsService::production().get_all_worklogs();
        let worklog_history = LocalWorklogService::production().get_history();
        let untracked_meeting_ids = MeetingsService::production().get_all_untracked_ids();

        // Build issue lookup map
        let all_issues = IssueService::production().get_all_issues();
        let mut issues_by_key: HashMap<String, Issue> = all_issues
            .into_iter()
            .map(|issue| (issue.key.clone(), issue))
            .collect();
        let github_issue_validations = Self::resolve_github_issue_validations(
            &github_sessions,
            &github_events,
            &mut issues_by_key,
        );
        let github_sessions = Self::filter_github_sessions_with_usable_issues(
            github_sessions,
            &github_issue_validations,
        );
        let github_events_by_id = github_events
            .into_iter()
            .map(|event| (event.id.clone(), event))
            .collect();

        let meeting_stats =
            Self::calculate_meeting_stats(&all_meetings, &config, &untracked_meeting_ids);
        let sprint_activities = Self::calculate_all_sprint_activities(&sprints);
        let worklog_wall = Self::calculate_worklog_wall();
        let unlocked_achievements =
            wtf_lib::services::achievement_service::AchievementService::production()
                .get_all_unlocked();

        TuiData {
            all_sprints: sprints,
            all_meetings,
            untracked_meeting_ids,
            all_worklogs,
            jira_worklogs,
            worklog_history,
            github_sessions,
            github_events_by_id,
            github_issue_validations,
            issues_by_key,
            meeting_stats,
            sprint_activities,
            worklog_wall,
            last_sync: Utc::now(),
            daily_hours_limit: config.worklog.daily_hours_limit,
            config,
            ui_state,
            unlocked_achievements,
        }
    }

    fn get_meetings_for_sprints(sprints: &[Sprint]) -> Vec<Meeting> {
        let all_meetings = MeetingsService::production().get_all_meetings();
        all_meetings
            .into_iter()
            .filter(|meeting| {
                sprints
                    .iter()
                    .any(|sprint| sprint.contains_meeting(meeting))
            })
            .collect()
    }

    fn get_github_sessions_for_sprints(sprints: &[Sprint]) -> Vec<GitHubSession> {
        let all_sessions = GitHubService::production()
            .get_all_sessions()
            .unwrap_or_default();

        // Filter sessions that fall within sprint date ranges
        all_sessions
            .into_iter()
            .filter(|session| {
                sprints.iter().any(|sprint| {
                    if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                        let session_datetime = session.start_time;
                        session_datetime >= start && session_datetime <= end
                    } else {
                        false
                    }
                })
            })
            .collect()
    }

    fn get_github_events_for_sprints(sprints: &[Sprint]) -> Vec<GitHubEvent> {
        let all_events = GitHubService::production()
            .get_all_events()
            .unwrap_or_default();

        all_events
            .into_iter()
            .filter(|event| {
                sprints.iter().any(|sprint| {
                    if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                        event.timestamp >= start && event.timestamp <= end
                    } else {
                        false
                    }
                })
            })
            .collect()
    }

    fn resolve_github_issue_validations(
        github_sessions: &[GitHubSession],
        github_events: &[GitHubEvent],
        issues_by_key: &mut HashMap<String, Issue>,
    ) -> HashMap<String, GitHubIssueValidation> {
        let mut validations = HashMap::new();
        let mut missing_keys = Vec::new();

        for key in Self::collect_detected_github_issue_keys(github_sessions, github_events) {
            if issues_by_key.contains_key(&key) {
                validations.insert(key, GitHubIssueValidation::Cached);
            } else {
                missing_keys.push(key);
            }
        }

        if missing_keys.is_empty() {
            return validations;
        }

        let fetched_issues = std::thread::spawn(move || {
            let jira_service = JiraService::production();
            let runtime = tokio::runtime::Runtime::new()
                .expect("failed to create runtime for GitHub Jira issue validation");

            missing_keys
                .into_iter()
                .map(|key| {
                    let issue = runtime.block_on(jira_service.get_issue_by_key(&key));
                    (key, issue)
                })
                .collect::<Vec<_>>()
        })
        .join()
        .expect("GitHub Jira validation worker thread panicked");

        for (key, issue) in fetched_issues {
            if let Some(issue) = issue {
                issues_by_key.insert(issue.key.clone(), issue);
                validations.insert(key, GitHubIssueValidation::Remote);
            } else {
                validations.insert(key, GitHubIssueValidation::Missing);
            }
        }

        validations
    }

    fn filter_github_sessions_with_usable_issues(
        github_sessions: Vec<GitHubSession>,
        validations: &HashMap<String, GitHubIssueValidation>,
    ) -> Vec<GitHubSession> {
        github_sessions
            .into_iter()
            .filter(|session| {
                let issue_keys = session.get_jira_issues();
                issue_keys.is_empty()
                    || issue_keys.iter().any(|key| {
                        !matches!(validations.get(key), Some(GitHubIssueValidation::Missing))
                    })
            })
            .collect()
    }

    fn collect_detected_github_issue_keys(
        github_sessions: &[GitHubSession],
        github_events: &[GitHubEvent],
    ) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();

        for session in github_sessions {
            keys.extend(session.get_jira_issues());
        }

        for event in github_events {
            keys.extend(event.get_jira_issues());
        }

        keys
    }

    pub fn valid_github_issues_for_session(&self, session: &GitHubSession) -> Vec<String> {
        session
            .get_jira_issues()
            .into_iter()
            .filter(|key| {
                !matches!(
                    self.github_issue_validations.get(key),
                    Some(GitHubIssueValidation::Missing)
                )
            })
            .collect()
    }

    fn calculate_meeting_stats(
        meetings: &[Meeting],
        config: &wtf_lib::config::Config,
        untracked_ids: &std::collections::HashSet<String>,
    ) -> MeetingStats {
        use wtf_lib::utils::meetings::is_untracked;
        let pending = meetings
            .iter()
            .filter(|m| m.jira_link.is_none() && !is_untracked(m, config, untracked_ids))
            .count();
        MeetingStats { pending }
    }

    fn calculate_all_sprint_activities(sprints: &[Sprint]) -> HashMap<usize, Vec<DayActivity>> {
        let mut activities = HashMap::new();

        for sprint in sprints {
            let activity = Self::calculate_sprint_activity(sprint);
            activities.insert(sprint.id, activity);
        }

        activities
    }

    fn calculate_sprint_activity(sprint: &Sprint) -> Vec<DayActivity> {
        use chrono::Datelike;
        use std::collections::HashSet;

        // Get both local worklogs AND Jira worklogs
        let local_worklogs = LocalWorklogService::production().get_all_local_worklogs();
        let jira_worklogs = WorklogsService::production().get_all_worklogs();

        // Get sprint date range
        let (start_date, end_date) = match (sprint.start, sprint.end) {
            (Some(start), Some(end)) => (start.date_naive(), end.date_naive()),
            _ => return vec![],
        };

        // Collect all Jira worklog IDs for deduplication
        let jira_worklog_ids: HashSet<String> =
            jira_worklogs.iter().map(|w| w.id.clone()).collect();

        // Group by date - start with Jira worklogs (authoritative source)
        let mut daily_hours: HashMap<NaiveDate, f64> = HashMap::new();

        // First, add all Jira worklogs (these are the source of truth from Jira)
        for worklog in jira_worklogs.iter() {
            let worklog_date = worklog.started.date_naive();
            if worklog_date >= start_date && worklog_date <= end_date {
                let hours = worklog.time_spent_seconds as f64 / 3600.0;
                *daily_hours.entry(worklog_date).or_insert(0.0) += hours;
            }
        }

        // Then add local worklogs that are NOT already in Jira
        // Deduplicate by checking if worklog_id exists in Jira
        for local_wl in local_worklogs.iter() {
            let worklog_date = local_wl.started.date_naive();
            if worklog_date >= start_date && worklog_date <= end_date {
                // Skip if this local worklog has been pushed to Jira (has worklog_id that matches)
                if let Some(ref worklog_id) = local_wl.worklog_id {
                    if jira_worklog_ids.contains(worklog_id) {
                        continue; // Already counted from Jira
                    }
                }

                // Count this local worklog (either not pushed, or push failed)
                let hours = local_wl.time_spent_seconds as f64 / 3600.0;
                *daily_hours.entry(worklog_date).or_insert(0.0) += hours;
            }
        }

        // Mark absence days
        let mut absence_days: HashMap<NaiveDate, bool> = HashMap::new();
        let absences = MeetingsService::production().get_absences();

        for absence in absences {
            let mut current_date = absence.start.date_naive();
            let end_date_abs = absence.end.date_naive();

            while current_date <= end_date_abs {
                let weekday = current_date.weekday().num_days_from_monday();
                if weekday < 5 && current_date >= start_date && current_date <= end_date {
                    absence_days.insert(current_date, true);
                    // Add 7 hours for absence if no work logged
                    daily_hours.entry(current_date).or_insert(7.0);
                }
                current_date = current_date.succ_opt().unwrap_or(current_date);
            }
        }

        // Generate full date range from start to end, filling gaps with 0 hours
        let mut activities = Vec::new();
        let mut current_date = start_date;

        while current_date <= end_date {
            let hours = daily_hours.get(&current_date).copied().unwrap_or(0.0);
            let is_absence = absence_days.contains_key(&current_date);
            activities.push(DayActivity {
                date: current_date,
                hours,
                is_absence,
            });
            current_date = current_date.succ_opt().unwrap_or(current_date);
        }

        activities
    }

    /// Calculate worklog wall data - last 365 days (full year) of daily activity
    fn calculate_worklog_wall() -> Vec<DayActivity> {
        use chrono::{Datelike, Duration, Local};
        use std::collections::HashSet;

        let today = Local::now().date_naive();

        // Find the Monday of the current week
        let days_from_monday = today.weekday().num_days_from_monday();
        let this_monday = today - Duration::days(days_from_monday as i64);

        // Go back 365 days and find that Monday
        let approx_start = this_monday - Duration::days(365);
        let start_days_from_monday = approx_start.weekday().num_days_from_monday();
        let start_monday = approx_start - Duration::days(start_days_from_monday as i64);

        // End on the Sunday of the current week
        let this_sunday = this_monday + Duration::days(6);

        // Get both local worklogs AND Jira worklogs
        let local_worklogs = LocalWorklogService::production().get_all_local_worklogs();
        let jira_worklogs = WorklogsService::production().get_all_worklogs();

        // Collect all Jira worklog IDs for deduplication
        let jira_worklog_ids: HashSet<String> =
            jira_worklogs.iter().map(|w| w.id.clone()).collect();

        // Group by date - start with Jira worklogs (authoritative source)
        let mut daily_hours: HashMap<NaiveDate, f64> = HashMap::new();

        // First, add all Jira worklogs
        for worklog in jira_worklogs.iter() {
            let worklog_date = worklog.started.date_naive();
            if worklog_date >= start_monday && worklog_date <= this_sunday {
                let hours = worklog.time_spent_seconds as f64 / 3600.0;
                *daily_hours.entry(worklog_date).or_insert(0.0) += hours;
            }
        }

        // Then add local worklogs that are NOT already in Jira
        for local_wl in local_worklogs.iter() {
            let worklog_date = local_wl.started.date_naive();
            if worklog_date >= start_monday && worklog_date <= this_sunday {
                // Skip if this local worklog has been pushed to Jira (has worklog_id that matches)
                if let Some(ref worklog_id) = local_wl.worklog_id {
                    if jira_worklog_ids.contains(worklog_id) {
                        continue; // Already counted from Jira
                    }
                }

                // Count this local worklog (either not pushed, or push failed)
                let hours = local_wl.time_spent_seconds as f64 / 3600.0;
                *daily_hours.entry(worklog_date).or_insert(0.0) += hours;
            }
        }

        // Mark absence days and add as 7-hour days (typical work day)
        let mut absence_days: HashMap<NaiveDate, bool> = HashMap::new();
        let absences = MeetingsService::production().get_absences();
        for absence in absences {
            let mut current_date = absence.start.date_naive();
            let end_date = absence.end.date_naive();

            while current_date <= end_date {
                // Only count weekdays (Mon-Fri) as absences
                let weekday = current_date.weekday().num_days_from_monday();
                if weekday < 5 && current_date >= start_monday && current_date <= this_sunday {
                    absence_days.insert(current_date, true);
                    daily_hours.entry(current_date).or_insert(7.0);
                }
                current_date = current_date.succ_opt().unwrap_or(current_date);
            }
        }

        // Generate full date range from start_monday to this_sunday
        let mut activities = Vec::new();
        let mut current_date = start_monday;

        while current_date <= this_sunday {
            let hours = daily_hours.get(&current_date).copied().unwrap_or(0.0);
            let is_absence = absence_days.contains_key(&current_date);
            activities.push(DayActivity {
                date: current_date,
                hours,
                is_absence,
            });
            current_date = current_date.succ_opt().unwrap_or(current_date);
        }

        activities
    }
}

#[cfg(test)]
mod tests {
    use super::GitHubIssueValidation;
    use super::TuiData;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;
    use wtf_lib::models::data::{GitHubEvent, GitHubSession};

    #[test]
    fn collect_detected_github_issue_keys_deduplicates_session_and_event_keys() {
        let session = GitHubSession {
            id: "session".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 16, 9, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 3, 16, 10, 0, 0).unwrap(),
            duration_seconds: 3600,
            repo: "org/repo".to_string(),
            description: "Pushed: PAT-11".to_string(),
            jira_issues: "PAT-11,APP-7".to_string(),
            event_ids: "evt-1,evt-2".to_string(),
            date: Utc
                .with_ymd_and_hms(2026, 3, 16, 9, 0, 0)
                .unwrap()
                .date_naive(),
        };
        let event = GitHubEvent {
            id: "evt-1".to_string(),
            event_type: "PushEvent".to_string(),
            repo: "org/repo".to_string(),
            timestamp: Utc.with_ymd_and_hms(2026, 3, 16, 9, 30, 0).unwrap(),
            description: "Pushed: PAT-11 and WEB-3".to_string(),
            jira_issues: "PAT-11,WEB-3".to_string(),
            date: Utc
                .with_ymd_and_hms(2026, 3, 16, 9, 30, 0)
                .unwrap()
                .date_naive(),
        };

        let keys = TuiData::collect_detected_github_issue_keys(&[session], &[event]);

        assert_eq!(
            keys.into_iter().collect::<Vec<_>>(),
            vec![
                "APP-7".to_string(),
                "PAT-11".to_string(),
                "WEB-3".to_string()
            ]
        );
    }

    #[test]
    fn filter_github_sessions_with_usable_issues_removes_missing_only_sessions() {
        let missing_only = GitHubSession {
            id: "missing-only".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 16, 9, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 3, 16, 9, 15, 0).unwrap(),
            duration_seconds: 900,
            repo: "org/repo".to_string(),
            description: String::new(),
            jira_issues: "MISS-1".to_string(),
            event_ids: String::new(),
            date: Utc
                .with_ymd_and_hms(2026, 3, 16, 9, 0, 0)
                .unwrap()
                .date_naive(),
        };
        let mixed = GitHubSession {
            id: "mixed".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 16, 10, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 3, 16, 10, 30, 0).unwrap(),
            duration_seconds: 1800,
            repo: "org/repo".to_string(),
            description: String::new(),
            jira_issues: "MISS-1,OK-2".to_string(),
            event_ids: String::new(),
            date: Utc
                .with_ymd_and_hms(2026, 3, 16, 10, 0, 0)
                .unwrap()
                .date_naive(),
        };

        let mut validations = HashMap::new();
        validations.insert("MISS-1".to_string(), GitHubIssueValidation::Missing);
        validations.insert("OK-2".to_string(), GitHubIssueValidation::Remote);

        let filtered = TuiData::filter_github_sessions_with_usable_issues(
            vec![missing_only, mixed.clone()],
            &validations,
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, mixed.id);
    }
}
