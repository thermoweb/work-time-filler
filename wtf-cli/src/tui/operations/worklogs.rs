// Worklog operations: create, stage, push, delete, reset

use wtf_lib::models::data::{LocalWorklogState, Meeting};
use wtf_lib::services::worklogs_service::LocalWorklogService;

use crate::logger;
use crate::tui::log_chronie_message;

use super::super::{
    types::{FetchStatus, WizardStep},
    Tui,
};

impl Tui {
    pub(in crate::tui) fn handle_push_worklogs(&mut self) {
        // Get all staged worklogs directly from DB (not from self.data which might be stale)
        let staged_worklogs = LocalWorklogService::get_all_local_worklogs()
            .into_iter()
            .filter(|w| w.status == LocalWorklogState::Staged)
            .collect::<Vec<_>>();

        if staged_worklogs.is_empty() {
            logger::log("No staged worklogs to push".to_string());

            // If wizard is in Pushing step with no worklogs, advance to Complete
            if let Some(wizard) = &self.wizard_state {
                if matches!(wizard.current_step, WizardStep::Pushing) {
                    self.wizard_push_complete();
                }
            }
            return;
        }

        let count = staged_worklogs.len();
        logger::log(format!("🚀 Starting push of {} worklogs...", count));
        self.fetch_status = FetchStatus::Fetching(format!("Pushing {} worklogs to Jira...", count));

        // Create worklog history BEFORE pushing to Jira for safety
        // This ensures we can revert even if the push crashes
        let worklog_ids: Vec<String> = staged_worklogs.iter().map(|w| w.id.clone()).collect();
        let history_id = LocalWorklogService::historize(worklog_ids);
        logger::log(format!("📝 Created history entry for {} worklogs", count));

        // Spawn background thread to push worklogs
        let (sender, receiver) = std::sync::mpsc::channel();
        let (progress_sender, progress_receiver) = std::sync::mpsc::channel();
        self.push_receiver = Some(receiver);
        self.push_progress_receiver = Some(progress_receiver);

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let mut success_count = 0;
                let mut error_count = 0;
                let total_count = staged_worklogs.len();

                for (idx, worklog) in staged_worklogs.iter().enumerate() {
                    let duration = chrono::Duration::seconds(worklog.time_spent_seconds);
                    let comment = if worklog.comment.is_empty() {
                        None
                    } else {
                        Some(worklog.comment.clone())
                    };

                    match wtf_lib::services::jira_service::IssueService::add_time(
                        &worklog.issue_id,
                        duration,
                        worklog.started,
                        comment,
                    )
                    .await
                    {
                        Ok(Some(jira_worklog)) => {
                            // Update local worklog status to Pushed and save Jira worklog ID
                            let mut updated_worklog = worklog.clone();
                            updated_worklog.status = LocalWorklogState::Pushed;
                            updated_worklog.worklog_id = Some(jira_worklog.id.clone());
                            LocalWorklogService::save_local_worklog(updated_worklog);
                            success_count += 1;
                            let _ = progress_sender.send(format!(
                                "✅ [{}/{}] Pushed {} ({:.1}h)",
                                idx + 1,
                                total_count,
                                worklog.issue_id,
                                worklog.time_spent_seconds as f64 / 3600.0
                            ));
                        }
                        Ok(None) => {
                            error_count += 1;
                            let _ = progress_sender.send(format!(
                                "❌ [{}/{}] Failed {}",
                                idx + 1,
                                total_count,
                                worklog.issue_id
                            ));
                        }
                        Err(e) => {
                            log::error!("Failed to push worklog for {}: {:?}", worklog.issue_id, e);
                            error_count += 1;
                            let _ = progress_sender.send(format!(
                                "❌ [{}/{}] Error {}: {}",
                                idx + 1,
                                total_count,
                                worklog.issue_id,
                                e
                            ));
                        }
                    }
                }

                let msg = format!(
                    "✅ Pushed {} worklogs ({} errors)",
                    success_count, error_count
                );
                let _ = sender.send((msg, history_id));
            });
        });
    }

    pub(in crate::tui) fn handle_reset_worklogs(&mut self) {
        // Delete all unpushed worklogs (Created and Staged, but not Pushed)
        let unpushed_worklogs: Vec<_> = self
            .data
            .all_worklogs
            .iter()
            .filter(|w| {
                w.status == LocalWorklogState::Staged || w.status == LocalWorklogState::Created
            })
            .collect();

        let count = unpushed_worklogs.len();
        for worklog in unpushed_worklogs {
            LocalWorklogService::remove_local_worklog(worklog);
        }

        logger::log(format!("Deleted {} unpushed worklogs", count));
        self.refresh_data();
        self.data.ui_state.selected_worklog_index = 0;
    }

    pub(in crate::tui) fn handle_delete_worklog(&mut self, worklog_id: String) {
        if let Some(worklog) = LocalWorklogService::get_worklog(&worklog_id) {
            LocalWorklogService::remove_local_worklog(&worklog);
            logger::log(format!("Deleted worklog {}", worklog_id));
            log_chronie_message("erasing_timeline", "🧙 Chronie:");
            self.refresh_data();
        }
    }

    pub(in crate::tui) fn handle_toggle_worklog_stage(&mut self, worklog_id: String) {
        if let Some(mut worklog) = LocalWorklogService::get_worklog(&worklog_id) {
            match worklog.status {
                LocalWorklogState::Created => {
                    worklog.status = LocalWorklogState::Staged;
                    LocalWorklogService::save_local_worklog(worklog.clone());
                    logger::log(format!("Staged worklog for {}", worklog.issue_id));
                    self.refresh_data();
                }
                LocalWorklogState::Staged => {
                    worklog.status = LocalWorklogState::Created;
                    LocalWorklogService::save_local_worklog(worklog.clone());
                    logger::log(format!("Unstaged worklog for {}", worklog.issue_id));
                    self.refresh_data();
                }
                LocalWorklogState::Pushed => {
                    // Cannot unstage pushed worklogs
                    logger::log("Cannot unstage pushed worklog".to_string());
                }
            }
        }
    }

    pub(in crate::tui) fn handle_stage_all_worklogs(&mut self) {
        let created_worklogs: Vec<_> = self
            .data
            .all_worklogs
            .iter()
            .filter(|w| w.status == LocalWorklogState::Created)
            .cloned()
            .collect();

        let count = created_worklogs.len();
        for mut worklog in created_worklogs {
            worklog.status = LocalWorklogState::Staged;
            LocalWorklogService::save_local_worklog(worklog);
        }

        logger::log(format!("Staged {} worklogs", count));
        self.refresh_data();
    }

    pub(in crate::tui) fn create_worklog_from_meeting(
        &mut self,
        meeting: &Meeting,
        issue_key: &str,
    ) {
        // Calculate duration from start and end
        let duration_seconds = (meeting.end - meeting.start).num_seconds();
        let duration_hours = duration_seconds as f64 / 3600.0;

        let comment = format!(
            "Meeting: {}",
            meeting.title.as_ref().unwrap_or(&"Untitled".to_string())
        );

        let worklog = LocalWorklogService::create_new_local_worklogs(
            meeting.start,
            duration_seconds,
            issue_key,
            Some(&comment),
            Some(meeting.id.clone()),
        );

        // Track for rollback
        if let Some(wizard) = &mut self.wizard_state {
            wizard.summary.worklogs_from_meetings += 1;
            wizard.summary.total_hours += duration_hours;
            wizard.rollback_log.created_worklog_ids.push(worklog.id);
        }
    }
}
