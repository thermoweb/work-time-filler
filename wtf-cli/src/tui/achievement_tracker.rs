use crate::logger;
use crate::tui::types::{AppEvent, EventSubscriber, Tui};
use wtf_lib::Achievement;

/// Achievement tracker that listens to app events and unlocks achievements
pub struct AchievementTracker;

impl AchievementTracker {
    /// Check which achievements should be unlocked based on an event
    /// Returns a list of achievements that meet their unlock conditions
    fn check_unlock_candidates(event: &AppEvent, _has_wizard: bool, tui: &Tui) -> Vec<Achievement> {
        let mut candidates = Vec::new();

        match event {
            AppEvent::PushComplete { history_id } => {
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

                // Forgot Friday: any worklog in this push is for a Friday pushed after that day
                if Self::has_forgotten_friday(history_id) {
                    candidates.push(Achievement::ForgotFriday);
                }

                // Perfect Sprint: every workday in a sprint is now covered
                if Self::completes_perfect_sprint(tui) {
                    candidates.push(Achievement::PerfectSprint);
                }

                // Overachiever: any worklog in this push is dated today (local time)
                if Self::has_today_worklog(history_id) {
                    candidates.push(Achievement::Overachiever);
                }

                // Wizard-only achievements
                if tui.wizard_state.is_some() {
                    let wizard = tui.wizard_state.as_ref().unwrap();

                    // Speed Runner: wizard start to push in under 3 minutes
                    if Self::is_speed_run(&wizard.started_at) {
                        candidates.push(Achievement::SpeedRunner);
                    }

                    // No Gaps: wizard produced worklogs but none from gap-fill
                    if wizard.summary.worklogs_from_gaps == 0
                        && (wizard.summary.worklogs_from_meetings
                            + wizard.summary.worklogs_from_github)
                            > 0
                    {
                        candidates.push(Achievement::NoGaps);
                    }

                    // GitHub Whisperer: at least one worklog came from a GitHub session
                    if wizard.summary.worklogs_from_github >= 1 {
                        candidates.push(Achievement::GitHubWhisperer);
                    }
                }

                // Rainbow Calendar: 3+ distinct color labels among linked meetings in this push
                if Self::has_rainbow_calendar(history_id, tui) {
                    candidates.push(Achievement::RainbowCalendar);
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
            AppEvent::AutoLinkComplete { linked_count } => {
                if *linked_count >= 10 {
                    candidates.push(Achievement::AutoLinkMaster);
                }
            }
            AppEvent::MeetingColorLinked => {
                candidates.push(Achievement::ColorCoder);
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
        if let Some(entry) = LocalWorklogService::production().get_history_by_id(history_id) {
            let sixty_days_ago = Utc::now() - chrono::Duration::days(60);

            // Check each worklog in this push, fetching them one by one
            // Early exit as soon as we find one >60 days old
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) =
                    LocalWorklogService::production().get_local_worklog_by_id(worklog_id)
                {
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
        let current_entry = match LocalWorklogService::production().get_history_by_id(history_id) {
            Some(e) => e,
            None => return false,
        };
        let current_dates: HashSet<chrono::NaiveDate> = current_entry
            .local_worklogs_id
            .iter()
            .filter_map(|wid| LocalWorklogService::production().get_local_worklog_by_id(wid))
            .map(|wl| wl.started.date_naive())
            .collect();

        if current_dates.is_empty() {
            return false;
        }

        // Count how many history entries (pushes) cover each date across all history
        let all_history = LocalWorklogService::production().get_history();
        let mut push_counts: HashMap<chrono::NaiveDate, usize> = HashMap::new();

        for entry in &all_history {
            let dates: HashSet<chrono::NaiveDate> = entry
                .local_worklogs_id
                .iter()
                .filter_map(|wid| LocalWorklogService::production().get_local_worklog_by_id(wid))
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
        let logged_dates: HashSet<NaiveDate> = LocalWorklogService::production()
            .get_all_local_worklogs()
            .into_iter()
            .filter(|w| w.status == wtf_lib::models::data::LocalWorklogState::Pushed)
            .map(|w| w.started.date_naive())
            .collect();

        if logged_dates.is_empty() {
            return false;
        }

        let today = chrono::Local::now().date_naive();

        // Check each of the four calendar quarters in years we have data for
        let years: HashSet<i32> = logged_dates.iter().map(|d| d.year()).collect();
        for year in years {
            for quarter in 1u32..=4 {
                let (q_start, q_end) = quarter_bounds(year, quarter);

                // Only check quarters that have fully ended
                if q_end >= today {
                    continue;
                }

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

    /// Check if the specified push contains a worklog linked to a declined meeting
    fn has_declined_meeting_worklog(history_id: &str, tui: &Tui) -> bool {
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        // Get the specific history entry directly by ID
        if let Some(entry) = LocalWorklogService::production().get_history_by_id(history_id) {
            // Check each worklog in this push
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) =
                    LocalWorklogService::production().get_local_worklog_by_id(worklog_id)
                {
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

    /// Check if the push contains a worklog for a Friday that was pushed after that Friday
    fn has_forgotten_friday(history_id: &str) -> bool {
        use chrono::{Datelike, Local, Weekday};
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        let today = Local::now().date_naive();
        let svc = LocalWorklogService::production();

        if let Some(entry) = svc.get_history_by_id(history_id) {
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) = svc.get_local_worklog_by_id(worklog_id) {
                    let worklog_date = worklog.started.date_naive();
                    if worklog_date.weekday() == Weekday::Fri && today > worklog_date {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if every workday in any followed sprint is covered by a pushed worklog
    fn completes_perfect_sprint(tui: &Tui) -> bool {
        use chrono::{Datelike, NaiveDate, Weekday};
        use std::collections::HashSet;
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        let pushed_dates: HashSet<NaiveDate> = LocalWorklogService::production()
            .get_all_local_worklogs()
            .into_iter()
            .filter(|w| w.status == wtf_lib::models::data::LocalWorklogState::Pushed)
            .map(|w| w.started.date_naive())
            .collect();

        if pushed_dates.is_empty() {
            return false;
        }

        for sprint in &tui.data.all_sprints {
            let (Some(start), Some(end)) = (sprint.start, sprint.end) else {
                continue;
            };
            let q_start = start.date_naive();
            let q_end = end.date_naive();

            let workdays: Vec<NaiveDate> = (0..)
                .map(|i| q_start + chrono::Duration::days(i))
                .take_while(|d| *d <= q_end)
                .filter(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
                .collect();

            if workdays.is_empty() {
                continue;
            }

            if workdays.iter().all(|d| pushed_dates.contains(d)) {
                return true;
            }
        }
        false
    }

    /// Check if any worklog in this push is dated today (local time) — "Overachiever"
    fn has_today_worklog(history_id: &str) -> bool {
        use chrono::Local;
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        let today = Local::now().date_naive();
        let svc = LocalWorklogService::production();
        if let Some(entry) = svc.get_history_by_id(history_id) {
            for worklog_id in &entry.local_worklogs_id {
                if let Some(worklog) = svc.get_local_worklog_by_id(worklog_id) {
                    if worklog.started.date_naive() == today {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if the wizard completed in under 3 minutes — "Speed Runner"
    fn is_speed_run(started_at: &chrono::DateTime<chrono::Utc>) -> bool {
        chrono::Utc::now() - *started_at < chrono::Duration::minutes(3)
    }

    /// Check if 3+ distinct calendar color labels are among meetings linked in this push
    fn has_rainbow_calendar(history_id: &str, tui: &Tui) -> bool {
        use std::collections::HashSet;
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        let svc = LocalWorklogService::production();
        let Some(entry) = svc.get_history_by_id(history_id) else {
            return false;
        };

        let color_ids: HashSet<String> = entry
            .local_worklogs_id
            .iter()
            .filter_map(|wid| svc.get_local_worklog_by_id(wid))
            .filter_map(|wl| wl.meeting_id)
            .filter_map(|mid| tui.data.all_meetings.iter().find(|m| m.id == mid).cloned())
            .filter_map(|m| m.color_id)
            .collect();

        color_ids.len() >= 3
    }

    fn handle_tiered(event: &AppEvent, has_wizard: bool, tui: &mut Tui) {
        let AppEvent::PushComplete { history_id } = event else {
            return;
        };

        if has_wizard {
            let (old, new) = tui.tiered_achievement_service.increment("wizard_runs", 1);
            Self::check_tier_crossings("wizard_runs", old, new, tui);
        }

        let hours = Self::hours_in_push(history_id);
        if hours > 0 {
            let (old, new) = tui
                .tiered_achievement_service
                .increment("hours_logged", hours);
            Self::check_tier_crossings("hours_logged", old, new, tui);
        }

        tui.data.tiered_progress = tui.tiered_achievement_service.get_all_progress();
    }

    fn hours_in_push(history_id: &str) -> u64 {
        use wtf_lib::services::worklogs_service::LocalWorklogService;
        let svc = LocalWorklogService::production();
        if let Some(entry) = svc.get_history_by_id(history_id) {
            let total_secs: i64 = entry
                .local_worklogs_id
                .iter()
                .filter_map(|wid| svc.get_local_worklog_by_id(wid))
                .map(|wl| wl.time_spent_seconds)
                .sum();
            return (total_secs.max(0) as u64) / 3600;
        }
        0
    }

    fn check_tier_crossings(track_id: &str, old: u64, new: u64, _tui: &mut Tui) {
        use wtf_lib::models::tiered_achievement::TieredAchievementDef;
        let defs = TieredAchievementDef::all();
        let Some(def) = defs.iter().find(|d| d.id == track_id) else {
            return;
        };
        for tier in &def.tiers {
            if old < tier.threshold && new >= tier.threshold {
                logger::log(format!(
                    "🏆 {} — {}: {}",
                    tier.icon,
                    def.unit.to_uppercase(),
                    tier.name
                ));
                logger::log(format!("   {}", tier.chronie_message));
            }
        }
    }

    /// Handle unlocking an achievement (log + publish event)
    fn handle_unlock(achievement: Achievement, tui: &mut Tui) {
        let meta = achievement.meta();
        logger::log(format!(
            "🏆 Achievement Unlocked: {} {}",
            meta.icon, meta.name
        ));
        logger::log(format!("   {}", meta.chronie_message));

        // Sync the TuiData snapshot so the achievements tab reflects immediately
        tui.data.unlocked_achievements = tui.achievement_service.get_all_unlocked();

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
            if tui.achievement_service.unlock(achievement) {
                Self::handle_unlock(achievement, tui);
            }
        }

        // Handle tiered achievement progression
        Self::handle_tiered(event, has_wizard, tui);
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
