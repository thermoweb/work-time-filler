use crate::commands::Command;
use async_trait::async_trait;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use toml::Value;
use wtf_lib::config::{Config, SensitiveString};

pub struct ConfigCommand;

#[async_trait]
impl Command for ConfigCommand {
    fn name(&self) -> &'static str {
        "config"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("list", sub_matches)) => ConfigListCommand.execute(sub_matches).await,
            _ => eprintln!("Invalid subcommand for config"),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Manage configuration")
            .subcommand(ConfigListCommand.clap_command())
    }
}

pub struct ConfigListCommand;

#[async_trait]
impl Command for ConfigListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let show_sensitive = matches.get_flag("show-sensitive");
        let config = Config::load().unwrap();
        let value = Value::try_from(&config).unwrap();
        print_nested("", &value, show_sensitive);
        config.save().unwrap();
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("List available configurations")
            .arg(
                Arg::new("show-sensitive")
                    .short('s')
                    .long("show-sensitive")
                    .help("show sensitive information")
                    .action(ArgAction::SetTrue),
            )
    }
}

fn print_nested(prefix: &str, value: &Value, show_sensitive: bool) {
    match value {
        Value::Table(map) => {
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                print_nested(&new_prefix, val, show_sensitive);
            }
        }
        Value::String(s) => {
            let value_to_show: String = match SensitiveString::decode_str(s) {
                Ok(sensitive_string) => {
                    if show_sensitive {
                        sensitive_string.reveal().to_string()
                    } else {
                        sensitive_string.to_string()
                    }
                }
                Err(_) => s.to_string(),
            };
            println!("{} = {}", prefix, value_to_show);
        }
        _ => println!("{} = {}", prefix, value),
    }
}
