use crate::models::data::{Absence, Meeting, Sprint, SprintState};
use crate::services::jira_service::{JiraService, SprintService};
use crate::storage::database::{GenericDatabase, DATABASE};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use log::{error, warn};
use once_cell::sync::Lazy;
use std::sync::Arc;

static MEETINGS_DATABASE: Lazy<Arc<GenericDatabase<Meeting>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "meetings").expect("could not initialize meeting database"),
    )
});

pub struct MeetingsService;

impl MeetingsService {
    pub fn clear_all_meetings() {
        if let Err(e) = MEETINGS_DATABASE.clear() {
            error!("Failed to clear meetings database: {}", e);
        }
    }

    pub fn get_all_meetings() -> Vec<Meeting> {
        Self::get_meetings(true)
    }

    pub fn get_meetings(fetch_all: bool) -> Vec<Meeting> {
        let all_meetings = MEETINGS_DATABASE.get_all().unwrap_or_else(|e| {
            error!("Failed to retrieve meetings from database: {}", e);
            Vec::new()
        });

        if fetch_all {
            return all_meetings;
        }

        if let Some(sprint) = JiraService::get_followed_sprint()
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

    pub fn get_meetings_for_sprint_id(spring_id: &str) -> Vec<Meeting> {
        match SprintService::get_sprint(spring_id) {
            Ok(Some(sprint)) => Self::get_meetings_for_sprint(&sprint),
            _ => {
                eprintln!("Sprint '{}' not found!", spring_id);
                vec![]
            }
        }
    }

    pub fn get_meetings_for_sprint(sprint: &Sprint) -> Vec<Meeting> {
        if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
            let (start, end) = sprint_day_bounds(start, end);
            Self::get_meetings_between_dates(start, end)
        } else {
            vec![]
        }
    }

    pub fn get_meetings_between_dates(start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Meeting> {
        MEETINGS_DATABASE
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|meeting| meeting.is_between(start, end))
            .cloned()
            .collect::<Vec<Meeting>>()
    }

    pub fn get_meeting_by_date(fetch_date: DateTime<Utc>) -> Vec<Meeting> {
        MEETINGS_DATABASE
            .get_all()
            .unwrap_or_default()
            .iter()
            .filter(|m| m.is_on_day(fetch_date.date_naive()))
            .cloned()
            .collect::<Vec<Meeting>>()
    }

    pub fn get_absences() -> Vec<Absence> {
        ABSENCES_DATABASE.get_all().unwrap_or_else(|e| {
            error!("Failed to retrieve absences from database: {}", e);
            Vec::new()
        })
    }

    pub fn is_absent(day: NaiveDate) -> bool {
        ABSENCES_DATABASE
            .get_all()
            .unwrap_or_default()
            .iter()
            .any(|a| a.start.date_naive() <= day && day <= a.end.date_naive())
    }

    pub fn save(meeting: &Meeting) {
        if let Err(e) = MEETINGS_DATABASE.insert(meeting) {
            error!("Failed to save meeting '{}': {}", meeting.id, e);
        }
    }

    pub fn get_meeting_by_id(id: String) -> Option<Meeting> {
        match MEETINGS_DATABASE.get(id.as_str()) {
            Ok(meeting) => meeting,
            Err(e) => {
                error!("Failed to retrieve meeting '{}': {}", id, e);
                None
            }
        }
    }

    pub fn delete_meeting(id: &str) {
        if let Err(e) = MEETINGS_DATABASE.remove(id) {
            error!("Failed to delete meeting '{}': {}", id, e);
        }
    }
}

// --- Untracked meetings ---

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct UntrackedMeeting {
    meeting_id: String,
}

impl crate::storage::database::Identifiable for UntrackedMeeting {
    fn get_id(&self) -> String {
        self.meeting_id.clone()
    }
}

static UNTRACKED_MEETINGS_DATABASE: Lazy<Arc<GenericDatabase<UntrackedMeeting>>> =
    Lazy::new(|| {
        Arc::new(
            GenericDatabase::new(&DATABASE, "untracked_meetings")
                .expect("could not initialize untracked_meetings database"),
        )
    });

impl MeetingsService {
    pub fn get_all_untracked_ids() -> std::collections::HashSet<String> {
        UNTRACKED_MEETINGS_DATABASE
            .get_all()
            .unwrap_or_default()
            .into_iter()
            .map(|u| u.meeting_id)
            .collect()
    }

    /// Toggle a meeting's manually-untracked state.
    /// Returns `true` if it is now untracked, `false` if it was removed.
    pub fn toggle_untracked(meeting_id: &str) -> bool {
        let already = UNTRACKED_MEETINGS_DATABASE.get(meeting_id).ok().flatten().is_some();
        if already {
            let _ = UNTRACKED_MEETINGS_DATABASE.remove(meeting_id);
            false
        } else {
            let record = UntrackedMeeting { meeting_id: meeting_id.to_string() };
            let _ = UNTRACKED_MEETINGS_DATABASE.insert(&record);
            true
        }
    }
}

static ABSENCES_DATABASE: Lazy<Arc<GenericDatabase<Absence>>> = Lazy::new(|| {
    Arc::new(
        GenericDatabase::new(&DATABASE, "absences").expect("could not initialize absence database"),
    )
});

pub struct AbsenceService;

impl AbsenceService {
    pub fn save_absence(absence: &Absence) {
        if let Err(e) = ABSENCES_DATABASE.insert(absence) {
            error!("Failed to save absence '{}': {}", absence.id, e);
        }
    }
}

/// Expands sprint start to midnight UTC (start-of-day) and sprint end to 23:59:59 UTC (end-of-day)
/// so that meetings on the sprint start/end day are not missed due to exact sprint timestamps.
fn sprint_day_bounds(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_day = Utc
        .from_utc_datetime(&start.date_naive().and_hms_opt(0, 0, 0).unwrap());
    let end_day = Utc
        .from_utc_datetime(&end.date_naive().and_hms_opt(23, 59, 59).unwrap());
    (start_day, end_day)
}
