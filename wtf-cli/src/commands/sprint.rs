use crate::commands::Command;
use crate::tasks::jira_tasks::{FetchJiraSprint, ListJiraSprints};
use crate::tasks::Task;
use async_trait::async_trait;
use chrono::Duration;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use colored::Colorize;
use std::ops::Sub;
use wtf_lib::services::jira_service::{JiraService, SprintService};
use wtf_lib::services::worklogs_service::WorklogsService;

pub struct SprintCommand;

#[async_trait]
impl Command for SprintCommand {
    fn name(&self) -> &'static str {
        "sprint"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("list", sub_matches)) => SprintListCommand.execute(sub_matches).await,
            Some(("fetch", sub_matches)) => SprintFetchCommand.execute(sub_matches).await,
            Some(("add", sub_matches)) => SprintAddCommand.execute(sub_matches).await,
            Some(("rm", sub_matches)) => SprintRemoveCommand.execute(sub_matches).await,
            Some(("status", sub_matches)) => SprintStatusCommand.execute(sub_matches).await,
            Some(("clear-worklogs", sub_matches)) => {
                SprintClearWorklogsCommand.execute(sub_matches).await
            }
            _ => eprintln!("Invalid subcommand for 'sprint'"),
        }
    }

    fn clap_command(&self) -> clap::Command {
        clap::Command::new(self.name())
            .about("Sprint board")
            .subcommand(SprintListCommand.clap_command())
            .subcommand(SprintFetchCommand.clap_command())
            .subcommand(SprintAddCommand.clap_command())
            .subcommand(SprintRemoveCommand.clap_command())
            .subcommand(SprintStatusCommand.clap_command())
            .subcommand(SprintClearWorklogsCommand.clap_command())
    }
}

struct SprintListCommand;

#[async_trait]
impl Command for SprintListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let fetch_all = matches.get_flag("all");
        ListJiraSprints::new(fetch_all).execute().await.unwrap();
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("list").about("List sprints").arg(
            Arg::new("all")
                .short('a')
                .long("all")
                .help("list all sprints")
                .action(ArgAction::SetTrue),
        )
    }
}

struct SprintFetchCommand;

#[async_trait]
impl Command for SprintFetchCommand {
    fn name(&self) -> &'static str {
        "fetch"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        let _ = FetchJiraSprint::new().execute().await;
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("fetch").about("Fetch data for sprints")
    }
}

struct SprintAddCommand;
#[async_trait]
impl Command for SprintAddCommand {
    fn name(&self) -> &'static str {
        "add"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let ids: Vec<String> = matches
            .get_many("ids")
            .expect("at least one id is required")
            .cloned()
            .collect();
        for id in ids {
            if let Some(mut sprint) = SprintService::get_sprint_by_id(&id) {
                sprint.followed = true;
                SprintService::save_sprint(&sprint);
                println!("sprint \"{}\" added successfully", sprint.id);
            } else {
                println!("Sprint '{}' not found", id);
            }
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("add")
            .about("Fetch data for a specific board")
            .arg(
                Arg::new("ids")
                    .required(true)
                    .value_parser(clap::value_parser!(String))
                    .num_args(1..)
                    .help("The board id"),
            )
    }
}

struct SprintRemoveCommand;

#[async_trait]
impl Command for SprintRemoveCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let ids: Vec<String> = matches
            .get_many::<String>("ids")
            .unwrap()
            .map(|s| s.to_string())
            .collect();

        // Check if "all" is specified
        if ids.len() == 1 && ids[0] == "all" {
            let followed_sprints = JiraService::get_followed_sprint();
            if followed_sprints.is_empty() {
                println!("No followed sprints to remove.");
                return;
            }

            let count = followed_sprints.len();
            for sprint in followed_sprints {
                match JiraService::unfollow_sprint(&sprint.id.to_string()) {
                    Ok(..) => {}
                    Err(e) => eprintln!("Error unfollowing sprint {}: {}", sprint.id, e),
                }
            }
            println!("Successfully unfollowed {} sprint(s)!", count);
            return;
        }

        // Handle individual IDs
        let mut success_count = 0;
        let mut error_count = 0;

        for id in &ids {
            match JiraService::unfollow_sprint(id) {
                Ok(..) => {
                    println!("Sprint {} unfollowed successfully!", id);
                    success_count += 1;
                }
                Err(e) => {
                    eprintln!("Error unfollowing sprint {}: {}", id, e);
                    error_count += 1;
                }
            }
        }

        if ids.len() > 1 {
            println!(
                "\nSummary: {} succeeded, {} failed",
                success_count, error_count
            );
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("rm").about("Remove sprint(s)").arg(
            Arg::new("ids")
                .required(true)
                .value_parser(clap::value_parser!(String))
                .num_args(1..)
                .help("Sprint ID(s) to unfollow, or 'all' to unfollow all sprints"),
        )
    }
}

struct SprintStatusCommand;

#[async_trait]
impl Command for SprintStatusCommand {
    fn name(&self) -> &'static str {
        "status"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let sprint_id = matches.get_one::<String>("id").unwrap();

        let min_hours = Duration::hours(7);
        let max_hours = Duration::hours(8);

        match SprintService::get_sprint(sprint_id) {
            Ok(sprint) => match sprint {
                None => println!("Sprint '{}' not found!", sprint_id),
                Some(current_sprint) => {
                    println!("sprint -> {:?}", current_sprint);
                    if let (Some(mut current), Some(end)) =
                        (current_sprint.start, current_sprint.end)
                    {
                        while current <= end {
                            let worklogs =
                                WorklogsService::get_worklogs_by_date(current.date_naive());
                            let time_spent_seconds =
                                worklogs.iter().map(|wl| wl.time_spent_seconds).sum::<u64>() as i64;
                            let time_spent = Duration::new(time_spent_seconds, 0).unwrap();
                            let day_status = if time_spent < min_hours {
                                let missing = min_hours.sub(time_spent);
                                format!(
                                    "missing at least {}h{:0>2}m ",
                                    missing.num_hours(),
                                    missing.num_minutes() % 60
                                )
                                .red()
                            } else if time_spent < max_hours {
                                "good".green()
                            } else {
                                "overload".green()
                            };
                            println!(
                                "{} => {} worklog(s) for a total {}h - {}",
                                current.format("%d-%m-%Y"),
                                worklogs.len(),
                                time_spent.num_hours(),
                                day_status
                            );

                            current += Duration::days(1);
                        }
                    }
                }
            },
            Err(e) => println!("{}", e),
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("status").about("give sprint status").arg(
            Arg::new("id")
                .required(true)
                .value_parser(clap::value_parser!(String))
                .help("The sprint id"),
        )
    }
}

struct SprintClearWorklogsCommand;

#[async_trait]
impl Command for SprintClearWorklogsCommand {
    fn name(&self) -> &'static str {
        "clear-worklogs"
    }

    async fn execute(&self, matches: &ArgMatches) {
        use std::collections::HashMap;
        use wtf_lib::common::common::Common;
        use wtf_lib::services::jira_service::IssueService;
        use wtf_lib::services::worklogs_service::LocalWorklogService;

        let sprint_id = matches.get_one::<String>("id").unwrap();
        let dry_run = matches.get_flag("dry-run");

        // Get the sprint
        let sprint = match SprintService::get_sprint_by_id(sprint_id) {
            Some(s) => s,
            None => {
                eprintln!("Sprint {} not found", sprint_id);
                return;
            }
        };

        let (start_date, end_date) = match (sprint.start, sprint.end) {
            (Some(start), Some(end)) => (start.date_naive(), end.date_naive()),
            _ => {
                eprintln!("Sprint {} has no start/end dates", sprint.name);
                return;
            }
        };

        println!(
            "Sprint: {} ({} to {})",
            sprint.name.bright_cyan(),
            start_date,
            end_date
        );
        println!();

        // Build a map of issue_id -> issue_key for lookups
        let all_issues = IssueService::get_all_issues();
        let issue_id_to_key: HashMap<String, String> = all_issues
            .iter()
            .map(|i| (i.id.clone(), i.key.clone()))
            .collect();

        // Get current user's email from config for filtering
        let config = wtf_lib::config::Config::load().expect("Failed to load config");
        let current_user_email = config.jira.username.clone();

        // Get all worklogs (Jira + Local) in the sprint date range
        let jira_worklogs = WorklogsService::get_all_worklogs();
        let local_worklogs = LocalWorklogService::get_all_local_worklogs();

        // Filter by date range AND by author (only your worklogs)
        let sprint_jira_wl: Vec<_> = jira_worklogs
            .iter()
            .filter(|w| {
                let date = w.started.date_naive();
                let is_in_range = date >= start_date && date <= end_date;
                let is_mine = w.author == current_user_email;
                is_in_range && is_mine
            })
            .collect();

        let sprint_local_wl: Vec<_> = local_worklogs
            .iter()
            .filter(|w| {
                let date = w.started.date_naive();
                date >= start_date && date <= end_date
            })
            .collect();

        let total_jira: i64 = sprint_jira_wl
            .iter()
            .map(|w| w.time_spent_seconds as i64)
            .sum();
        let total_local: i64 = sprint_local_wl.iter().map(|w| w.time_spent_seconds).sum();

        println!("Found:");
        println!(
            "  • {} Jira worklogs ({})",
            sprint_jira_wl.len(),
            Common::readable_time_spent(total_jira)
        );
        println!(
            "  • {} local worklogs ({})",
            sprint_local_wl.len(),
            Common::readable_time_spent(total_local)
        );
        println!();

        if sprint_jira_wl.is_empty() && sprint_local_wl.is_empty() {
            println!("✓ No worklogs to clear!");
            return;
        }

        if dry_run {
            println!("🔍 DRY RUN - showing what would be deleted:");
            println!();

            // Get config to build DELETE URLs for preview
            let config = wtf_lib::config::Config::load().expect("Failed to load config");
            let base_url = &config.jira.base_url;

            if !sprint_jira_wl.is_empty() {
                println!("Jira worklogs to delete:");
                for (idx, wl) in sprint_jira_wl.iter().enumerate() {
                    // Get issue key from issue_id
                    let issue_key = issue_id_to_key
                        .get(&wl.issue_id)
                        .cloned()
                        .unwrap_or_else(|| wl.issue_id.clone());

                    let delete_url = format!(
                        "{}/rest/api/3/issue/{}/worklog/{}",
                        base_url, issue_key, wl.id
                    );

                    println!(
                        "  • {} - {} - {} ({})",
                        wl.started.format("%Y-%m-%d %H:%M"),
                        wl.issue_id,
                        wl.comment.as_ref().unwrap_or(&"<no comment>".to_string()),
                        Common::readable_time_spent(wl.time_spent_seconds as i64)
                    );

                    // Show first 5 URLs as examples
                    if idx < 5 {
                        println!("      DELETE: {}", delete_url);
                    }
                }
                println!();
                if sprint_jira_wl.len() > 5 {
                    println!("  (showing first 5 DELETE URLs only)");
                    println!();
                }
            }

            if !sprint_local_wl.is_empty() {
                println!("Local worklogs to delete:");
                for wl in &sprint_local_wl {
                    println!(
                        "  • {} - {} - {} ({})",
                        wl.started.format("%Y-%m-%d %H:%M"),
                        wl.issue_id,
                        wl.comment,
                        Common::readable_time_spent(wl.time_spent_seconds)
                    );
                }
                println!();
            }

            println!("Run without --dry-run to actually delete these worklogs.");
        } else {
            println!("⚠️  WARNING: This will delete ALL worklogs for this sprint from Jira AND local database!");
            println!("This action CANNOT be undone!");
            println!();
            print!("Type the sprint name '{}' to confirm: ", sprint.name);

            use std::io::{self, Write};
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            if input != sprint.name {
                println!("Confirmation failed. Aborting.");
                return;
            }

            println!();
            println!("Deleting worklogs...");

            // Delete from Jira first
            let total_jira = sprint_jira_wl.len();
            let mut deleted_jira = 0;
            for (idx, wl) in sprint_jira_wl.iter().enumerate() {
                // Get issue key from issue_id (Jira API needs the key, not the numeric ID)
                let issue_key = issue_id_to_key
                    .get(&wl.issue_id)
                    .cloned()
                    .unwrap_or_else(|| wl.issue_id.clone()); // fallback to issue_id if not found

                IssueService::delete_worklog(&issue_key, &wl.id).await;
                deleted_jira += 1;

                // Print progress every 10 worklogs or on last one
                if (idx + 1) % 10 == 0 || idx + 1 == total_jira {
                    print!(
                        "\r  Deleting from Jira: {}/{} worklogs...",
                        idx + 1,
                        total_jira
                    );
                    io::stdout().flush().unwrap();
                }
            }
            if total_jira > 0 {
                println!(); // newline after progress
            }

            // Delete local worklogs
            let total_local = sprint_local_wl.len();
            let mut deleted_local = 0;
            for (idx, wl) in sprint_local_wl.iter().enumerate() {
                LocalWorklogService::remove_local_worklog(wl);
                deleted_local += 1;

                // Print progress every 10 worklogs or on last one
                if (idx + 1) % 10 == 0 || idx + 1 == total_local {
                    print!(
                        "\r  Deleting local worklogs: {}/{} worklogs...",
                        idx + 1,
                        total_local
                    );
                    io::stdout().flush().unwrap();
                }
            }
            if total_local > 0 {
                println!(); // newline after progress
            }

            println!();
            println!("✓ Deleted {} Jira worklogs", deleted_jira);
            println!("✓ Deleted {} local worklogs", deleted_local);
            println!("✓ Sprint worklogs cleared!");
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new("clear-worklogs")
            .about("Delete ALL worklogs for a sprint from Jira and local database")
            .arg(
                Arg::new("id")
                    .required(true)
                    .value_parser(clap::value_parser!(String))
                    .help("The sprint id"),
            )
            .arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .help("Show what would be deleted without actually deleting")
                    .action(ArgAction::SetTrue),
            )
    }
}
