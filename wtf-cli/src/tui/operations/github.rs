// GitHub operations: sync events, create worklogs from sessions

use std::sync::mpsc::channel;
use std::thread;

use wtf_lib::models::data::GitHubSession;
use wtf_lib::services::worklogs_service::LocalWorklogService;

use crate::logger;

use super::super::{
    types::{FetchStatus, WorklogCreationConfirmation, WorklogSource},
    Tui,
};

impl Tui {
    pub(in crate::tui) fn handle_github_sync(&mut self) {
        // Don't start a new fetch if one is already in progress
        if matches!(self.fetch_status, FetchStatus::Fetching(_)) {
            return;
        }

        let (sender, receiver) = channel();
        self.fetch_receiver = Some(receiver);

        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                use wtf_lib::services::github_service::GitHubService;

                let _ = sender.send(FetchStatus::Fetching(
                    "Syncing GitHub events...".to_string(),
                ));

                // Check if GitHub CLI is configured
                if !GitHubService::is_configured() {
                    let _ =
                        sender.send(FetchStatus::Error("GitHub CLI not configured".to_string()));
                    return;
                }

                // Get followed sprints
                let sprints = wtf_lib::services::jira_service::JiraService::get_followed_sprint();
                if sprints.is_empty() {
                    let _ = sender.send(FetchStatus::Error("No followed sprints".to_string()));
                    return;
                }

                // Sync events and sessions
                match GitHubService::sync_events_for_sprints(&sprints) {
                    Ok((events_count, sessions_count)) => {
                        logger::log(format!(
                            "✅ Synced {} events, {} sessions",
                            events_count, sessions_count
                        ));
                        let _ = sender.send(FetchStatus::Complete);
                    }
                    Err(e) => {
                        logger::log(format!("❌ GitHub sync failed: {}", e));
                        let _ = sender.send(FetchStatus::Error(e));
                    }
                }
            });
        });
    }

    pub(in crate::tui) fn handle_create_worklog_from_session(&mut self) {
        // Get sessions in reverse chronological order (same as display)
        let mut sorted_sessions: Vec<_> = self.data.github_sessions.clone();
        sorted_sessions.sort_by(|a, b| b.start_time.cmp(&a.start_time));

        if let Some(session) = sorted_sessions
            .get(self.data.ui_state.selected_github_session_index)
            .cloned()
        {
            let jira_issues = session.get_jira_issues();

            if jira_issues.is_empty() {
                logger::log(
                    "❌ Cannot create worklog: No Jira issues found in this session".to_string(),
                );
                return;
            }

            // If multiple issues, we'll handle them one at a time
            // For now, let's handle the first issue with confirmation
            let issue_id = &jira_issues[0];

            // Check if issue exists
            if !self.data.issues_by_key.contains_key(issue_id) {
                logger::log(format!("⚠️  Issue {} not found in database", issue_id));
                return;
            }

            let duration_seconds = session.duration_seconds;
            let time_per_issue = if jira_issues.len() > 1 {
                duration_seconds / jira_issues.len() as i64
            } else {
                duration_seconds
            };

            let requested_hours = time_per_issue as f64 / 3600.0;
            let session_date = session.start_time.date_naive();
            let existing_hours = LocalWorklogService::calculate_daily_total(session_date);

            // Check if this would exceed daily limit
            if existing_hours + requested_hours > self.data.daily_hours_limit {
                // Show confirmation popup
                self.worklog_creation_confirmation = Some(WorklogCreationConfirmation {
                    source: WorklogSource::GitHub {
                        session_id: session.id.clone(),
                        description: session
                            .description
                            .split(';')
                            .next()
                            .unwrap_or("Development work")
                            .to_string(),
                    },
                    issue_id: issue_id.clone(),
                    date: session_date,
                    requested_hours,
                    existing_hours,
                    daily_limit: self.data.daily_hours_limit,
                    user_input: String::new(),
                });
                return;
            }

            // Below daily limit - create worklogs directly
            self.create_worklogs_from_session(&session, &jira_issues, time_per_issue);
        }
    }

    pub(in crate::tui) fn create_worklogs_from_session(
        &mut self,
        session: &GitHubSession,
        jira_issues: &[String],
        time_per_issue: i64,
    ) -> usize {
        let mut created_count = 0;

        for issue_id in jira_issues {
            // Check if issue exists
            if !self.data.issues_by_key.contains_key(issue_id) {
                logger::log(format!(
                    "⚠️  Skipping {}: Issue not found in database",
                    issue_id
                ));
                continue;
            }

            // Create worklog
            let comment = format!(
                "GitHub activity: {}",
                session
                    .description
                    .split(';')
                    .next()
                    .unwrap_or("Development work")
            );

            let worklog = LocalWorklogService::create_new_local_worklogs(
                session.start_time,
                time_per_issue,
                issue_id,
                Some(&comment),
                None,
            );

            // Track for wizard rollback
            if let Some(wizard) = &mut self.wizard_state {
                wizard
                    .rollback_log
                    .created_worklog_ids
                    .push(worklog.id.clone());
            }

            created_count += 1;
            logger::log(format!(
                "✅ Created worklog for {} ({:.1}h)",
                issue_id,
                time_per_issue as f64 / 3600.0
            ));
        }

        if created_count > 0 {
            logger::log(format!(
                "📝 Created {} worklog(s) from GitHub session",
                created_count
            ));
            self.refresh_data();
        } else {
            logger::log("⚠️  No worklogs created".to_string());
        }

        created_count
    }
}
