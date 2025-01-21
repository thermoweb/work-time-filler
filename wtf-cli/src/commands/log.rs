use crate::commands::Command;
use async_trait::async_trait;
use clap::ArgMatches;
use log::info;
use wtf_lib::config::Config;
use wtf_lib::services::jira_service::JiraService;

pub struct LogCommand;

#[async_trait]
impl Command for LogCommand {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        info!("loading configuration");
        let config = match Config::load() {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        };
        info!("Logging work times");
        let jira_service = JiraService::new(&config.jira);

        match jira_service
            .get_worklogs(config.jira.username.clone())
            .await
        {
            Ok(worklogs) => worklogs.iter().for_each(|worklog| println!("{}", worklog)),
            Err(e) => eprintln!("Error: {:?}", e),
        }
    }

    fn clap_command(&self) -> clap::Command {
        clap::Command::new("log").about("logging")
    }
}
