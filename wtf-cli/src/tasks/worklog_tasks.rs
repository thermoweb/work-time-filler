use crate::logger;
use crate::tasks::Task;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use log::{debug, error};
use rayon::prelude::*;
use std::error::Error;
use wtf_lib::models::data::{Meeting, Sprint};
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::LocalWorklogService;

pub struct MeetingWorklogTask {
    sprints: Vec<Sprint>,
}

impl MeetingWorklogTask {
    pub fn new(sprints: Vec<Sprint>) -> Self {
        Self { sprints }
    }

    fn log_sprint_meetings(&self, sprint: Sprint) {
        debug!("loging time for sprint {:?}", sprint);
        if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
            let meetings_to_log = Self::get_meeting_to_logs(start, end);
            meetings_to_log.par_iter().for_each(|(day, meeting)| {
                Self::log_meeting_for_day(day.clone(), meeting.clone());
            });
        }
    }

    fn get_meeting_to_logs(
        mut current: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<(DateTime<Utc>, Meeting)> {
        let mut meetings_to_log: Vec<(DateTime<Utc>, Meeting)> = Vec::new();
        while current <= end {
            debug!("day {}", current);
            if MeetingsService::is_absent(current.date_naive()) {
                debug!("user was absent that day !");
            } else {
                meetings_to_log.extend(
                    MeetingsService::get_meeting_by_date(current)
                        .into_iter()
                        .filter(|m| !Self::meeting_already_logged(m, current.date_naive()))
                        .map(|meeting| (current, meeting)),
                );
            }
            current += Duration::days(1);
        }
        meetings_to_log
    }

    fn meeting_already_logged(meeting: &Meeting, day: NaiveDate) -> bool {
        let found = LocalWorklogService::get_local_worklogs_on_day_for_meeting(&meeting.id, day);
        !found.is_empty()
    }

    fn log_meeting_for_day(current: DateTime<Utc>, meeting: Meeting) {
        let meeting_title = meeting.clone().title.unwrap_or("no title".to_string());
        debug!("meeting: {}", meeting_title);
        if let Some(jira_link) = meeting.clone().jira_link {
            let meeting_time_spent = (meeting.end - meeting.start).num_seconds();
            if let Some(start_date) = meeting.get_start_for_day(current.date_naive()) {
                let created_worklog = LocalWorklogService::create_new_local_worklogs(
                    start_date,
                    meeting_time_spent,
                    jira_link.as_str(),
                    Some(format!("{}", meeting_title).as_str()),
                    Some(meeting.id),
                );
                logger::log(format!(
                    "'{:.1}h worklog created for '{}' in issue '{}' -> {}",
                    meeting_time_spent as f64 / 3_600.0,
                    meeting_title,
                    jira_link,
                    created_worklog.id
                ));
            } else {
                debug!("{:?}", meeting);
                error!(
                    "Couldn't create worklog for '{}' at '{}'",
                    meeting_title, current
                );
            }
        } else {
            debug!("no jira link to log time for meeting {}", meeting_title);
        }
    }
}

impl Task for MeetingWorklogTask {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        for sprint in self.sprints.iter() {
            self.log_sprint_meetings(sprint.clone());
        }
        Ok(())
    }
}
