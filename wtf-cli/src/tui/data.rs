use chrono::{DateTime, NaiveDate, Utc};
use std::collections::HashMap;
use wtf_lib::config::Config;
use wtf_lib::models::data::{
    GitHubSession, Issue, LocalWorklog, LocalWorklogHistory, Meeting, Sprint,
};
use wtf_lib::services::github_service::GitHubService;
use wtf_lib::services::jira_service::{IssueService, JiraService};
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::{LocalWorklogService, WorklogsService};

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
        }
    }
}

/// All data needed for dashboard display
#[derive(Debug, Clone)]
pub struct TuiData {
    pub all_sprints: Vec<Sprint>,
    pub all_meetings: Vec<Meeting>,
    pub all_worklogs: Vec<LocalWorklog>,
    pub worklog_history: Vec<LocalWorklogHistory>,
    pub github_sessions: Vec<GitHubSession>,
    pub issues_by_key: HashMap<String, Issue>,
    pub meeting_stats: MeetingStats,
    pub sprint_activities: HashMap<usize, Vec<DayActivity>>,
    pub worklog_wall: Vec<DayActivity>,
    pub last_sync: DateTime<Utc>,
    pub daily_hours_limit: f64,
    pub config: Config,
    pub ui_state: TabUiState,
}

impl TuiData {
    /// Collect all dashboard data from services
    pub fn collect() -> Self {
        Self::collect_with_ui_state(TabUiState::default())
    }

    /// Collect data while preserving existing UI state
    pub fn collect_with_ui_state(ui_state: TabUiState) -> Self {
        let config = Config::load().unwrap_or_default();
        let sprints = JiraService::get_followed_sprint();

        // Get meetings and GitHub sessions for followed sprints only
        let all_meetings = Self::get_meetings_for_sprints(&sprints);
        let github_sessions = Self::get_github_sessions_for_sprints(&sprints);

        let all_worklogs = LocalWorklogService::get_all_local_worklogs();
        let worklog_history = LocalWorklogService::get_history();

        // Build issue lookup map
        let all_issues = IssueService::get_all_issues();
        let issues_by_key: HashMap<String, Issue> = all_issues
            .into_iter()
            .map(|issue| (issue.key.clone(), issue))
            .collect();

        let meeting_stats = Self::calculate_meeting_stats(&all_meetings);
        let sprint_activities = Self::calculate_all_sprint_activities(&sprints);
        let worklog_wall = Self::calculate_worklog_wall();

        TuiData {
            all_sprints: sprints,
            all_meetings,
            all_worklogs,
            worklog_history,
            github_sessions,
            issues_by_key,
            meeting_stats,
            sprint_activities,
            worklog_wall,
            last_sync: Utc::now(),
            daily_hours_limit: config.worklog.daily_hours_limit,
            config,
            ui_state,
        }
    }

    fn get_meetings_for_sprints(sprints: &[Sprint]) -> Vec<Meeting> {
        let all_meetings = MeetingsService::get_all_meetings();

        // Filter meetings that fall within sprint date ranges
        all_meetings
            .into_iter()
            .filter(|meeting| {
                // Check if meeting is within any sprint date range
                sprints.iter().any(|sprint| {
                    if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                        let meeting_start = meeting.start;
                        meeting_start >= start && meeting_start <= end
                    } else {
                        false
                    }
                })
            })
            .collect()
    }

    fn get_github_sessions_for_sprints(sprints: &[Sprint]) -> Vec<GitHubSession> {
        let all_sessions = GitHubService::get_all_sessions().unwrap_or_default();

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

    fn calculate_meeting_stats(meetings: &[Meeting]) -> MeetingStats {
        let pending = meetings.iter().filter(|m| m.jira_link.is_none()).count();

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
        let local_worklogs = LocalWorklogService::get_all_local_worklogs();
        let jira_worklogs = WorklogsService::get_all_worklogs();

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
        let absences = MeetingsService::get_absences();

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
        let local_worklogs = LocalWorklogService::get_all_local_worklogs();
        let jira_worklogs = WorklogsService::get_all_worklogs();

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
        let absences = MeetingsService::get_absences();
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
