use crate::commands::Command;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use colored::Colorize;

const REPO_URL: &str = "https://github.com/thermoweb/work-time-filler";

pub struct UpdateCommand;

#[async_trait]
impl Command for UpdateCommand {
    fn name(&self) -> &'static str {
        "update"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let allow_prerelease = matches.get_flag("unstable");
        let current = env!("CARGO_PKG_VERSION");
        println!("Current version: {}", current.cyan());
        println!("Checking for updates...");

        let latest = if allow_prerelease {
            wtf_lib::utils::version::check_latest_prerelease_version().await
        } else {
            wtf_lib::utils::version::check_latest_version().await
        };

        match latest {
            None => {
                println!("{}", "✅ Already up to date.".green());
            }
            Some(tag) => {
                println!("New version available: {}", tag.yellow());
                println!("Installing {}...", tag);

                let status = std::process::Command::new("cargo")
                    .args([
                        "install",
                        "--git",
                        REPO_URL,
                        "--tag",
                        &tag,
                        "--locked",
                        "wtf-cli",
                        "--force",
                        "--config",
                        "net.git-fetch-with-cli=true",
                    ])
                    .status();

                match status {
                    Ok(s) if s.success() => {
                        println!("{}", format!("✅ Successfully updated to {}!", tag).green());
                    }
                    Ok(s) => {
                        eprintln!(
                            "{}",
                            format!("❌ cargo install exited with status: {}", s).red()
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "{}",
                            format!("❌ Failed to run cargo: {}. Is cargo installed?", e).red()
                        );
                    }
                }
            }
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Update wtf to the latest version")
            .arg(
                clap::Arg::new("unstable")
                    .long("unstable")
                    .help("Allow updating to pre-release versions")
                    .action(clap::ArgAction::SetTrue),
            )
    }
}
