use crate::models::data::{LocalWorklog, LocalWorklogHistory, LocalWorklogState, Worklog};
use crate::services::jira_service::IssueService;
use crate::storage::database::{GenericDatabase, DATABASE};
use chrono::{DateTime, NaiveDate, Utc};
use log::{debug, error};

pub struct LocalWorklogService {
    worklogs_db: GenericDatabase<LocalWorklog>,
    history_db: GenericDatabase<LocalWorklogHistory>,
}

impl LocalWorklogService {
    pub fn new(
        worklogs_db: GenericDatabase<LocalWorklog>,
        history_db: GenericDatabase<LocalWorklogHistory>,
    ) -> Self {
        Self {
            worklogs_db,
            history_db,
        }
    }

    /// Create a service backed by the production sled database.
    pub fn production() -> Self {
        let worklogs_db = GenericDatabase::new(&DATABASE, "local_worklogs")
            .expect("could not initialize local_worklogs database");
        let history_db = GenericDatabase::new(&DATABASE, "local_worklogs_history")
            .expect("could not initialize local_worklogs_history database");
        Self::new(worklogs_db, history_db)
    }

    pub fn get_worklog(&self, worklog_id: &String) -> Option<LocalWorklog> {
        self.worklogs_db.get(worklog_id).unwrap_or_else(|e| {
            error!("Failed to get worklog '{}': {}", worklog_id, e);
            None
        })
    }

    pub fn get_worklog_history(&self, worklog_history_id: &str) -> Option<LocalWorklogHistory> {
        self.history_db
            .get(worklog_history_id)
            .unwrap_or_else(|e| {
                error!(
                    "Failed to get worklog history '{}': {}",
                    worklog_history_id, e
                );
                None
            })
    }

    pub async fn revert_worklog_history(&self, worklog_history: &LocalWorklogHistory) {
        let worklogs_to_revert = worklog_history
            .local_worklogs_id
            .iter()
            .filter_map(|wid| self.get_worklog(wid))
            .collect::<Vec<_>>();
        for wl in worklogs_to_revert {
            if let Some(worklog_id) = &wl.worklog_id {
                debug!(
                    "removing worklog '{}' for issue '{}'",
                    worklog_id, wl.issue_id
                );
                IssueService::production().delete_worklog(&wl.issue_id, worklog_id).await;
                self.remove_local_worklog(&wl);
            } else {
                debug!("local worklog not associated with jira worklog...");
            }
        }
        self.history_db
            .remove(&worklog_history.id)
            .unwrap_or_else(|e| {
                error!(
                    "Failed to remove worklog history '{}': {}",
                    worklog_history.id, e
                );
            });
    }

    pub fn get_all_local_worklogs(&self) -> Vec<LocalWorklog> {
        self.worklogs_db.get_all().unwrap_or_default()
    }

    pub fn get_local_worklog_by_id(&self, id: &str) -> Option<LocalWorklog> {
        self.worklogs_db.get(id).ok().flatten()
    }

    pub fn get_local_worklogs_on_day_for_meeting(
        &self,
        meeting_id: &str,
        day: NaiveDate,
    ) -> Vec<LocalWorklog> {
        self.worklogs_db
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

    pub fn get_all_local_worklogs_by_status(&self, statuses: Vec<LocalWorklogState>) -> Vec<LocalWorklog> {
        self.worklogs_db
            .get_all()
            .unwrap_or_else(|_| Vec::new())
            .iter()
            .filter(|wl| statuses.contains(&wl.status))
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn save_local_worklog(&self, local_worklog: LocalWorklog) {
        if let Err(e) = self.worklogs_db.insert(&local_worklog) {
            error!("Failed to save local worklog '{}': {}", local_worklog.id, e);
        }
    }

    pub fn remove_local_worklog(&self, local_worklog: &LocalWorklog) {
        if let Err(e) = self.worklogs_db.remove(local_worklog.id.as_str()) {
            error!(
                "Failed to remove local worklog '{}': {}",
                local_worklog.id, e
            );
        }
    }

    pub fn create_history_for_pushed_worklogs(&self) {
        let pushed_worklogs =
            self.get_all_local_worklogs_by_status(vec![LocalWorklogState::Pushed]);

        if pushed_worklogs.is_empty() {
            debug!("No pushed worklogs found to historize");
            return;
        }

        let history = self.get_history();
        let mut historized_worklog_ids = std::collections::HashSet::new();
        for entry in history {
            for wid in &entry.local_worklogs_id {
                historized_worklog_ids.insert(wid.clone());
            }
        }

        let worklog_ids: Vec<String> = pushed_worklogs
            .iter()
            .filter(|w| !historized_worklog_ids.contains(&w.id))
            .map(|w| w.id.clone())
            .collect();

        if worklog_ids.is_empty() {
            debug!("All pushed worklogs are already historized");
            return;
        }

        self.historize(worklog_ids.clone());
        debug!(
            "Created recovery history for {} unhistorized pushed worklogs",
            worklog_ids.len()
        );
    }

    /// Import Jira worklogs that have no local counterpart, save them as Pushed LocalWorklogs,
    /// and group them into a single new history entry so they can be reverted later.
    /// Returns the number of imported worklogs.
    pub fn create_history_for_jira_only_worklogs(&self, jira_worklogs: &[Worklog]) -> usize {
        let local_worklogs = self.get_all_local_worklogs();
        let tracked_jira_ids: std::collections::HashSet<String> = local_worklogs
            .iter()
            .filter_map(|w| w.worklog_id.clone())
            .collect();

        let jira_only: Vec<&Worklog> = jira_worklogs
            .iter()
            .filter(|w| !tracked_jira_ids.contains(&w.id))
            .collect();

        if jira_only.is_empty() {
            debug!("No untracked Jira worklogs to import");
            return 0;
        }

        let mut new_ids = Vec::new();
        for jira_wl in &jira_only {
            let local_id = Self::generate_md5_id(&jira_wl.issue_id, jira_wl.started);
            let local_wl = LocalWorklog {
                id: local_id.clone(),
                comment: jira_wl
                    .comment
                    .clone()
                    .unwrap_or_else(|| "wtf[jira-import]".to_string()),
                time_spent_seconds: jira_wl.time_spent_seconds as i64,
                issue_id: jira_wl.issue_id.clone(),
                status: LocalWorklogState::Pushed,
                started: jira_wl.started,
                meeting_id: None,
                worklog_id: Some(jira_wl.id.clone()),
            };
            if let Err(e) = self.worklogs_db.insert(&local_wl) {
                error!(
                    "Failed to save imported Jira worklog '{}': {}",
                    local_wl.id, e
                );
            } else {
                new_ids.push(local_id);
            }
        }

        if new_ids.is_empty() {
            return 0;
        }
        let count = new_ids.len();
        self.historize(new_ids);
        debug!(
            "Imported {} Jira-only worklogs into a new history entry",
            count
        );
        count
    }

    pub fn create_new_local_worklogs(
        &self,
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
        if let Err(e) = self.worklogs_db.insert(&worklog) {
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

    pub fn historize(&self, local_worklogs_id: Vec<String>) -> String {
        let wl_history = LocalWorklogHistory::new(Utc::now(), local_worklogs_id);
        let history_id = wl_history.id.clone();
        if let Err(e) = self.history_db.insert(&wl_history) {
            error!(
                "Failed to create worklog history '{}': {}",
                wl_history.id, e
            );
        }
        history_id
    }

    pub fn get_history(&self) -> Vec<LocalWorklogHistory> {
        let mut history = self.history_db.get_all().unwrap_or_default();
        history.sort_by(|a, b| b.date.cmp(&a.date));
        history
    }

    pub fn get_history_by_id(&self, history_id: &str) -> Option<LocalWorklogHistory> {
        self.history_db.get(history_id).ok().flatten()
    }

    /// Delete a history entry from the database WITHOUT reverting in Jira
    pub fn delete_history_from_db(&self, history_id: &str) -> Result<(), String> {
        self.history_db
            .remove(history_id)
            .map_err(|e| format!("Failed to delete history: {}", e))?;
        debug!("Deleted history entry from DB: {}", history_id);
        Ok(())
    }

    /// Calculate total hours logged for a specific date
    pub fn calculate_daily_total(&self, date: NaiveDate) -> f64 {
        self.worklogs_db
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|wl| wl.started.date_naive() == date)
            .map(|wl| wl.time_spent_seconds as f64 / 3600.0)
            .sum()
    }

    /// Find days in a date range that have gaps (less than daily_limit hours logged)
    pub fn find_gap_days(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
        daily_limit: f64,
        min_threshold: f64,
    ) -> Vec<(NaiveDate, f64)> {
        use chrono::Datelike;

        let mut gaps = Vec::new();
        let mut current_date = start_date;

        while current_date <= end_date {
            let weekday = current_date.weekday().num_days_from_monday();
            if weekday >= 5 {
                current_date = current_date.succ_opt().unwrap_or(current_date);
                continue;
            }

            let existing_hours = self.calculate_daily_total(current_date);

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

pub struct WorklogsService {
    db: GenericDatabase<Worklog>,
}

impl WorklogsService {
    pub fn new(db: GenericDatabase<Worklog>) -> Self {
        Self { db }
    }

    /// Create a service backed by the production sled database.
    pub fn production() -> Self {
        let db = GenericDatabase::new(&DATABASE, "worklogs")
            .expect("could not initialize worklogs database");
        Self::new(db)
    }

    pub fn get_all_worklogs(&self) -> Vec<Worklog> {
        self.db.get_all().unwrap_or_default()
    }

    pub fn get_worklogs_by_date(&self, day: NaiveDate) -> Vec<Worklog> {
        self.db
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|w| w.started.date_naive() == day)
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn save_worklog(&self, worklog: Worklog) {
        self.db.insert(&worklog).unwrap();
    }

    pub fn remove_worklog(&self, worklog_id: &str) {
        self.db.remove(worklog_id).unwrap();
    }

    pub fn save_all_worklogs(&self, worklogs: Vec<Worklog>) {
        self.db.save_all(worklogs).unwrap();
    }

    pub fn replace_worklogs_for_date_range(
        &self,
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
        new_worklogs: Vec<Worklog>,
    ) {
        let all_worklogs = self.get_all_worklogs();

        let worklogs_to_keep: Vec<Worklog> = all_worklogs
            .into_iter()
            .filter(|w| {
                let date = w.started.date_naive();
                date < start_date || date > end_date
            })
            .collect();

        let mut all_combined = worklogs_to_keep;
        all_combined.extend(new_worklogs);

        self.db.clear().unwrap();
        self.db.save_all(all_combined).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::data::{LocalWorklog, LocalWorklogState, Worklog};
    use crate::storage::database::{Database, GenericDatabase};
    use chrono::{NaiveDate, TimeZone, Utc};

    fn make_local_service() -> LocalWorklogService {
        let db = Database::temporary();
        let worklogs_db = GenericDatabase::new(&db, "local_worklogs").unwrap();
        let history_db = GenericDatabase::new(&db, "local_worklogs_history").unwrap();
        LocalWorklogService::new(worklogs_db, history_db)
    }

    fn make_worklogs_service() -> WorklogsService {
        let db = Database::temporary();
        let worklogs_db = GenericDatabase::new(&db, "worklogs").unwrap();
        WorklogsService::new(worklogs_db)
    }

    fn local_worklog(id: &str, started: DateTime<Utc>, seconds: i64) -> LocalWorklog {
        LocalWorklog {
            id: id.to_string(),
            comment: "test".to_string(),
            time_spent_seconds: seconds,
            issue_id: "PROJ-1".to_string(),
            status: LocalWorklogState::Created,
            started,
            meeting_id: None,
            worklog_id: None,
        }
    }

    fn worklog(id: &str, started: DateTime<Utc>, seconds: u64) -> Worklog {
        Worklog {
            id: id.to_string(),
            author: "user".to_string(),
            created: started,
            time_spent: "1h".to_string(),
            time_spent_seconds: seconds,
            comment: None,
            issue_id: "PROJ-1".to_string(),
            started,
        }
    }

    #[test]
    fn test_save_and_get_local_worklog() {
        let svc = make_local_service();
        let t = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        svc.save_local_worklog(local_worklog("wl-1", t, 3600));

        let all = svc.get_all_local_worklogs();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "wl-1");
    }

    #[test]
    fn test_remove_local_worklog() {
        let svc = make_local_service();
        let t = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let wl = local_worklog("wl-del", t, 1800);
        svc.save_local_worklog(wl.clone());
        svc.remove_local_worklog(&wl);
        assert!(svc.get_all_local_worklogs().is_empty());
    }

    #[test]
    fn test_calculate_daily_total() {
        let svc = make_local_service();
        let jan10 = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let jan11 = Utc.with_ymd_and_hms(2024, 1, 11, 9, 0, 0).unwrap();
        svc.save_local_worklog(local_worklog("wl-a", jan10, 3600)); // 1h
        svc.save_local_worklog(local_worklog("wl-b", jan10, 1800)); // 0.5h
        svc.save_local_worklog(local_worklog("wl-c", jan11, 7200)); // 2h on different day

        let total = svc.calculate_daily_total(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap());
        assert!((total - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_find_gap_days_skips_weekends() {
        let svc = make_local_service();
        // 2024-01-13 is Saturday, 2024-01-14 is Sunday
        let gaps = svc.find_gap_days(
            NaiveDate::from_ymd_opt(2024, 1, 13).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 14).unwrap(),
            8.0,
            0.0,
        );
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_find_gap_days_weekday_with_gap() {
        let svc = make_local_service();
        // 2024-01-10 is Wednesday — no work logged, expect gap with min_threshold=0.5
        let gaps = svc.find_gap_days(
            NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 10).unwrap(),
            8.0,
            0.5,
        );
        assert_eq!(gaps.len(), 1);
        assert!((gaps[0].1 - 8.0).abs() < 0.001);
    }

    #[test]
    fn test_worklogs_service_save_and_get() {
        let svc = make_worklogs_service();
        let t = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        svc.save_worklog(worklog("jira-wl-1", t, 3600));

        let all = svc.get_all_worklogs();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "jira-wl-1");
    }

    #[test]
    fn test_worklogs_service_get_by_date() {
        let svc = make_worklogs_service();
        let jan10 = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let jan11 = Utc.with_ymd_and_hms(2024, 1, 11, 9, 0, 0).unwrap();
        svc.save_worklog(worklog("w1", jan10, 3600));
        svc.save_worklog(worklog("w2", jan11, 3600));

        let day = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let results = svc.get_worklogs_by_date(day);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "w1");
    }

    #[test]
    fn test_worklogs_service_remove() {
        let svc = make_worklogs_service();
        let t = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        svc.save_worklog(worklog("w-del", t, 3600));
        svc.remove_worklog("w-del");
        assert!(svc.get_all_worklogs().is_empty());
    }
}
