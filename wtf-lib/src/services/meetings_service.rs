use crate::models::data::{Absence, Meeting, Sprint, SprintState};
use crate::services::jira_service::{JiraService, SprintService};
use crate::storage::database::{GenericDatabase, DATABASE};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use log::{error, warn};

// --- UntrackedMeeting (private) ---

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct UntrackedMeeting {
    meeting_id: String,
}

impl crate::storage::database::Identifiable for UntrackedMeeting {
    fn get_id(&self) -> String {
        self.meeting_id.clone()
    }
}

// --- MeetingsService ---

pub struct MeetingsService {
    meetings_db: GenericDatabase<Meeting>,
    untracked_db: GenericDatabase<UntrackedMeeting>,
    absences_db: GenericDatabase<Absence>,
}

impl MeetingsService {
    pub(crate) fn new(
        meetings_db: GenericDatabase<Meeting>,
        untracked_db: GenericDatabase<UntrackedMeeting>,
        absences_db: GenericDatabase<Absence>,
    ) -> Self {
        Self {
            meetings_db,
            untracked_db,
            absences_db,
        }
    }

    /// Create a service backed by the production sled database.
    pub fn production() -> Self {
        let meetings_db = GenericDatabase::new(&DATABASE, "meetings")
            .expect("could not initialize meeting database");
        let untracked_db = GenericDatabase::new(&DATABASE, "untracked_meetings")
            .expect("could not initialize untracked_meetings database");
        let absences_db = GenericDatabase::new(&DATABASE, "absences")
            .expect("could not initialize absence database");
        Self::new(meetings_db, untracked_db, absences_db)
    }

    pub fn clear_all_meetings(&self) {
        if let Err(e) = self.meetings_db.clear() {
            error!("Failed to clear meetings database: {}", e);
        }
    }

    pub fn get_all_meetings(&self) -> Vec<Meeting> {
        self.get_meetings(true)
    }

    pub fn get_meetings(&self, fetch_all: bool) -> Vec<Meeting> {
        let all_meetings = self.meetings_db.get_all().unwrap_or_else(|e| {
            error!("Failed to retrieve meetings from database: {}", e);
            Vec::new()
        });

        if fetch_all {
            return all_meetings;
        }

        if let Some(sprint) = JiraService::production().get_followed_sprint()
            .iter()
            .find(|s| matches!(s.state, SprintState::Active))
        {
            if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                let (start, end) = sprint_day_bounds(start, end);
                return all_meetings
                    .iter()
                    .cloned()
                    .filter(|m| m.is_between(start, end))
                    .collect::<Vec<Meeting>>();
            } else {
                warn!("Active sprint '{}' missing start or end date", sprint.name);
            }
        }
        all_meetings
    }

    pub fn get_meetings_for_sprint_id(&self, spring_id: &str) -> Vec<Meeting> {
        match SprintService::production().get_sprint(spring_id) {
            Ok(Some(sprint)) => self.get_meetings_for_sprint(&sprint),
            _ => {
                eprintln!("Sprint '{}' not found!", spring_id);
                vec![]
            }
        }
    }

    pub fn get_meetings_for_sprint(&self, sprint: &Sprint) -> Vec<Meeting> {
        if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
            let (start, end) = sprint_day_bounds(start, end);
            self.get_meetings_between_dates(start, end)
        } else {
            vec![]
        }
    }

    pub fn get_meetings_between_dates(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Meeting> {
        self.meetings_db
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|meeting| meeting.is_between(start, end))
            .cloned()
            .collect::<Vec<Meeting>>()
    }

    pub fn get_meeting_by_date(&self, fetch_date: DateTime<Utc>) -> Vec<Meeting> {
        self.meetings_db
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|m| m.is_on_day(fetch_date.date_naive()))
            .cloned()
            .collect::<Vec<Meeting>>()
    }

    pub fn get_absences(&self) -> Vec<Absence> {
        self.absences_db.get_all().unwrap_or_else(|e| {
            error!("Failed to retrieve absences from database: {}", e);
            Vec::new()
        })
    }

    pub fn is_absent(&self, day: NaiveDate) -> bool {
        self.absences_db
            .get_all()
            .unwrap_or_default()
            .iter()
            .any(|a| a.start.date_naive() <= day && day <= a.end.date_naive())
    }

    pub fn save_absence(&self, absence: &Absence) {
        if let Err(e) = self.absences_db.insert(absence) {
            error!("Failed to save absence '{}': {}", absence.id, e);
        }
    }

    pub fn save(&self, meeting: &Meeting) {
        if let Err(e) = self.meetings_db.insert(meeting) {
            error!("Failed to save meeting '{}': {}", meeting.id, e);
        }
    }

    pub fn get_meeting_by_id(&self, id: String) -> Option<Meeting> {
        match self.meetings_db.get(id.as_str()) {
            Ok(meeting) => meeting,
            Err(e) => {
                error!("Failed to retrieve meeting '{}': {}", id, e);
                None
            }
        }
    }

    pub fn delete_meeting(&self, id: &str) {
        if let Err(e) = self.meetings_db.remove(id) {
            error!("Failed to delete meeting '{}': {}", id, e);
        }
    }

    pub fn get_all_untracked_ids(&self) -> std::collections::HashSet<String> {
        self.untracked_db
            .get_all()
            .unwrap_or_default()
            .into_iter()
            .map(|u| u.meeting_id)
            .collect()
    }

    /// Toggle a meeting's manually-untracked state.
    /// Returns `true` if it is now untracked, `false` if it was removed.
    pub fn toggle_untracked(&self, meeting_id: &str) -> bool {
        let already = self.untracked_db
            .get(meeting_id)
            .ok()
            .flatten()
            .is_some();
        if already {
            let _ = self.untracked_db.remove(meeting_id);
            false
        } else {
            let record = UntrackedMeeting {
                meeting_id: meeting_id.to_string(),
            };
            let _ = self.untracked_db.insert(&record);
            true
        }
    }
}

/// Kept for backward compatibility — delegates to `MeetingsService::production()`.
pub struct AbsenceService;

impl AbsenceService {
    pub fn save_absence(absence: &Absence) {
        MeetingsService::production().save_absence(absence);
    }
}

/// Expands sprint start to midnight UTC (start-of-day) and sprint end to 23:59:59 UTC (end-of-day)
/// so that meetings on the sprint start/end day are not missed due to exact sprint timestamps.
fn sprint_day_bounds(start: DateTime<Utc>, end: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_day = Utc.from_utc_datetime(&start.date_naive().and_hms_opt(0, 0, 0).unwrap());
    let end_day = Utc.from_utc_datetime(&end.date_naive().and_hms_opt(23, 59, 59).unwrap());
    (start_day, end_day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::{Database, GenericDatabase};
    use chrono::{Duration, TimeZone, Utc};
    use std::collections::HashMap;

    fn make_service() -> MeetingsService {
        let db = Database::temporary();
        let meetings_db = GenericDatabase::new(&db, "meetings").unwrap();
        let untracked_db = GenericDatabase::new(&db, "untracked_meetings").unwrap();
        let absences_db = GenericDatabase::new(&db, "absences").unwrap();
        MeetingsService::new(meetings_db, untracked_db, absences_db)
    }

    fn make_meeting(id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Meeting {
        Meeting {
            id: id.to_string(),
            title: Some(id.to_string()),
            description: None,
            start,
            end,
            attendees: None,
            jira_link: None,
            recurrence: None,
            logs: HashMap::new(),
            my_response_status: None,
            color_id: None,
        }
    }

    #[test]
    fn test_save_and_get_meeting() {
        let svc = make_service();
        let start = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 10, 10, 0, 0).unwrap();
        let meeting = make_meeting("meet-1", start, end);

        svc.save(&meeting);

        let all = svc.get_all_meetings();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "meet-1");
    }

    #[test]
    fn test_get_meeting_by_id() {
        let svc = make_service();
        let start = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 10, 10, 0, 0).unwrap();
        svc.save(&make_meeting("meet-2", start, end));

        assert!(svc.get_meeting_by_id("meet-2".to_string()).is_some());
        assert!(svc.get_meeting_by_id("unknown".to_string()).is_none());
    }

    #[test]
    fn test_delete_meeting() {
        let svc = make_service();
        let start = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 10, 10, 0, 0).unwrap();
        svc.save(&make_meeting("meet-3", start, end));
        svc.delete_meeting("meet-3");

        assert!(svc.get_meeting_by_id("meet-3".to_string()).is_none());
    }

    #[test]
    fn test_get_meetings_between_dates() {
        let svc = make_service();
        let jan10 = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let jan11 = Utc.with_ymd_and_hms(2024, 1, 11, 9, 0, 0).unwrap();
        svc.save(&make_meeting("m1", jan10, jan10 + Duration::hours(1)));
        svc.save(&make_meeting("m2", jan11, jan11 + Duration::hours(1)));

        let results = svc.get_meetings_between_dates(
            Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 10, 23, 59, 59).unwrap(),
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn test_toggle_untracked() {
        let svc = make_service();

        // first toggle → now untracked
        assert!(svc.toggle_untracked("meet-x"));
        let ids = svc.get_all_untracked_ids();
        assert!(ids.contains("meet-x"));

        // second toggle → removed
        assert!(!svc.toggle_untracked("meet-x"));
        assert!(svc.get_all_untracked_ids().is_empty());
    }

    #[test]
    fn test_save_and_is_absent() {
        let svc = make_service();
        let start = Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 1, 17, 0, 0, 0).unwrap();
        let absence = Absence { id: "abs-1".to_string(), start, end };
        svc.save_absence(&absence);

        use chrono::NaiveDate;
        assert!(svc.is_absent(NaiveDate::from_ymd_opt(2024, 1, 16).unwrap()));
        assert!(!svc.is_absent(NaiveDate::from_ymd_opt(2024, 1, 18).unwrap()));
    }

    #[test]
    fn test_clear_all_meetings() {
        let svc = make_service();
        let start = Utc.with_ymd_and_hms(2024, 1, 10, 9, 0, 0).unwrap();
        let end = start + Duration::hours(1);
        svc.save(&make_meeting("m-clear", start, end));
        assert_eq!(svc.get_all_meetings().len(), 1);

        svc.clear_all_meetings();
        assert!(svc.get_all_meetings().is_empty());
    }
}
