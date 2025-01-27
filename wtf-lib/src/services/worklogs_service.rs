use crate::models::data::{LocalWorklog, LocalWorklogHistory, LocalWorklogState, Worklog};
use crate::services::jira_service::IssueService;
use crate::storage::database::{GenericDatabase, DATABASE};
use chrono::{DateTime, NaiveDate, Utc};
use log::{debug, error};
use once_cell::sync::Lazy;
use std::sync::Arc;

static LOCAL_WORKLOGS_DB: Lazy<Arc<GenericDatabase<LocalWorklog>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "local_worklogs")
            .expect("could not initialize local_worklogs database"),
    )
});

static LOCAL_WORKLOGS_HISTORY_DB: Lazy<Arc<GenericDatabase<LocalWorklogHistory>>> =
    Lazy::new(|| {
        Arc::new(
            GenericDatabase::new(&DATABASE, "local_worklogs_history")
                .expect("could not initialize local_worklogs_history database"),
        )
    });

static WORKLOGS_DATABASE: Lazy<Arc<GenericDatabase<Worklog>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "worklogs")
            .expect("could not initialize worklogs database"),
    )
});

pub struct LocalWorklogService;

impl LocalWorklogService {
    pub fn get_worklog(worklog_id: &String) -> Option<LocalWorklog> {
        LOCAL_WORKLOGS_DB.get(worklog_id).unwrap_or_else(|e| {
            error!("Failed to get worklog '{}': {}", worklog_id, e);
            None
        })
    }

    pub fn get_worklog_history(worklog_history_id: &str) -> Option<LocalWorklogHistory> {
        LOCAL_WORKLOGS_HISTORY_DB
            .get(worklog_history_id)
            .unwrap_or_else(|e| {
                error!(
                    "Failed to get worklog history '{}': {}",
                    worklog_history_id, e
                );
                None
            })
    }

    pub async fn revert_worklog_history(worklog_history: &LocalWorklogHistory) {
        let worklogs_to_revert = worklog_history
            .local_worklogs_id
            .iter()
            .filter_map(|wid| Self::get_worklog(wid))
            .collect::<Vec<_>>();
        for wl in worklogs_to_revert {
            if let Some(worklog_id) = &wl.worklog_id {
                debug!(
                    "removing worklog '{}' for issue '{}'",
                    worklog_id, wl.issue_id
                );
                IssueService::delete_worklog(&wl.issue_id, worklog_id).await;
                LocalWorklogService::remove_local_worklog(&wl);
            } else {
                debug!("local worklog not associated with jira worklog...");
            }
        }
        LOCAL_WORKLOGS_HISTORY_DB
            .remove(&worklog_history.id)
            .unwrap_or_else(|e| {
                error!(
                    "Failed to remove worklog history '{}': {}",
                    worklog_history.id, e
                );
            });
    }
    pub fn get_all_local_worklogs() -> Vec<LocalWorklog> {
        LOCAL_WORKLOGS_DB.get_all().unwrap_or_default()
    }

    pub fn get_local_worklog_by_id(id: &str) -> Option<LocalWorklog> {
        LOCAL_WORKLOGS_DB.get(id).ok().flatten()
    }

    pub fn get_local_worklogs_on_day_for_meeting(
        meeting_id: &str,
        day: NaiveDate,
    ) -> Vec<LocalWorklog> {
        LOCAL_WORKLOGS_DB
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|wl| {
                wl.meeting_id
                    .clone()
                    .filter(|id| id == meeting_id)
                    .is_some()
                    && wl.started.date_naive() == day
            })
            .cloned()
            .collect::<Vec<LocalWorklog>>()
    }

    pub fn get_all_local_worklogs_by_status(statuses: Vec<LocalWorklogState>) -> Vec<LocalWorklog> {
        LOCAL_WORKLOGS_DB
            .get_all()
            .unwrap_or_else(|_| Vec::new())
            .iter()
            .filter(|wl| statuses.contains(&wl.status))
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn save_local_worklog(local_worklog: LocalWorklog) {
        if let Err(e) = LOCAL_WORKLOGS_DB.insert(&local_worklog) {
            error!("Failed to save local worklog '{}': {}", local_worklog.id, e);
        }
    }

    pub fn remove_local_worklog(local_worklog: &LocalWorklog) {
        if let Err(e) = LOCAL_WORKLOGS_DB.remove(local_worklog.id.as_str()) {
            error!(
                "Failed to remove local worklog '{}': {}",
                local_worklog.id, e
            );
        }
    }

    /// Create history entry for already-pushed worklogs (recovery function)
    /// Use this to create missing history for worklogs that were pushed before history was saved
    pub fn create_history_for_pushed_worklogs() {
        let pushed_worklogs =
            Self::get_all_local_worklogs_by_status(vec![LocalWorklogState::Pushed]);

        if pushed_worklogs.is_empty() {
            debug!("No pushed worklogs found to historize");
            return;
        }

        // Get all existing history entries to check which worklogs are already historized
        let history = Self::get_history();
        let mut historized_worklog_ids = std::collections::HashSet::new();
        for entry in history {
            for wid in &entry.local_worklogs_id {
                historized_worklog_ids.insert(wid.clone());
            }
        }

        // Only historize worklogs that aren't already in a history entry
        let worklog_ids: Vec<String> = pushed_worklogs
            .iter()
            .filter(|w| !historized_worklog_ids.contains(&w.id))
            .map(|w| w.id.clone())
            .collect();

        if worklog_ids.is_empty() {
            debug!("All pushed worklogs are already historized");
            return;
        }

        Self::historize(worklog_ids.clone());
        debug!(
            "Created recovery history for {} unhistorized pushed worklogs",
            worklog_ids.len()
        );
    }

    pub fn create_new_local_worklogs(
        started: DateTime<Utc>,
        time_spent_seconds: i64,
        issue_id: &str,
        message: Option<&str>,
        meeting_id: Option<String>,
    ) -> LocalWorklog {
        let id = Self::generate_md5_id(issue_id, started);
        let comment = format!("wtf[{}]-{}", id, message.unwrap_or("no_msg"));
        let worklog = LocalWorklog {
            id,
            comment,
            time_spent_seconds,
            status: LocalWorklogState::Created,
            issue_id: issue_id.to_string(),
            started,
            meeting_id,
            worklog_id: None,
        };
        if let Err(e) = LOCAL_WORKLOGS_DB.insert(&worklog) {
            error!("Failed to create worklog '{}': {}", worklog.id, e);
        }
        debug!("new worklog created: '{}'", worklog.id);
        worklog
    }

    fn generate_md5_id(issue_id: &str, started: DateTime<Utc>) -> String {
        let input = format!("{}-{}", issue_id, started);
        let digest = md5::compute(input);
        format!("{:x}", digest)[..8].to_string()
    }

    pub fn historize(local_worklogs_id: Vec<String>) -> String {
        let wl_history = LocalWorklogHistory::new(Utc::now(), local_worklogs_id);
        let history_id = wl_history.id.clone();
        if let Err(e) = LOCAL_WORKLOGS_HISTORY_DB.insert(&wl_history) {
            error!(
                "Failed to create worklog history '{}': {}",
                wl_history.id, e
            );
        }
        history_id
    }

    pub fn get_history() -> Vec<LocalWorklogHistory> {
        let mut history = LOCAL_WORKLOGS_HISTORY_DB.get_all().unwrap_or_default();
        history.sort_by(|a, b| b.date.cmp(&a.date));
        history
    }

    pub fn get_history_by_id(history_id: &str) -> Option<LocalWorklogHistory> {
        LOCAL_WORKLOGS_HISTORY_DB.get(history_id).ok().flatten()
    }

    /// Delete a history entry from the database WITHOUT reverting in Jira
    /// This just removes the history record, the worklogs remain in Pushed state
    pub fn delete_history_from_db(history_id: &str) -> Result<(), String> {
        LOCAL_WORKLOGS_HISTORY_DB
            .remove(history_id)
            .map_err(|e| format!("Failed to delete history: {}", e))?;
        debug!("Deleted history entry from DB: {}", history_id);
        Ok(())
    }

    /// Calculate total hours logged for a specific date
    /// Includes all worklogs (Created, Staged, Pushed) for that day
    pub fn calculate_daily_total(date: NaiveDate) -> f64 {
        LOCAL_WORKLOGS_DB
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|wl| wl.started.date_naive() == date)
            .map(|wl| wl.time_spent_seconds as f64 / 3600.0)
            .sum()
    }

    /// Find days in a date range that have gaps (less than daily_limit hours logged)
    /// Returns Vec<(date, hours_to_add)> for days that need filling
    /// Skips weekends and days already over min_threshold
    pub fn find_gap_days(
        start_date: NaiveDate,
        end_date: NaiveDate,
        daily_limit: f64,
        min_threshold: f64,
    ) -> Vec<(NaiveDate, f64)> {
        use chrono::Datelike;

        let mut gaps = Vec::new();
        let mut current_date = start_date;

        while current_date <= end_date {
            // Skip weekends (Saturday=6, Sunday=7 in num_days_from_monday)
            let weekday = current_date.weekday().num_days_from_monday();
            if weekday >= 5 {
                current_date = current_date.succ_opt().unwrap_or(current_date);
                continue;
            }

            let existing_hours = Self::calculate_daily_total(current_date);

            // Skip days already substantially logged (over threshold)
            if existing_hours >= min_threshold {
                current_date = current_date.succ_opt().unwrap_or(current_date);
                continue;
            }

            let hours_to_add = daily_limit - existing_hours;
            if hours_to_add > 0.0 {
                gaps.push((current_date, hours_to_add));
            }

            current_date = current_date.succ_opt().unwrap_or(current_date);
        }

        gaps
    }
}

pub struct WorklogsService;

impl WorklogsService {
    pub fn get_all_worklogs() -> Vec<Worklog> {
        WORKLOGS_DATABASE.get_all().unwrap_or_default()
    }

    pub fn get_worklogs_by_date(day: NaiveDate) -> Vec<Worklog> {
        WORKLOGS_DATABASE
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|w| w.started.date_naive() == day)
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn save_worklog(worklog: Worklog) {
        WORKLOGS_DATABASE.insert(&worklog).unwrap();
    }

    pub fn remove_worklog(worklog_id: &str) {
        WORKLOGS_DATABASE.remove(worklog_id).unwrap();
    }

    pub fn save_all_worklogs(worklogs: Vec<Worklog>) {
        WORKLOGS_DATABASE.save_all(worklogs).unwrap();
    }

    pub fn replace_worklogs_for_date_range(
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
        new_worklogs: Vec<Worklog>,
    ) {
        // Get all existing worklogs
        let all_worklogs = Self::get_all_worklogs();

        // Keep only worklogs OUTSIDE the date range we're updating
        let worklogs_to_keep: Vec<Worklog> = all_worklogs
            .into_iter()
            .filter(|w| {
                let date = w.started.date_naive();
                date < start_date || date > end_date
            })
            .collect();

        // Combine kept worklogs with new ones
        let mut all_combined = worklogs_to_keep;
        all_combined.extend(new_worklogs);

        // Clear and save all
        WORKLOGS_DATABASE.clear().unwrap();
        WORKLOGS_DATABASE.save_all(all_combined).unwrap();
    }
}
