use crate::commands::Command;
use crate::storage::storage::FileStorage;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use wtf_lib::client::jira_client::JiraClient;
use wtf_lib::config::Config;
use wtf_lib::models::jira::JiraBoard;

pub struct BoardCommand;

#[async_trait]
impl Command for BoardCommand {
    fn name(&self) -> &'static str {
        "board"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("list", sub_matches)) => BoardListCommand.execute(sub_matches).await,
            Some(("fetch", sub_matches)) => BoardFetchCommand.execute(sub_matches).await,
            Some(("add", sub_matches)) => BoardAddCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand for 'board'"),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Manage boards")
            .subcommand(BoardListCommand.clap_command())
            .subcommand(BoardFetchCommand.clap_command())
            .subcommand(BoardAddCommand.clap_command())
    }
}

pub struct BoardListCommand;

#[async_trait]
impl Command for BoardListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        println!("Listing all boards...");
        match FileStorage::load_data::<JiraBoard>("boards") {
            Some(boards) => boards.into_iter().for_each(|b| println!("{}", b)),
            None => println!("No boards found."),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("list").about("List all available boards")
    }
}

struct BoardFetchCommand;

#[async_trait]
impl Command for BoardFetchCommand {
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
        let boards = jira_client.get_all_boards().await.unwrap();
        FileStorage::save_data("boards", boards);
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("fetch").about("Fetch data for boards")
    }
}

struct BoardAddCommand;

#[async_trait]
impl Command for BoardAddCommand {
    fn name(&self) -> &'static str {
        "add"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let id = matches.get_one::<usize>("id").unwrap();

        let available_board = FileStorage::load_data::<JiraBoard>("boards").unwrap_or(Vec::new());

        let board_to_add = available_board.iter().find(|board| &board.id == id);
        match board_to_add {
            Some(board) => {
                let mut followed_boards =
                    FileStorage::load_data::<JiraBoard>("followed_boards").unwrap_or(Vec::new());
                if followed_boards.iter().any(|b| &b.id == id) {
                    println!("Board with ID {} already in your workspace", id);
                } else {
                    followed_boards.push(board.clone());
                    FileStorage::save_data("followed_boards", followed_boards);
                    println!("Board '{}' has been added to your workspace", id);
                }
            }
            None => println!("No board with id {} found.", id),
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
