use crate::commands::Command;
use crate::tasks::worklog_tasks::MeetingWorklogTask;
use crate::tasks::Task;
use async_trait::async_trait;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use colored::Colorize;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use inquire::{Confirm, CustomUserError, Select, Text};
use itertools::Itertools;
use log::debug;
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Display;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Color, Modify, Style};
use tabled::{Table, Tabled};
use wtf_lib::common::common::Common;
use wtf_lib::models::data::{Issue, Meeting, Sprint};
use wtf_lib::services::jira_service::{IssueService, JiraService, SprintService};
use wtf_lib::services::meetings_service::MeetingsService;

pub struct MeetingCommand;

#[async_trait]
impl Command for MeetingCommand {
    fn name(&self) -> &'static str {
        "meeting"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("list", sub_matches)) => ListMeetingCommand.execute(sub_matches).await,
            Some(("link", sub_matches)) => LinkGoogleMeetingsCommand.execute(sub_matches).await,
            Some(("log", sub_matches)) => LogMeetingCommand.execute(sub_matches).await,
            Some(("clear", sub_matches)) => ClearMeetingsCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand"),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("meeting")
            .about("Meeting tool")
            .subcommand(ListMeetingCommand.clap_command())
            .subcommand(LinkGoogleMeetingsCommand.clap_command())
            .subcommand(LogMeetingCommand.clap_command())
            .subcommand(ClearMeetingsCommand.clap_command())
    }
}

pub struct LogMeetingCommand;

#[async_trait]
impl Command for LogMeetingCommand {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn execute(&self, matches: &ArgMatches) {
        // Initialize stdout logger for CLI mode
        crate::logger::init_logger(crate::logger::stdout_logger());

        let sprints = get_sprints_from_id_args(matches);

        match MeetingWorklogTask::new(sprints).execute().await {
            Ok(()) => {}
            Err(e) => eprintln!("{}", e),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("log")
            .about("log time for meetings")
            .arg(create_sprint_ids_arg())
    }
}

pub struct ListMeetingCommand;

#[async_trait]
impl Command for ListMeetingCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, matches: &ArgMatches) {
        debug!("execute meeting list");
        let mut meetings = get_meetings_from_args(matches);

        meetings.sort_by(|a, b| a.start.cmp(&b.start));
        let meetings_data = meetings
            .iter()
            .map(|m| MeetingInfo::from_meeting(&m))
            .collect::<Vec<_>>();
        let mut table = Table::new(meetings_data);
        table.with(Style::modern().remove_horizontal());
        table.with(Modify::new(Columns::new(..)).with(Alignment::center()));
        table.with(Modify::new(Columns::first()).with(Alignment::left()));
        table.with(
            Modify::new(Columns::first())
                .with(Color::BOLD | Color::FG_WHITE)
                .with(Alignment::center()),
        );
        println!("{}", table);
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("List meetings")
            .arg(create_all_arg())
            .arg(create_sprint_ids_arg())
    }
}

impl ListMeetingCommand {}

#[derive(Tabled)]
struct MeetingInfo {
    title: String,
    jira_link: String,
    start: String,
    end: String,
    recurrence: String,
}

impl MeetingInfo {
    fn from_meeting(meeting: &Meeting) -> Self {
        MeetingInfo {
            title: meeting.title.clone().unwrap_or("no title".to_string()),
            jira_link: meeting
                .jira_link
                .clone()
                .unwrap_or("None".red().to_string()),
            start: Common::format_date_time(&meeting.start),
            end: Common::format_date_time(&meeting.end),
            recurrence: meeting
                .recurrence
                .clone()
                .map(|r| r.join(""))
                .map(|_| "Yes".to_string())
                .unwrap_or("".to_string()),
        }
    }
}

pub struct LinkGoogleMeetingsCommand;

#[async_trait]
impl Command for LinkGoogleMeetingsCommand {
    fn name(&self) -> &'static str {
        "link"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let meetings = get_meetings_from_args(matches);
        let mut link_cache: HashMap<String, String> = HashMap::new();

        // Build a map of recurring meeting titles to their jira links
        let mut recurring_meeting_links: HashMap<String, String> = HashMap::new();
        for meeting in &meetings {
            if meeting.recurrence.is_some() && meeting.jira_link.is_some() {
                if let Some(ref title) = meeting.title {
                    recurring_meeting_links
                        .insert(title.clone(), meeting.jira_link.as_ref().unwrap().clone());
                }
            }
        }

        for meeting in meetings {
            // Skip meetings that look like absences
            if let Some(ref title) = meeting.title {
                if title.contains("Absence") || title.contains("absent") {
                    debug!("Skipping absence event: '{}'", title);
                    continue;
                }

                // For recurring meetings, check if we already have a link for this title
                if meeting.recurrence.is_some() && meeting.jira_link.is_none() {
                    if let Some(existing_link) = recurring_meeting_links.get(title) {
                        debug!(
                            "Reusing link '{}' for recurring meeting '{}'",
                            existing_link, title
                        );
                        link_issue(meeting, existing_link);
                        continue;
                    }
                }
            }

            if let Some(link) = meeting.jira_link {
                debug!(
                    "meeting '{:?}' already linked with issue '{}'",
                    meeting.title, link
                );
                continue;
            }

            let meeting_id = meeting.id.clone();
            if let Some(cached_link) = link_cache.get(&meeting_id) {
                debug!(
                    "meeting '{}' already linked with cache : {}",
                    meeting_id, cached_link
                );
                link_issue(meeting, cached_link);
                continue;
            }

            let result = Self::find_issue_for_meeting(meeting.clone()).await;
            if let Some(value) = result {
                link_issue(meeting.clone(), value.as_str());
                link_cache.insert(meeting_id, value.clone());

                // If this is a recurring meeting, cache by title too
                if meeting.recurrence.is_some() {
                    if let Some(ref title) = meeting.title {
                        recurring_meeting_links.insert(title.clone(), value);
                    }
                }
            }
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Link meetings to jira issues")
            .arg(create_sprint_ids_arg())
            .arg(create_all_arg())
    }
}

impl LinkGoogleMeetingsCommand {
    async fn find_issue_for_meeting(meeting: Meeting) -> Option<String> {
        let title = meeting.clone().title.unwrap_or("no title".to_string());
        let candidates = get_jira_candidates(&meeting).await;
        if !candidates.is_empty() {
            debug!("found candidates: {:?}", candidates);
            if candidates.len() > 1 {
                let choices = candidates
                    .iter()
                    .map(|i| IssueChoice::from(i.clone()))
                    .collect::<Vec<IssueChoice>>();
                let link_to_message = format!(
                    "Select the issue to link to meeting '{}':",
                    meeting
                        .title
                        .unwrap_or(meeting.description.unwrap_or(meeting.id))
                );
                let selected_option = Select::new(link_to_message.as_str(), choices).prompt();
                match selected_option {
                    Ok(selected_choice) => Some(selected_choice.key),
                    Err(_) => {
                        println!("Skipped");
                        None
                    }
                }
            } else {
                let issue_to_link = candidates.get(0).unwrap();
                let question = format!(
                    "Link meeting '{}' to issue '[{}]{}'",
                    title.magenta(),
                    issue_to_link.key,
                    issue_to_link.summary.cyan()
                );
                let ans = Confirm::new(question.as_str()).with_default(true).prompt();
                match ans {
                    Ok(true) => Some(issue_to_link.key.clone()),
                    Ok(false) => {
                        println!("Skipped");
                        None
                    }
                    Err(_) => {
                        println!("Skipped");
                        None
                    }
                }
            }
        } else {
            println!(
                "\nNo Jira candidates found for meeting '{}'",
                title.magenta()
            );
            println!("You can:");
            println!("  - Type an issue key (e.g., PROJ-123) to link");
            println!("  - Press Enter to skip this meeting");

            let issue_selected = Text::new("Issue to link (or Enter to skip): ")
                .with_autocomplete(&issue_suggestor)
                .with_page_size(10)
                .prompt();

            match issue_selected {
                Ok(input) if input.trim().is_empty() => {
                    println!("Skipped");
                    None
                }
                Ok(input) => {
                    let regex = Regex::new(r"\[([a-zA-Z]+-[0-9]+)]").unwrap();
                    if let Some(caps) = regex.captures(&input) {
                        if let Some(issue_id) = caps.get(1) {
                            return Some(issue_id.as_str().to_string());
                        }
                    }
                    // Also try to match direct issue key like "PROJ-123"
                    let direct_regex = Regex::new(r"^([a-zA-Z]+-[0-9]+)$").unwrap();
                    if let Some(caps) = direct_regex.captures(&input) {
                        if let Some(issue_id) = caps.get(1) {
                            return Some(issue_id.as_str().to_string());
                        }
                    }
                    println!("Invalid issue format, skipped");
                    None
                }
                Err(_) => {
                    println!("Skipped");
                    None
                }
            }
        }
    }
}

fn issue_suggestor(input: &str) -> Result<Vec<String>, CustomUserError> {
    let input = input.to_lowercase();
    Ok(IssueService::get_all_issues()
        .iter()
        .filter(|issue| {
            issue.summary.to_lowercase().contains(&input)
                || issue.key.to_lowercase().contains(&input)
        })
        .map(|issue| format!("[{}] {}", issue.key, issue.summary))
        .take(10)
        .collect())
}

async fn get_jira_candidates(meeting: &Meeting) -> Vec<Issue> {
    meeting
        .get_jira_candidates()
        .iter()
        .unique()
        .map(|key| JiraService::get_issue_by_key(key))
        .collect::<FuturesUnordered<_>>()
        .filter_map(|i| async { i })
        .collect::<Vec<Issue>>()
        .await
}

struct IssueChoice {
    key: String,
    title: String,
}

impl Display for IssueChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("[{}] {}", self.key, self.title))
    }
}

impl IssueChoice {
    fn from(issue: Issue) -> Self {
        IssueChoice {
            key: issue.key.to_string(),
            title: issue.summary.to_string(),
        }
    }
}

fn link_issue(mut meeting: Meeting, issue_key: &str) {
    meeting.jira_link = Some(issue_key.to_string());
    MeetingsService::save(&meeting);
    println!(
        "meeting '{}' linked to issue '{}'",
        meeting.title.unwrap_or(meeting.id),
        issue_key
    )
}

fn get_sprints_from_id_args(matches: &ArgMatches) -> Vec<Sprint> {
    let sprints = if matches.contains_id("sprint-id") {
        debug!("sprint-id found");
        matches
            .get_many::<String>("sprint-id")
            .unwrap()
            .cloned()
            .filter_map(|sprint_id| SprintService::get_sprint(&sprint_id).unwrap())
            .collect::<Vec<_>>()
    } else {
        debug!("sprint-id not found -> followed sprint returned");
        JiraService::get_followed_sprint()
    };
    sprints
}

fn get_meetings_from_args(matches: &ArgMatches) -> Vec<Meeting> {
    if matches.get_flag("all") {
        MeetingsService::get_all_meetings()
    } else {
        debug!("no 'all' flag");
        get_sprints_from_id_args(matches)
            .iter()
            .flat_map(|s| MeetingsService::get_meetings_for_sprint(s))
            .collect::<Vec<_>>()
    }
}

pub struct ClearMeetingsCommand;

#[async_trait]
impl Command for ClearMeetingsCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        println!("Clearing meetings database...");
        MeetingsService::clear_all_meetings();
        println!("✓ Meetings database cleared. Run 'wtf fetch google' to rebuild.");
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("clear")
            .about("Clear all meetings from database (use when migrating schema)")
    }
}

fn create_all_arg() -> Arg {
    Arg::new("all")
        .short('a')
        .long("all")
        .help("list all meetings")
        .action(ArgAction::SetTrue)
}

fn create_sprint_ids_arg() -> Arg {
    Arg::new("sprint-id")
        .short('s')
        .long("sprint-id")
        .help("The sprint id")
        .value_parser(clap::value_parser!(String))
        .num_args(1..)
}
