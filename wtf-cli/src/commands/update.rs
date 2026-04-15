use crate::commands::Command;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use colored::Colorize;
use inquire;

const REPO_URL: &str = "https://github.com/thermoweb/work-time-filler";

pub struct UpdateCommand;

#[async_trait]
impl Command for UpdateCommand {
    fn name(&self) -> &'static str {
        "update"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("list", sub_matches)) => {
                let include_prerelease = sub_matches.get_flag("unstable");
                println!("Fetching available versions...");
                let mut versions =
                    wtf_lib::utils::version::list_versions(include_prerelease).await;
                versions.sort_by(|a, b| {
                    let a = a.trim_start_matches('v');
                    let b = b.trim_start_matches('v');
                    if wtf_lib::utils::version::is_older_than(a, b) {
                        std::cmp::Ordering::Greater
                    } else if wtf_lib::utils::version::is_older_than(b, a) {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });
                if versions.is_empty() {
                    println!("{}", "No versions found.".yellow());
                } else {
                    let current = env!("CARGO_PKG_VERSION");
                    for v in &versions {
                        let tag = v.trim_start_matches('v');
                        if tag == current {
                            println!("  {} {}", v.green(), "(current)".dimmed());
                        } else {
                            println!("  {}", v);
                        }
                    }
                }
            }
            _ => {
                let allow_prerelease = matches.get_flag("unstable");
                let target_version = matches.get_one::<String>("version");
                let current = env!("CARGO_PKG_VERSION");
                println!("Current version: {}", current.cyan());

                let tag = match target_version.map(|s| s.as_str()) {
                    Some("") => {
                        // -v without value: show interactive picker
                        println!("Fetching available versions...");
                        let mut versions =
                            wtf_lib::utils::version::list_versions(allow_prerelease).await;
                        versions.sort_by(|a, b| {
                            let a = a.trim_start_matches('v');
                            let b = b.trim_start_matches('v');
                            if wtf_lib::utils::version::is_older_than(a, b) {
                                std::cmp::Ordering::Greater
                            } else if wtf_lib::utils::version::is_older_than(b, a) {
                                std::cmp::Ordering::Less
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        });
                        if versions.is_empty() {
                            eprintln!("{}", "❌ No versions found.".red());
                            return;
                        }
                        match inquire::Select::new("Select version to install:", versions).prompt() {
                            Ok(v) => Some(v),
                            Err(_) => {
                                println!("Cancelled.");
                                return;
                            }
                        }
                    }
                    Some(v) => {
                        // -v <value>: install directly
                        let tag = if v.starts_with('v') {
                            v.to_string()
                        } else {
                            format!("v{}", v)
                        };
                        println!("Installing {}...", tag);
                        Some(tag)
                    }
                    None => {
                        // no -v: update to latest
                        println!("Checking for updates...");
                        let latest = if allow_prerelease {
                            wtf_lib::utils::version::check_latest_prerelease_version().await
                        } else {
                            wtf_lib::utils::version::check_latest_version().await
                        };
                        match latest {
                            None => {
                                println!("{}", "✅ Already up to date.".green());
                                None
                            }
                            Some(t) => {
                                println!("New version available: {}", t.yellow());
                                println!("Installing {}...", t);
                                Some(t)
                            }
                        }
                    }
                };

                if let Some(tag) = tag {
                    install(&tag);
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
                    .short('u')
                    .help("Allow updating to pre-release versions")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("version")
                    .long("version")
                    .short('v')
                    .help("Install a specific version (e.g. 0.1.1 or 0.1.1-beta.3), or omit value for interactive picker")
                    .value_name("VERSION")
                    .num_args(0..=1)
                    .default_missing_value(""),
            )
            .subcommand(
                ClapCommand::new("list")
                    .about("List all available versions")
                    .arg(
                        clap::Arg::new("unstable")
                            .long("unstable")
                            .short('u')
                            .help("Include pre-release versions")
                            .action(clap::ArgAction::SetTrue),
                    ),
            )
    }
}

fn install(tag: &str) {
    let status = std::process::Command::new("cargo")
        .args([
            "install",
            "--git",
            REPO_URL,
            "--tag",
            tag,
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
