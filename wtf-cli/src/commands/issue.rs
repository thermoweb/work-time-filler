use crate::commands::Command;
use crate::storage::storage::FileStorage;
use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command as ClapCommand};
use indicatif::{ProgressIterator, ProgressStyle};
use log::{debug, error, info, warn};
use wtf_lib::client::jira_client::JiraClient;
use wtf_lib::config::Config;
use wtf_lib::duration::parse_duration;
use wtf_lib::models::jira::{JiraIssue, JiraSprint};

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
        let config = match Config::load() {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        };

        let jira_client = JiraClient::new(&config.jira);
        let sprints = FileStorage::load_data::<JiraSprint>("followed_sprint").unwrap_or(Vec::new());

        if sprints.is_empty() {
            println!("No sprint found.");
            return;
        }
        info!("Found {} sprint(s)", sprints.len());

        let mut issues_to_store = Vec::new();
        for sprint in sprints {
            info!("Retrieving issues for sprint {}", sprint);
            match jira_client.get_all_issues(&sprint).await {
                Ok(issue_fetcher) => {
                    if issue_fetcher.len() == 0 {
                        info!("No issues found.");
                    } else {
                        let count = issue_fetcher.len();
                        let issue_progress_style = ProgressStyle::default_bar()
                            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} issues ({percent}%)")
                            .unwrap();
                        for issue in issue_fetcher.progress_with_style(issue_progress_style) {
                            issues_to_store.push(issue);
                        }
                        debug!("{} issues found", count);
                    }
                }
                Err(e) => eprintln!("Error: {:?}", e),
            }
        }

        info!("Storing {} issues", issues_to_store.len());
        FileStorage::save_data("issues", issues_to_store);
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
        match FileStorage::load_data::<JiraIssue>("issues") {
            Some(issues) => issues.into_iter().for_each(|b| println!("{}", b)),
            None => println!("No issue found."),
        }
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

        match FileStorage::load_data::<JiraIssue>("issues") {
            Some(issues) => {
                if let Some(issue) = issues
                    .iter()
                    .find(|i| i.key.to_lowercase() == issue_key.to_lowercase())
                {
                    let config = match Config::load() {
                        Ok(config) => config,
                        Err(err) => {
                            eprintln!("Error: {}", err);
                            std::process::exit(1);
                        }
                    };

                    let jira_client = JiraClient::new(&config.jira);
                    match jira_client.add_time_to_issue(issue.clone(), duration).await {
                        Ok(()) => debug!("Logged '{}' for issue '{}'", duration, issue_key),
                        Err(e) => error!("Error: {:?}", e),
                    }
                } else {
                    error!("No issue found.")
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
