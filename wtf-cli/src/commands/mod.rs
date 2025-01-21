pub mod board;
pub mod issue;
pub mod log;
pub mod sprint;

use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wtf_lib::models::jira::JiraBoard;

pub fn build_app(registry: &CommandRegistry) -> ClapCommand {
    let mut app = ClapCommand::new("wtf");
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

#[derive(Serialize, Deserialize, Debug, Default)]
struct AppData {
    pub boards: Vec<JiraBoard>,
}
