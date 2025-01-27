use crate::tui::types::{AppEvent, EventSubscriber, Tui};
use wtf_lib::Achievement;
use wtf_lib::services::AchievementService;
use crate::logger;

/// Achievement tracker that listens to app events and unlocks achievements
pub struct AchievementTracker;

impl AchievementTracker {
    /// Check which achievements should be unlocked based on an event
    /// Returns a list of achievements that meet their unlock conditions
    fn check_unlock_candidates(event: &AppEvent, has_wizard: bool, tui: &Tui) -> Vec<Achievement> {
        let mut candidates = Vec::new();
        
        match event {
            AppEvent::PushComplete { history_id } => {
                // Wizard completion achievement
                if has_wizard {
                    candidates.push(Achievement::ChroniesApprentice);
                }
                
                // Timeline Fixer: Check if any worklog in THIS push is >60 days old
                if Self::has_old_worklog_in_push(history_id) {
                    candidates.push(Achievement::TimelineFixer);
                }
                
                // Git Squash Master: Check if this push results in 3+ times for same day
                if Self::has_multiple_pushes_same_day(history_id) {
                    candidates.push(Achievement::GitSquashMaster);
                }
                
                // Declined But Logged: Check if any worklog is linked to a declined meeting
                if Self::has_declined_meeting_worklog(history_id, tui) {
                    candidates.push(Achievement::DeclinedButLogged);
                }
            }
            AppEvent::RevertComplete => {
                // The Undoer: First time reverting
                candidates.push(Achievement::TheUndoer);
            }
            AppEvent::AboutPopupOpened => {
                // About popup achievement
                candidates.push(Achievement::AboutClicker);
            }
            AppEvent::SecretSequenceTriggered { sequence_name } => {
                // Map sequence name to achievement
                match sequence_name.as_str() {
                    "chronie" => {
                        candidates.push(Achievement::ChroniesFriend);
                    }
                    _ => {}
                }
            }
            AppEvent::FetchComplete(_) | AppEvent::DataRefreshed(_) => {
                // Auto-Link Master: Check if all meetings are linked
                if Self::has_perfect_auto_linking(tui) {
                    candidates.push(Achievement::AutoLinkMaster);
                }
            }
            _ => {}
        }
        
        candidates
    }
    
    /// Check if the specified push contains worklogs more than 60 days old
    fn has_old_worklog_in_push(history_id: &str) -> bool {
        use wtf_lib::services::worklogs_service::LocalWorklogService;
        use chrono::Utc;
        
        // Get the specific history entry directly by ID
        if let Some(entry) = LocalWorklogService::get_history_by_id(history_id) {
            let sixty_days_ago = Utc::now() - chrono::Duration::days(60);
            
            // Check each worklog in this push, fetching them one by one
            // Early exit as soon as we find one >60 days old
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) = LocalWorklogService::get_local_worklog_by_id(worklog_id) {
                    if worklog.started < sixty_days_ago {
                        return true; // Found one! No need to check more
                    }
                }
            }
        }
        
        false
    }
    
    /// Check if the specified push contains 3+ worklogs for the same day
    fn has_multiple_pushes_same_day(history_id: &str) -> bool {
        use wtf_lib::services::worklogs_service::LocalWorklogService;
        use std::collections::HashMap;
        
        // Get the specific history entry directly by ID
        if let Some(entry) = LocalWorklogService::get_history_by_id(history_id) {
            // Count how many worklogs per date in THIS push
            let mut date_counts: HashMap<chrono::NaiveDate, usize> = HashMap::new();
            
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) = LocalWorklogService::get_local_worklog_by_id(worklog_id) {
                    *date_counts.entry(worklog.started.date_naive()).or_insert(0) += 1;
                }
            }
            
            // Check if any date has 3+ worklogs in this push
            date_counts.values().any(|&count| count >= 3)
        } else {
            false
        }
    }
    
    /// Check if user has 10+ meetings and all are auto-linked (no unlinked meetings)
    fn has_perfect_auto_linking(tui: &Tui) -> bool {
        let all_meetings = &tui.data.all_meetings;
        
        // Need at least 10 meetings
        if all_meetings.len() < 10 {
            return false;
        }
        
        // Check if ALL meetings are linked (none have jira_link = None)
        let all_linked = all_meetings.iter().all(|m| m.jira_link.is_some());
        
        all_linked
    }
    
    /// Check if the specified push contains a worklog linked to a declined meeting
    fn has_declined_meeting_worklog(history_id: &str, tui: &Tui) -> bool {
        use wtf_lib::services::worklogs_service::LocalWorklogService;
        
        // Get the specific history entry directly by ID
        if let Some(entry) = LocalWorklogService::get_history_by_id(history_id) {
            // Check each worklog in this push
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) = LocalWorklogService::get_local_worklog_by_id(worklog_id) {
                    // Check if worklog has a meeting link
                    if let Some(meeting_id) = &worklog.meeting_id {
                        // Find the meeting in tui data
                        if let Some(meeting) = tui.data.all_meetings.iter().find(|m| &m.id == meeting_id) {
                            // Check if the meeting was declined
                            let is_declined = meeting
                                .my_response_status
                                .as_ref()
                                .map(|s| s == "declined")
                                .unwrap_or(false);
                            
                            if is_declined {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        
        false
    }
    
    /// Handle unlocking an achievement (log + publish event)
    fn handle_unlock(achievement: Achievement, tui: &mut Tui) {
        let meta = achievement.meta();
        logger::log(format!("🏆 Achievement Unlocked: {} {}", meta.icon, meta.name));
        logger::log(format!("   {}", meta.chronie_message));
        
        // Publish event for potential UI notification
        tui.event_bus.publish(AppEvent::AchievementUnlocked { achievement });
    }
}

impl EventSubscriber for AchievementTracker {
    fn on_event(&mut self, event: &AppEvent, tui: &mut Tui) {
        // Check all achievements to see if any should be unlocked
        let has_wizard = tui.wizard_state.is_some();
        let candidates = Self::check_unlock_candidates(event, has_wizard, tui);
        
        for achievement in candidates {
            // Try to unlock - returns true only if newly unlocked
            if AchievementService::unlock(achievement) {
                Self::handle_unlock(achievement, tui);
            }
        }
    }
}
