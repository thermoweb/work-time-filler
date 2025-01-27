use crate::commands::Command;
use crate::logger;
use crate::tasks::github_tasks::{
    FetchGithubEventsTask, LogGithubEventsTask, ShowGithubEventsTask, ShowGithubSessionsTask,
};
use crate::tasks::Task;
use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command as ClapCommand};

pub struct GitHubCommand;

#[async_trait]
impl Command for GitHubCommand {
    fn name(&self) -> &'static str {
        "github"
    }

    async fn execute(&self, matches: &ArgMatches) {
        logger::init_logger(logger::stdout_logger());

        match matches.subcommand() {
            Some(("fetch", sub_matches)) => FetchGithubEventsCommand.execute(sub_matches).await,
            Some(("log", sub_matches)) => LogGithubEventsCommand.execute(sub_matches).await,
            Some(("sessions", sub_matches)) => ShowGithubSessionsCommand.execute(sub_matches).await,
            Some(("events", sub_matches)) => ShowGithubEventsCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand. Use 'wtf github --help' for usage."),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("github")
            .alias("gh")
            .about("Manage GitHub events and create worklogs from development activity")
            .subcommand(FetchGithubEventsCommand.clap_command())
            .subcommand(LogGithubEventsCommand.clap_command())
            .subcommand(ShowGithubSessionsCommand.clap_command())
            .subcommand(ShowGithubEventsCommand.clap_command())
    }
}

struct FetchGithubEventsCommand;

#[async_trait]
impl Command for FetchGithubEventsCommand {
    fn name(&self) -> &'static str {
        "fetch"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        FetchGithubEventsTask::new().execute().await.unwrap();
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("Fetch and display GitHub events for followed sprints")
    }
}

struct LogGithubEventsCommand;

#[async_trait]
impl Command for LogGithubEventsCommand {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        LogGithubEventsTask::new().execute().await.unwrap();
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Create worklogs from GitHub development activity")
            .long_about("Analyze GitHub events (commits, PRs, reviews) from followed sprints and create local worklogs. \
                        Attempts to link activities to Jira issues based on branch names, commit messages, and PR titles.")
    }
}

struct ShowGithubSessionsCommand;

#[async_trait]
impl Command for ShowGithubSessionsCommand {
    fn name(&self) -> &'static str {
        "sessions"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let date = matches.get_one::<String>("date").map(|s| s.as_str());
        ShowGithubSessionsTask::new(date).execute().await.unwrap();
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Show stored GitHub work sessions from database")
            .arg(
                Arg::new("date")
                    .short('d')
                    .long("date")
                    .value_name("YYYY-MM-DD")
                    .help("Filter sessions by date"),
            )
    }
}

struct ShowGithubEventsCommand;

#[async_trait]
impl Command for ShowGithubEventsCommand {
    fn name(&self) -> &'static str {
        "events"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let date = matches.get_one::<String>("date").map(|s| s.as_str());
        ShowGithubEventsTask::new(date).execute().await.unwrap();
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Show stored GitHub events from database")
            .arg(
                Arg::new("date")
                    .short('d')
                    .long("date")
                    .value_name("YYYY-MM-DD")
                    .help("Filter events by date"),
            )
    }
}
