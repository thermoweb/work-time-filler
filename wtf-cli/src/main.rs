use crate::commands::board::BoardCommand;
use crate::commands::issue::IssueCommand;
use crate::commands::log::LogCommand;
use crate::commands::sprint::SprintCommand;
use crate::commands::CommandRegistry;
use log::error;

mod commands;
mod storage;

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut registry = CommandRegistry::new();
    registry.register(LogCommand);
    registry.register(SprintCommand);
    registry.register(BoardCommand);
    registry.register(IssueCommand);

    let app = commands::build_app(&registry);
    let matches = app.get_matches();

    if let Some((name, sub_matches)) = matches.subcommand() {
        registry.execute(name, sub_matches).await;
    } else {
        error!("No command found");
    }
}
