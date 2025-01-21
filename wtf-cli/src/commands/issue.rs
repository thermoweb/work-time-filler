use crate::commands::Command;
use crate::storage::storage::FileStorage;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use indicatif::{ProgressIterator, ProgressStyle};
use log::{debug, info};
use wtf_lib::client::jira_client::JiraClient;
use wtf_lib::config::Config;
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
            _ => eprintln!("Invalid subcommand for 'sprint'"),
        }
    }

    fn clap_command(&self) -> clap::Command {
        clap::Command::new(self.name())
            .about("issues")
            .subcommand(IssueFetchCommand.clap_command())
            .subcommand(IssueListCommand.clap_command())
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
