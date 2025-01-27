use crate::commands::Command;
use crate::tasks::google_tasks::FetchGoogleCalendarTask;
use crate::tasks::jira_tasks::{
    FetchJiraBoard, FetchJiraIssues, FetchJiraSprint, FetchJiraWorklogs,
};
use crate::tasks::Task;
use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command as ClapCommand};
use indicatif::MultiProgress;
use log::{debug, info};
use wtf_lib::services::jira_service::JiraService;

pub struct FetchCommand;

enum FetchType {
    All,
    Board,
    Issue,
    Sprint,
    Worklog,
    GoogleMeetings,
}

impl std::str::FromStr for FetchType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(FetchType::All),
            "board" => Ok(FetchType::Board),
            "issue" => Ok(FetchType::Issue),
            "sprint" => Ok(FetchType::Sprint),
            "worklog" => Ok(FetchType::Worklog),
            "google" => Ok(FetchType::GoogleMeetings),
            _ => Err(format!("Unknown fetch type: {}", s)),
        }
    }
}

#[async_trait]
impl Command for FetchCommand {
    fn name(&self) -> &'static str {
        "fetch"
    }

    async fn execute(&self, matches: &ArgMatches) {
        if let Some(fetch_type) = matches.get_one::<String>("type") {
            match fetch_type.parse() {
                Ok(FetchType::All) => fetch_all().await,
                Ok(FetchType::Board) => fetch_boards(None).await,
                Ok(FetchType::Sprint) => fetch_sprints(None).await,
                Ok(FetchType::Issue) => fetch_issues(None).await,
                Ok(FetchType::Worklog) => fetch_worklogs(None).await,
                Ok(FetchType::GoogleMeetings) => {
                    if let Err(e) = fetch_google_meetings(None).await {
                        eprintln!("Error: {}", e);
                    }
                }
                Err(err) => eprintln!("Error: {}", err),
            }
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Fetches and updates the wtf database")
            .arg(
                Arg::new("type")
                    .required(true)
                    .value_parser(["all", "board", "issue", "sprint", "worklog", "google"])
                    .help("The type of items to fetch"),
            )
    }
}

async fn fetch_boards(multi_progress: Option<MultiProgress>) {
    FetchJiraBoard::new()
        .with_progress(multi_progress.unwrap_or(MultiProgress::new()))
        .execute()
        .await
        .unwrap();
}

async fn fetch_sprints(multi_progress: Option<MultiProgress>) {
    let _ = FetchJiraSprint::new()
        .with_progress(multi_progress.unwrap_or(MultiProgress::new()))
        .execute()
        .await;
}

async fn fetch_issues(multi_progress: Option<MultiProgress>) {
    let sprints = JiraService::get_followed_sprint();
    let _ = FetchJiraIssues::new(sprints)
        .with_progress(multi_progress.unwrap_or(MultiProgress::new()))
        .execute()
        .await
        .unwrap();
}

async fn fetch_worklogs(multi_progress: Option<MultiProgress>) {
    let sprints = JiraService::get_followed_sprint();
    FetchJiraWorklogs::new(sprints)
        .with_progress(multi_progress.unwrap_or(MultiProgress::new()))
        .execute()
        .await
        .unwrap();
}

pub async fn fetch_google_meetings(multi_progress: Option<MultiProgress>) -> Result<(), String> {
    let sprints = JiraService::get_followed_sprint();

    // Get min start date and max end date
    let min_date = sprints.iter().filter_map(|s| s.start).min();
    let max_date = sprints.iter().filter_map(|s| s.end).max();

    if let (Some(min), Some(max)) = (min_date, max_date) {
        if let Some(mp) = &multi_progress {
            mp.println(format!(
                "Fetching Google Calendar events from {} to {}...",
                min.format("%Y-%m-%d"),
                max.format("%Y-%m-%d")
            ))
            .ok();
        }
        match FetchGoogleCalendarTask::new(min, max).execute().await {
            Ok(_) => {
                if let Some(mp) = &multi_progress {
                    mp.println("Google Calendar fetch completed").ok();
                }
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Failed to fetch Google Calendar events: {}", e);
                if let Some(mp) = &multi_progress {
                    mp.println(&error_msg).ok();
                }
                Err(error_msg)
            }
        }
    } else {
        let error_msg = "No followed sprints found. Cannot determine date range for Google Calendar fetch.";
        if let Some(mp) = &multi_progress {
            mp.println(error_msg).ok();
            mp.println("Please follow at least one sprint first.").ok();
        }
        Err(error_msg.to_string())
    }
}

async fn fetch_all() {
    info!("starting fetch all");
    let m = MultiProgress::new();

    fetch_boards(Some(m.clone())).await;
    fetch_sprints(Some(m.clone())).await;
    fetch_issues(Some(m.clone())).await;
    fetch_worklogs(Some(m.clone())).await;
    let _ = fetch_google_meetings(Some(m.clone())).await;

    debug!("fetch all finished.")
}
