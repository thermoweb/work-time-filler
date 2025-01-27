use crate::commands::board::BoardCommand;
use crate::commands::config::ConfigCommand;
use crate::commands::fetch::FetchCommand;
use crate::commands::github::GitHubCommand;
use crate::commands::google::GoogleCommand;
use crate::commands::init::InitCommand;
use crate::commands::issue::IssueCommand;
use crate::commands::meeting::MeetingCommand;
use crate::commands::sprint::SprintCommand;
use crate::commands::tui::TuiCommand;
use crate::commands::worklog::LogCommand;
use crate::commands::{Command, CommandRegistry};

mod commands;
mod logger;
mod tasks;
mod tui;

#[tokio::main]
async fn main() {
    // Check for debug flag from environment or CLI
    if std::env::var("RUST_LOG").is_ok() || std::env::var("WTF_DEBUG").is_ok() {
        logger::enable_debug();
    }

    let mut registry = CommandRegistry::new();
    registry.register(TuiCommand);
    registry.register(InitCommand);
    registry.register(FetchCommand);
    registry.register(SprintCommand);
    registry.register(BoardCommand);
    registry.register(IssueCommand);
    registry.register(GitHubCommand);
    registry.register(GoogleCommand);
    registry.register(ConfigCommand);
    registry.register(MeetingCommand);
    registry.register(LogCommand);

    let app = commands::build_app(&registry);
    let matches = app.get_matches();
    
    // Check for global --debug flag
    if matches.get_flag("debug") {
        logger::enable_debug();
    }

    // Determine which command will run
    let command_name = matches.subcommand_name().unwrap_or(TuiCommand.name());
    
    // Initialize env_logger for CLI commands (not for TUI)
    // TUI initializes its own log bridge in Tui::new()
    if command_name != TuiCommand.name() {
        let _ = env_logger::try_init();
    }

    if let Some((name, sub_matches)) = matches.subcommand() {
        registry.execute(name, sub_matches).await;
    } else {
        // No subcommand provided, show tui
        registry.execute(TuiCommand.name(), &matches).await;
    }
}
