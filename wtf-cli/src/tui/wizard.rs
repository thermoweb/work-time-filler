// Wizard workflow implementation for guided worklog creation

use std::collections::HashMap;

use wtf_lib::models::data::LocalWorklogState;
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::LocalWorklogService;

use crate::logger;

use super::types::*;
use super::{get_branding_text, Tui};

impl Tui {
    pub(super) fn launch_wizard(&mut self) {
        // Get the selected sprint
        if let Some(sprint) = self.data.all_sprints.get(self.data.ui_state.selected_sprint_index) {
            // Check if sprint has date range
            if sprint.start.is_none() || sprint.end.is_none() {
                logger::log("⚠️  Chronie says: Sprint has no date range!".to_string());
                return;
            }

            logger::log(format!(
                "🧙 Chronie is starting the wizard for {}...",
                sprint.name
            ));

            // Initialize wizard state
            self.wizard_state = Some(WizardState {
                sprint_id: sprint.id,
                sprint_name: sprint.name.clone(),
                current_step: WizardStep::Syncing,
                completed_steps: std::collections::HashSet::new(),
                summary: WizardSummary::default(),
                rollback_log: WizardRollbackLog::default(),
                skip_reasons: HashMap::new(),
                push_logs: Vec::new(),
                spinner_frame: 0,
                push_current: 0,
                push_total: 0,
                startup_message: get_branding_text("startup"), // Set once at wizard start
            });

            // Start the first step (syncing)
            self.wizard_step_sync();
        }
    }

    // Wizard step implementations
    pub(super) fn wizard_step_sync(&mut self) {
        logger::log("📡 Step 1/7: Syncing data...".to_string());
        self.handle_update();
        // The update will complete asynchronously
        // We'll advance to next step when fetch_status becomes Idle
    }

    pub(super) fn wizard_step_autolink(&mut self) {
        use regex::Regex;

        logger::log("🔗 Step 1/7: Auto-linking meetings...".to_string());

        let jira_regex = Regex::new(r"([A-Z]+-\d+)").unwrap();
        let mut linked_count = 0;

        if let Some(wizard) = &mut self.wizard_state {
            // Get all unlinked meetings
            let unlinked_meetings: Vec<_> = self
                .data
                .all_meetings
                .iter()
                .filter(|m| m.jira_link.is_none())
                .collect();

            for meeting in unlinked_meetings {
                // Try to extract Jira issue key from title or description
                let empty_string = String::new();
                let title = meeting.title.as_ref().unwrap_or(&empty_string);
                let description = meeting.description.as_ref().unwrap_or(&empty_string);
                let combined = format!("{} {}", title, description);

                if let Some(captures) = jira_regex.captures(&combined) {
                    if let Some(issue_key) = captures.get(1) {
                        let key = issue_key.as_str().to_string();

                        // Check if issue exists in our data
                        if self.data.issues_by_key.contains_key(&key) {
                            // Link the meeting by updating jira_link field
                            if let Some(mut meeting) =
                                MeetingsService::get_meeting_by_id(meeting.id.clone())
                            {
                                meeting.jira_link = Some(key.clone());
                                MeetingsService::save(&meeting);

                                // Track for rollback
                                wizard
                                    .rollback_log
                                    .original_meeting_links
                                    .insert(meeting.id.clone(), None);
                                wizard
                                    .rollback_log
                                    .linked_meeting_ids
                                    .push(meeting.id.clone());
                                linked_count += 1;
                            }
                        }
                    }
                }
            }

            wizard.summary.meetings_auto_linked = linked_count;
            wizard.completed_steps.insert(1); // Step 1 (sync) complete

            logger::log(format!("✅ Auto-linked {} meetings", linked_count));

            // Move to next step
            self.wizard_advance_to_manual_linking();
        }
    }

    pub(super) fn wizard_advance_to_manual_linking(&mut self) {
        // Refresh data to see the newly linked meetings
        self.refresh_data();

        // Get unlinked meetings for manual linking step
        let unlinked_meetings: Vec<_> = self
            .data
            .all_meetings
            .iter()
            .filter(|m| m.jira_link.is_none())
            .cloned()
            .collect();

        if unlinked_meetings.is_empty() {
            // No meetings to link, skip to next step
            logger::log("ℹ️  Step 2/7: No unlinked meetings, skipping manual linking".to_string());
            if let Some(wizard) = &mut self.wizard_state {
                wizard.completed_steps.insert(2); // Step 2 complete
                self.wizard_step_create_meeting_worklogs();
            }
        } else {
            logger::log(format!(
                "📝 Step 2/7: {} unlinked meetings - link manually or skip (S)",
                unlinked_meetings.len()
            ));

            if let Some(wizard) = &mut self.wizard_state {
                wizard.current_step = WizardStep::ManualLinking {
                    unlinked_meetings,
                    selected_index: 0,
                };
            }
        }
    }

    pub(super) fn wizard_step_create_meeting_worklogs(&mut self) {
        logger::log("📅 Step 3/7: Creating worklogs from meetings...".to_string());

        if let Some(wizard) = &self.wizard_state {
            let sprint = self
                .data
                .all_sprints
                .iter()
                .find(|s| s.id == wizard.sprint_id);

            if let Some(sprint) = sprint {
                if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                    // Get all linked meetings in sprint date range
                    let meetings_to_log: Vec<_> = self
                        .data
                        .all_meetings
                        .iter()
                        .filter(|m| m.jira_link.is_some() && m.start >= start && m.start <= end)
                        .cloned()
                        .collect();

                    let count = meetings_to_log.len();

                    // Create worklogs
                    for meeting in meetings_to_log {
                        if let Some(issue_key) = &meeting.jira_link {
                            self.create_worklog_from_meeting(&meeting, issue_key);
                        }
                    }

                    logger::log(format!("✅ Created worklogs from {} meetings", count));
                }
            }
        }

        // Refresh and advance
        self.refresh_data();
        if let Some(wizard) = &mut self.wizard_state {
            wizard.completed_steps.insert(3); // Step 3 complete
            wizard.current_step = WizardStep::CreatingGitHubWorklogs {
                sessions: vec![],
                current_session_index: 0,
            };
        }

        self.wizard_step_create_github_worklogs();
    }

    pub(super) fn wizard_step_create_github_worklogs(&mut self) {
        logger::log("💻 Step 4/7: Creating worklogs from GitHub sessions...".to_string());

        // Get sessions from wizard state or initialize
        let should_initialize = if let Some(wizard) = &self.wizard_state {
            matches!(wizard.current_step, WizardStep::CreatingGitHubWorklogs { ref sessions, .. } if sessions.is_empty())
        } else {
            false
        };

        if should_initialize {
            // Get sprint date range
            if let Some(wizard) = &self.wizard_state {
                if let Some(sprint) = self
                    .data
                    .all_sprints
                    .iter()
                    .find(|s| s.id == wizard.sprint_id)
                {
                    if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                        // Filter GitHub sessions to this sprint's date range
                        let sprint_sessions: Vec<_> = self
                            .data
                            .github_sessions
                            .iter()
                            .filter(|s| {
                                let session_date = s.start_time.date_naive();
                                session_date >= start.date_naive()
                                    && session_date <= end.date_naive()
                                    && !s.get_jira_issues().is_empty() // Only sessions with Jira issues
                            })
                            .cloned()
                            .collect();

                        let count = sprint_sessions.len();
                        logger::log(format!("📊 Found {} GitHub sessions in sprint", count));

                        if count == 0 {
                            // No sessions, skip to next step
                            if let Some(wizard) = &mut self.wizard_state {
                                wizard.completed_steps.insert(4); // Step 4 complete (no sessions)
                                wizard
                                    .skip_reasons
                                    .insert(4, "no GitHub sessions found".to_string());
                                wizard.current_step = WizardStep::FillingGaps {
                                    selected_issue: None,
                                };
                            }
                            self.wizard_step_fill_gaps();
                            return;
                        }

                        // Initialize the sessions list
                        if let Some(wizard) = &mut self.wizard_state {
                            wizard.current_step = WizardStep::CreatingGitHubWorklogs {
                                sessions: sprint_sessions,
                                current_session_index: 0,
                            };
                        }
                    }
                }
            }
        }

        // Process current session
        self.wizard_process_next_github_session();
    }

    pub(super) fn wizard_process_next_github_session(&mut self) {
        let (session, current_index, total_count) = if let Some(wizard) = &self.wizard_state {
            if let WizardStep::CreatingGitHubWorklogs {
                ref sessions,
                current_session_index,
            } = wizard.current_step
            {
                if current_session_index >= sessions.len() {
                    // All sessions processed, advance to next step
                    if let Some(wizard) = &mut self.wizard_state {
                        wizard.completed_steps.insert(4); // Step 4 complete
                        wizard.current_step = WizardStep::FillingGaps {
                            selected_issue: None,
                        };
                    }
                    self.wizard_step_fill_gaps();
                    return;
                }

                let session = sessions[current_session_index].clone();
                (session, current_session_index, sessions.len())
            } else {
                return;
            }
        } else {
            return;
        };

        logger::log(format!(
            "💻 Processing GitHub session {}/{}",
            current_index + 1,
            total_count
        ));

        // Get Jira issues from session
        let jira_issues = session.get_jira_issues();

        // Calculate time per issue
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
            let first_issue = jira_issues
                .first()
                .map(|s| s.to_string())
                .unwrap_or_default();
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
                issue_id: first_issue,
                date: session_date,
                requested_hours,
                existing_hours,
                daily_limit: self.data.daily_hours_limit,
                user_input: String::new(),
            });
        } else {
            // Below daily limit - create worklogs directly
            let created = self.create_worklogs_from_session(&session, &jira_issues, time_per_issue);

            // Track in wizard
            if let Some(wizard) = &mut self.wizard_state {
                wizard.summary.worklogs_from_github += created;
                wizard.summary.total_hours += (time_per_issue * created as i64) as f64 / 3600.0;
            }

            // Advance to next session
            self.wizard_advance_github_session();
        }
    }

    pub(super) fn wizard_advance_github_session(&mut self) {
        if let Some(wizard) = &mut self.wizard_state {
            if let WizardStep::CreatingGitHubWorklogs {
                ref mut current_session_index,
                ..
            } = wizard.current_step
            {
                *current_session_index += 1;
            }
        }

        self.wizard_process_next_github_session();
    }

    pub(super) fn wizard_step_fill_gaps(&mut self) {
        logger::log("🔧 Step 5/7: Fill gaps with default task...".to_string());

        // Show issue selection popup
        let mut all_issues: Vec<_> = self.data.issues_by_key.values().cloned().collect();
        all_issues.sort_by(|a, b| a.key.cmp(&b.key));

        if let Some(wizard) = &self.wizard_state {
            self.gap_fill_state = Some(GapFillState {
                sprint_id: wizard.sprint_id,
                all_issues,
                selected_issue_index: 0,
                search_query: String::new(),
            });
        }
    }

    pub(super) fn wizard_step_review(&mut self) {
        logger::log("📋 Step 6/7: Reviewing created worklogs...".to_string());

        // Refresh data to get latest worklogs
        self.refresh_data();

        if let Some(wizard) = &mut self.wizard_state {
            // Get the sprint to filter worklogs
            let sprint = self
                .data
                .all_sprints
                .iter()
                .find(|s| s.id == wizard.sprint_id);

            if let Some(sprint) = sprint {
                if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                    // Count worklogs in "Created" state for this sprint
                    let worklogs_to_push: Vec<_> = self
                        .data
                        .all_worklogs
                        .iter()
                        .filter(|w| {
                            w.status == LocalWorklogState::Created
                                && w.started >= start
                                && w.started <= end
                        })
                        .collect();

                    let count = worklogs_to_push.len();
                    let total_hours: f64 = worklogs_to_push
                        .iter()
                        .map(|w| w.time_spent_seconds as f64 / 3600.0)
                        .sum();

                    wizard.summary.pushed_count = count;

                    logger::log(format!(
                        "📊 Found {} worklogs ({:.1}h) ready to push",
                        count, total_hours
                    ));

                    if count == 0 {
                        logger::log(
                            "ℹ️  No worklogs to push, skipping to completion...".to_string(),
                        );
                        wizard.completed_steps.insert(6);
                        wizard.completed_steps.insert(7);
                        wizard.current_step = WizardStep::Complete;
                    } else {
                        wizard.completed_steps.insert(6);
                        wizard.current_step = WizardStep::ReviewingWorklogs {
                            excluded_days: std::collections::HashSet::new(),
                        };
                    }
                }
            }
        }
    }

    pub(super) fn wizard_step_push(&mut self) {
        logger::log("🚀 Step 7/7: Pushing worklogs to Jira...".to_string());

        let (sprint_id, _pushed_count) = if let Some(wizard) = &mut self.wizard_state {
            wizard.current_step = WizardStep::Pushing;
            wizard.push_current = 0;
            wizard.push_total = wizard.summary.pushed_count;
            (wizard.sprint_id, wizard.summary.pushed_count)
        } else {
            return;
        };

        // Stage all Created worklogs for this sprint before pushing
        let sprint = self
            .data
            .all_sprints
            .iter()
            .find(|s| s.id == sprint_id)
            .cloned();

        if let Some(sprint) = sprint {
            if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                // Get all Created worklogs in sprint date range
                let worklogs_to_stage: Vec<_> = self
                    .data
                    .all_worklogs
                    .iter()
                    .filter(|w| {
                        w.status == LocalWorklogState::Created
                            && w.started >= start
                            && w.started <= end
                    })
                    .cloned()
                    .collect();

                // Stage them
                let staged_count = worklogs_to_stage.len();
                for mut worklog in worklogs_to_stage {
                    worklog.status = LocalWorklogState::Staged;
                    LocalWorklogService::save_local_worklog(worklog);
                }

                logger::log(format!("📦 Staged {} worklogs for push", staged_count));

                // DON'T call refresh_data() here - it's async and handle_push_worklogs
                // will read stale data. Instead, handle_push_worklogs queries the DB directly.

                // Trigger the push operation (reuse existing push functionality)
                self.handle_push_worklogs();

                // Refresh data AFTER push is started (async in background)
                self.refresh_data();
            } else {
                logger::log("⚠️  Sprint has no date range - cannot stage worklogs".to_string());
            }
        } else {
            logger::log("⚠️  Sprint not found - cannot stage worklogs".to_string());
        }
    }

    pub(super) fn wizard_push_complete(&mut self) {
        logger::log("✅ Complete! Push complete!".to_string());

        if let Some(wizard) = &mut self.wizard_state {
            wizard.completed_steps.insert(7);
            wizard.current_step = WizardStep::Complete;

            // Log success message with Chronie's wisdom
            if let Some(msg) = crate::tui::get_branding_text("wizard_complete") {
                logger::log(format!("✅ {}", msg));
            } else {
                logger::log("✅ Wizard complete!".to_string());
            }
        }
    }

    pub(super) fn wizard_rollback(&mut self) {
        if let Some(wizard) = &self.wizard_state {
            let log = &wizard.rollback_log;

            // Unlink meetings
            for meeting_id in &log.linked_meeting_ids {
                if let Some(mut meeting) = MeetingsService::get_meeting_by_id(meeting_id.clone()) {
                    meeting.jira_link = None;
                    MeetingsService::save(&meeting);
                    logger::log(format!("🔗 Unlinked meeting: {}", meeting_id));
                } else {
                    logger::log(format!("⚠️  Meeting {} not found", meeting_id));
                }
            }

            // Delete created worklogs
            for worklog_id in &log.created_worklog_ids {
                if let Some(worklog) = LocalWorklogService::get_worklog(worklog_id) {
                    LocalWorklogService::remove_local_worklog(&worklog);
                    logger::log(format!("🗑️  Deleted worklog: {}", worklog_id));
                } else {
                    logger::log(format!("⚠️  Worklog {} not found", worklog_id));
                }
            }

            let total_unlinked = log.linked_meeting_ids.len();
            let total_deleted = log.created_worklog_ids.len();
            logger::log(format!(
                "✅ Rollback complete: {} meetings unlinked, {} worklogs deleted",
                total_unlinked, total_deleted
            ));
        }
    }

    /// Update wizard animation frame
    pub(super) fn wizard_update_animation(&mut self) {
        if let Some(wizard) = &mut self.wizard_state {
            if !matches!(wizard.current_step, WizardStep::Complete) {
                wizard.spinner_frame = (wizard.spinner_frame + 1) % 10;
            }
        }
    }
}

// ============================================================================
// EventBus Integration - Wizard reacts to application events
// ============================================================================

/// Wizard subscriber that reacts to events during workflow
pub struct WizardEventHandler;

impl EventSubscriber for WizardEventHandler {
    fn on_event(&mut self, event: &AppEvent, tui: &mut Tui) {
        // Only process events if wizard is active
        let Some(wizard) = &tui.wizard_state else {
            return;
        };

        match (event, &wizard.current_step) {
            // Fetch completed during sync step - advance to auto-linking
            (AppEvent::FetchComplete(_), WizardStep::Syncing) => {
                tui.wizard_step_autolink();
            }

            // Push completed during push step - advance to complete
            (AppEvent::PushComplete { .. }, WizardStep::Pushing) => {
                tui.wizard_push_complete();
            }

            // Push progress during push step - update progress tracking
            (
                AppEvent::PushProgress {
                    current,
                    total,
                    message,
                },
                WizardStep::Pushing,
            ) => {
                if let Some(wizard) = &mut tui.wizard_state {
                    wizard.push_current = *current;
                    wizard.push_total = *total;
                    wizard.push_logs.push(message.clone());
                    // Keep only last 10 logs
                    if wizard.push_logs.len() > 10 {
                        wizard.push_logs.remove(0);
                    }
                }
            }

            _ => {} // Ignore other events
        }
    }
}
