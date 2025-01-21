use crate::commands::Command;
use crate::storage::storage::FileStorage;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use indicatif::{ProgressIterator, ProgressStyle};
use log::info;
use wtf_lib::client::jira_client::JiraClient;
use wtf_lib::config::Config;
use wtf_lib::models::jira::{JiraBoard, JiraSprint};

pub struct SprintCommand;

#[async_trait]
impl Command for SprintCommand {
    fn name(&self) -> &'static str {
        "sprint"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("list", sub_matches)) => SprintListCommand.execute(sub_matches).await,
            Some(("fetch", sub_matches)) => SprintFetchCommand.execute(sub_matches).await,
            Some(("add", sub_matches)) => SprintAddCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand for 'sprint'"),
        }
    }

    fn clap_command(&self) -> clap::Command {
        clap::Command::new(self.name())
            .about("Sprint board")
            .subcommand(SprintListCommand.clap_command())
            .subcommand(SprintFetchCommand.clap_command())
            .subcommand(SprintAddCommand.clap_command())
    }
}

struct SprintListCommand;

#[async_trait]
impl Command for SprintListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        match FileStorage::load_data::<JiraSprint>("sprints") {
            Some(sprints) => sprints.into_iter().for_each(|b| println!("{}", b)),
            None => println!("No sprint found."),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("list").about("List all available sprints")
    }
}

struct SprintFetchCommand;

#[async_trait]
impl Command for SprintFetchCommand {
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
        let boards =
            FileStorage::load_data::<JiraBoard>("followed_boards").unwrap_or_else(Vec::new);

        if boards.is_empty() {
            println!("No boards found.");
            return;
        }
        info!("Found {} board(s)", boards.len());

        let mut sprints_to_store = Vec::new();
        for board in boards {
            match jira_client.get_all_sprint(board.id).await {
                Ok(sprints_fetcher) => {
                    info!(
                        "retrieving {} sprints for board {}",
                        sprints_fetcher.len(),
                        board
                    );
                    let sprint_progress_style = ProgressStyle::default_bar()
                        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} sprints ({percent}%)")
                        .unwrap();
                    for sprint in sprints_fetcher.progress_with_style(sprint_progress_style) {
                        sprints_to_store.push(sprint);
                    }
                }
                Err(e) => eprintln!("Error: {:?}", e),
            }
        }

        info!("Storing {} sprints", sprints_to_store.len());
        FileStorage::save_data("sprints", sprints_to_store);
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("fetch").about("Fetch data for sprints")
    }
}

struct SprintAddCommand;

#[async_trait]
impl Command for SprintAddCommand {
    fn name(&self) -> &'static str {
        "add"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let id = matches.get_one::<usize>("id").unwrap();

        let available_sprints =
            FileStorage::load_data::<JiraSprint>("sprints").unwrap_or_else(Vec::new);

        let sprint_to_add = available_sprints.iter().find(|board| &board.id == id);
        match sprint_to_add {
            Some(sprint) => {
                let mut followed_sprints = FileStorage::load_data::<JiraSprint>("followed_sprint")
                    .unwrap_or_else(Vec::new);
                if followed_sprints.iter().any(|b| &b.id == id) {
                    println!("Sprint with ID {} already in your workspace", id);
                } else {
                    followed_sprints.push(sprint.clone());
                    FileStorage::save_data("followed_sprint", followed_sprints);
                    println!("Sprint '{}' has been added to your workspace", id);
                }
            }
            None => println!("No sprint with id {} found.", id),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("add")
            .about("Fetch data for a specific board")
            .arg(
                clap::Arg::new("id")
                    .required(true)
                    .value_parser(clap::value_parser!(usize))
                    .help("The board id"),
            )
    }
}
