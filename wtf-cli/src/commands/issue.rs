use crate::commands::Command;
use crate::tasks::jira_tasks::FetchJiraIssues;
use crate::tasks::Task;
use async_trait::async_trait;
use chrono::Utc;
use clap::{Arg, ArgMatches, Command as ClapCommand};
use log::{debug, error, info};
use wtf_lib::duration::parse_duration;
use wtf_lib::services::jira_service::{IssueService, JiraService};

pub struct IssueCommand;

#[async_trait]
impl Command for IssueCommand {
    fn name(&self) -> &'static str {
        "issue"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("fetch", sub_matches)) => IssueFetchCommand.execute(sub_matches).await,
            Some(("list", sub_matches)) => IssueListCommand.execute(sub_matches).await,
            Some(("log-time", sub_matches)) => IssueLogTimeCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand for 'issue'"),
        }
    }

    fn clap_command(&self) -> clap::Command {
        clap::Command::new(self.name())
            .about("Manage issues")
            .arg(
                Arg::new("issue-key")
                    .help("Issue Key")
                    .required(false)
                    .index(1),
            )
            .subcommand(IssueFetchCommand.clap_command())
            .subcommand(IssueListCommand.clap_command())
            .subcommand(IssueLogTimeCommand.clap_command())
    }
}

struct IssueFetchCommand;

#[async_trait]
impl Command for IssueFetchCommand {
    fn name(&self) -> &'static str {
        "fetch"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        let sprints = JiraService::get_followed_sprint();
        if sprints.is_empty() {
            println!("No sprint found.");
            return;
        }
        info!("Found {} sprint(s)", sprints.len());

        match FetchJiraIssues::new(sprints).execute().await {
            Ok(()) => info!("Jira issues fetched"),
            Err(err) => println!("Error: {}", err),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        clap::Command::new(self.name()).about("fetch issues")
    }
}

struct IssueListCommand;

#[async_trait]
impl Command for IssueListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        IssueService::get_all_issues()
            .into_iter()
            .for_each(|b| println!("{:?}", b));
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("list").about("List all available issues")
    }
}

struct IssueLogTimeCommand;

#[async_trait]
impl Command for IssueLogTimeCommand {
    fn name(&self) -> &'static str {
        "log-time"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let time = matches.get_one::<String>("time").unwrap();
        let duration = match parse_duration(time) {
            Ok(duration) => duration,
            Err(_e) => panic!("OH !"),
        };
        let issue_key = matches.get_one::<String>("issue-key").unwrap();

        match IssueService::get_by_key(&issue_key) {
            Some(issue) => {
                match IssueService::add_time(issue.key.as_str(), duration, Utc::now(), None).await {
                    Ok(_) => debug!("Logged '{}' for issue '{}'", duration, issue_key),
                    Err(e) => error!("Error: {:?}", e),
                }
            }
            None => error!("No issue found."),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Log time on issue")
            .arg(
                Arg::new("issue-key")
                    .help("Issue Key")
                    .required(true)
                    .index(1),
            )
            .arg(
                Arg::new("time")
                    .help("The time you want to log, e.g., '1h', '30m'")
                    .required(true)
                    .index(2),
            )
    }
}
