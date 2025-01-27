pub mod board;
pub mod config;
pub mod fetch;
pub mod github;
pub mod google;
pub mod init;
pub mod issue;
pub mod meeting;
pub mod sprint;
pub mod tui;
pub mod worklog;

use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use std::collections::HashMap;

pub fn build_app(registry: &CommandRegistry) -> ClapCommand {
    let mut app = ClapCommand::new("wtf")
        .arg(clap::Arg::new("debug")
            .long("debug")
            .help("Enable debug logging")
            .action(clap::ArgAction::SetTrue)
            .global(true));
            
    for subcommand in registry.commands.values() {
        app = app.subcommand(subcommand.clap_command());
    }

    app
}

#[async_trait]
pub trait Command {
    fn name(&self) -> &'static str;
    async fn execute(&self, matches: &ArgMatches);
    fn clap_command(&self) -> ClapCommand;
}

pub struct CommandRegistry {
    pub commands: HashMap<&'static str, Box<dyn Command + Send + Sync>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register<C: Command + Send + Sync + 'static>(&mut self, command: C) {
        self.commands.insert(command.name(), Box::new(command));
    }

    pub async fn execute(&self, name: &str, matches: &ArgMatches) {
        if let Some(command) = self.commands.get(name) {
            command.execute(matches).await;
        } else {
            println!("{} not found", name);
        }
    }
}
