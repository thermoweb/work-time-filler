use crate::tasks::google_tasks::GoogleEvent::{Absence, Meeting, Unknown};
use crate::tasks::Task;
use chrono::{DateTime, Utc};
use google_calendar3::api::Event;
use log::debug;
use std::collections::HashMap;
use std::error::Error;
use wtf_lib::models::data::{Absence as AbsenceEntity, Attendee, Meeting as MeetingEntity};
use wtf_lib::services::google_service::GoogleService;
use wtf_lib::services::meetings_service::{AbsenceService, MeetingsService};

pub struct FetchGoogleCalendarTask {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl FetchGoogleCalendarTask {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }
}

impl Task for FetchGoogleCalendarTask {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        debug!("fetch google calendar task");
        let hub = GoogleService::get_hub().await?;

        let results = hub
            .events()
            .list("primary")
            .time_min(self.start)
            .time_max(self.end)
            .single_events(true) // Expand recurring events to individual instances
            .max_results(1000)
            .doit()
            .await;

        match results {
            Ok((_, events)) => {
                if let Some(items) = events.items {
                    let count = items.len();
                    let mut meeting_count = 0;
                    let mut absence_count = 0;
                    let mut error_count = 0;
                    let mut fetched_meeting_ids = std::collections::HashSet::new();

                    for event in items {
                        match GoogleEvent::from_google(event.clone()) {
                            Absence(absence) => {
                                AbsenceService::save_absence(&absence);
                                absence_count += 1;
                            }
                            Meeting(meeting) => {
                                fetched_meeting_ids.insert(meeting.id.clone());
                                upsert_meetings(meeting);
                                meeting_count += 1;
                            }
                            Unknown => {
                                debug!("Unknown event: {:?}", event);
                                error_count += 1;
                            }
                        }
                    }

                    // Clean up meetings that no longer exist in Google Calendar
                    let db_meetings =
                        MeetingsService::get_meetings_between_dates(self.start, self.end);
                    debug!(
                        "Found {} meetings in database for date range {} to {}",
                        db_meetings.len(),
                        self.start.format("%Y-%m-%d"),
                        self.end.format("%Y-%m-%d")
                    );
                    debug!(
                        "Fetched {} meeting IDs from Google Calendar",
                        fetched_meeting_ids.len()
                    );

                    let mut removed_count = 0;
                    for db_meeting in db_meetings {
                        if !fetched_meeting_ids.contains(&db_meeting.id) {
                            debug!(
                                "Removing stale meeting: {} - {} (start: {})",
                                db_meeting.id,
                                db_meeting.title.as_deref().unwrap_or("Untitled"),
                                db_meeting.start.format("%Y-%m-%d %H:%M")
                            );
                            MeetingsService::delete_meeting(&db_meeting.id);
                            removed_count += 1;
                        } else {
                            debug!(
                                "Keeping meeting: {} - {}",
                                db_meeting.id,
                                db_meeting.title.as_deref().unwrap_or("Untitled")
                            );
                        }
                    }

                    debug!("{count} Google Calendar events fetched ({meeting_count} meetings, {absence_count} absences, {error_count} skipped, {removed_count} removed)");
                } else {
                    debug!("No upcoming events found.");
                }
            }
            Err(e) => {
                return Err(format!("Failed to retrieve Google Calendar events: {}", e).into());
            }
        }
        debug!("Google Calendar Task Finished.");
        Ok(())
    }
}

fn upsert_meetings(mut meeting: MeetingEntity) {
    match MeetingsService::get_meeting_by_id(meeting.id.to_string()) {
        Some(db_meeting) => {
            meeting.jira_link = db_meeting.jira_link;
        }
        None => {
            debug!("No meeting with id: {}", meeting.id);
        }
    }
    MeetingsService::save(&meeting);
}
pub enum GoogleEvent {
    Absence(AbsenceEntity),
    Meeting(MeetingEntity),
    Unknown,
}

impl GoogleEvent {
    pub fn from_google(event: Event) -> Self {
        let title = event
            .summary
            .clone()
            .unwrap_or_else(|| "No Title".to_string());

        if title.contains("Absence") {
            // Safely extract absence data
            return if let (Some(id), Some(start_dt), Some(end_dt)) = (
                event.id.clone(),
                event.start.as_ref().and_then(|s| s.date_time),
                event.end.as_ref().and_then(|s| s.date_time),
            ) {
                let absence = AbsenceEntity {
                    id,
                    start: start_dt,
                    end: end_dt,
                };
                Absence(absence)
            } else {
                debug!(
                    "Skipping Absence event '{}' - missing required fields (id, start, or end)",
                    title
                );
                Unknown
            };
        }

        // Try to parse as meeting
        if let Some(meeting) = from_google(event.clone()) {
            debug!("Meeting[{:?}]: {:?}", event.event_type, meeting);
            return Meeting(meeting);
        } else {
            debug!(
                "Event: {} [{:?}] | Start: {:?} End: {:?} | {:?}",
                title,
                event.event_type,
                event.start.and_then(|s| s.date_time),
                event.end.and_then(|s| s.date_time),
                event.description
            );
        }
        Unknown
    }
}

fn from_google(event: Event) -> Option<wtf_lib::models::data::Meeting> {
    if let (Some(start), Some(end)) = (
        event.start.as_ref().and_then(|s| s.date_time),
        event.end.as_ref().and_then(|s| s.date_time),
    ) {
        // Extract current user's response status
        let my_response_status = event.attendees.as_ref().and_then(|attendees| {
            attendees
                .iter()
                .find(|a| a.self_.unwrap_or(false))
                .and_then(|a| a.response_status.clone())
        });

        let attendees = event.attendees.map(|v| {
            v.iter()
                .map(|a| Attendee::from_google(a))
                .collect::<Vec<Attendee>>()
        });

        return Some(wtf_lib::models::data::Meeting {
            id: event.id.unwrap(),
            start,
            end,
            title: event.summary,
            description: event.description,
            attendees,
            jira_link: None,
            recurrence: event.recurrence,
            logs: HashMap::new(),
            my_response_status,
        });
    }
    None
}
