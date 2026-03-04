use crate::logger;
use crate::tui::types::{AppEvent, EventSubscriber, Tui};
use wtf_lib::services::AchievementService;
use wtf_lib::Achievement;

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

                // Night Owl: Check if the push is happening after 10pm or before 6am
                if Self::has_night_worklog() {
                    candidates.push(Achievement::NightOwl);
                }

                // Quarter Crunch: Check if any full calendar quarter now has ≥90% working days covered
                if Self::completes_quarter_crunch() {
                    candidates.push(Achievement::QuarterCrunch);
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
        use chrono::Utc;
        use wtf_lib::services::worklogs_service::LocalWorklogService;

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
        use std::collections::{HashMap, HashSet};
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        // Get the dates covered by THIS push
        let current_entry = match LocalWorklogService::get_history_by_id(history_id) {
            Some(e) => e,
            None => return false,
        };
        let current_dates: HashSet<chrono::NaiveDate> = current_entry
            .local_worklogs_id
            .iter()
            .filter_map(|wid| LocalWorklogService::get_local_worklog_by_id(wid))
            .map(|wl| wl.started.date_naive())
            .collect();

        if current_dates.is_empty() {
            return false;
        }

        // Count how many history entries (pushes) cover each date across all history
        let all_history = LocalWorklogService::get_history();
        let mut push_counts: HashMap<chrono::NaiveDate, usize> = HashMap::new();

        for entry in &all_history {
            let dates: HashSet<chrono::NaiveDate> = entry
                .local_worklogs_id
                .iter()
                .filter_map(|wid| LocalWorklogService::get_local_worklog_by_id(wid))
                .map(|wl| wl.started.date_naive())
                .collect();
            for date in dates {
                *push_counts.entry(date).or_insert(0) += 1;
            }
        }

        // Achievement triggers if any date covered by the current push has been pushed 3+ times
        current_dates
            .iter()
            .any(|d| push_counts.get(d).copied().unwrap_or(0) >= 3)
    }

    /// Check if the push is happening after 10pm or before 6am (local time)
    fn has_night_worklog() -> bool {
        use chrono::Timelike;
        let hour = chrono::Local::now().hour();
        hour >= 22 || hour < 6
    }

    /// Check if any full calendar quarter has ≥90% of its Mon–Fri days covered by pushed worklogs
    fn completes_quarter_crunch() -> bool {
        use chrono::{Datelike, NaiveDate, Weekday};
        use std::collections::HashSet;
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        // Collect all dates that have at least one pushed worklog
        let logged_dates: HashSet<NaiveDate> = LocalWorklogService::get_all_local_worklogs()
            .into_iter()
            .filter(|w| w.status == wtf_lib::models::data::LocalWorklogState::Pushed)
            .map(|w| w.started.date_naive())
            .collect();

        if logged_dates.is_empty() {
            return false;
        }

        // Check each of the four calendar quarters in years we have data for
        let years: HashSet<i32> = logged_dates.iter().map(|d| d.year()).collect();
        for year in years {
            for quarter in 1u32..=4 {
                let (q_start, q_end) = quarter_bounds(year, quarter);
                let workdays: Vec<NaiveDate> = (0..)
                    .map(|i| q_start + chrono::Duration::days(i))
                    .take_while(|d| *d <= q_end)
                    .filter(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
                    .collect();
                if workdays.is_empty() {
                    continue;
                }
                let covered = workdays.iter().filter(|d| logged_dates.contains(d)).count();
                if covered * 100 / workdays.len() >= 90 {
                    return true;
                }
            }
        }
        false
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
                        if let Some(meeting) =
                            tui.data.all_meetings.iter().find(|m| &m.id == meeting_id)
                        {
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
        logger::log(format!(
            "🏆 Achievement Unlocked: {} {}",
            meta.icon, meta.name
        ));
        logger::log(format!("   {}", meta.chronie_message));

        // Publish event for potential UI notification
        tui.event_bus
            .publish(AppEvent::AchievementUnlocked { achievement });
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

fn quarter_bounds(year: i32, quarter: u32) -> (chrono::NaiveDate, chrono::NaiveDate) {
    use chrono::{Datelike, NaiveDate};
    let start_month = (quarter - 1) * 3 + 1;
    let start = NaiveDate::from_ymd_opt(year, start_month, 1).unwrap();
    let end = if quarter == 4 {
        NaiveDate::from_ymd_opt(year, 12, 31).unwrap()
    } else {
        let next_q_start = NaiveDate::from_ymd_opt(year, start_month + 3, 1).unwrap();
        next_q_start.pred_opt().unwrap()
    };
    let _ = end.year(); // use Datelike to suppress unused import warning
    (start, end)
}
