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
