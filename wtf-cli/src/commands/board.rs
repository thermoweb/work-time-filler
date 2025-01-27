use crate::commands::Command;
use crate::tasks::jira_tasks::FetchJiraBoard;
use crate::tasks::Task;
use async_trait::async_trait;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Color, Modify, Style};
use tabled::{Table, Tabled};
use wtf_lib::models::data::{Board, BoardType};
use wtf_lib::services::jira_service::JiraService;

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
            Some(("rm", sub_matches)) => BoardRemoveCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand for 'board'"),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Manage boards")
            .subcommand(BoardListCommand.clap_command())
            .subcommand(BoardFetchCommand.clap_command())
            .subcommand(BoardAddCommand.clap_command())
            .subcommand(BoardRemoveCommand.clap_command())
    }
}

pub struct BoardListCommand;

#[async_trait]
impl Command for BoardListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let list_all = matches.get_flag("all");
        let boards = if list_all {
            println!("Listing all available boards:");
            JiraService::get_available_boards().unwrap()
        } else {
            println!("Listing followed boards:");
            JiraService::get_followed_boards().unwrap()
        };
        if boards.is_empty() {
            println!("No board found.");
            return;
        }
        let board_data = boards.iter().map(|b| BoardInfo::new(b)).collect::<Vec<_>>();
        let mut table = Table::new(board_data);
        table.with(Style::modern().remove_horizontal());
        table.with(
            Modify::new(Columns::first())
                .with(Color::BOLD | Color::FG_WHITE)
                .with(Alignment::center()),
        );
        println!("{}", table);
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("list").about("List boards").arg(
            Arg::new("all")
                .short('a')
                .long("all")
                .help("list all boards")
                .action(ArgAction::SetTrue),
        )
    }
}

#[derive(Tabled)]
struct BoardInfo {
    id: usize,
    name: String,
    board_type: String,
}

impl BoardInfo {
    fn new(board: &Board) -> Self {
        let board_type = match &board.board_type {
            BoardType::Kanban => "Kanban",
            BoardType::Scrum => "Scrum",
            BoardType::Simple => "Simple",
            _ => "Unknown",
        };
        Self {
            id: board.id.clone(),
            name: board.name.clone(),
            board_type: board_type.to_string(),
        }
    }
}

struct BoardFetchCommand;

#[async_trait]
impl Command for BoardFetchCommand {
    fn name(&self) -> &'static str {
        "fetch"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        let _ = FetchJiraBoard::new().execute().await;
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
        let id = matches.get_one::<String>("id").unwrap();
        match JiraService::follow_board(id) {
            Ok(_) => println!("Board '{}' followed", id),
            Err(e) => println!("{}", e),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("add").about("Fetch data").arg(
            Arg::new("id")
                .required(true)
                .value_parser(clap::value_parser!(String))
                .help("The board id"),
        )
    }
}

struct BoardRemoveCommand;

#[async_trait]
impl Command for BoardRemoveCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let id = matches.get_one::<String>("id").unwrap();

        match JiraService::unfollow_board(id) {
            Ok(..) => println!("Board '{}' unfollow successfully!", id),
            Err(e) => println!("{}", e),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("rm").about("Remove specific board").arg(
            Arg::new("id")
                .required(true)
                .value_parser(clap::value_parser!(String))
                .help("The board id"),
        )
    }
}
