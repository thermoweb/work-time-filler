use crate::commands::fetch::fetch_google_meetings;
use crate::commands::Command;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};

pub struct GoogleCommand;

#[async_trait]
impl Command for GoogleCommand {
    fn name(&self) -> &'static str {
        "google"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("fetch", sub_matches)) => FetchGoogleCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand for 'google'"),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Manage google events")
            .subcommand(FetchGoogleCommand.clap_command())
    }
}

struct FetchGoogleCommand;

#[async_trait]
impl Command for FetchGoogleCommand {
    fn name(&self) -> &'static str {
        "fetch"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        let _ = fetch_google_meetings(None).await;
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("Fetch data from google calendar api")
    }
}
