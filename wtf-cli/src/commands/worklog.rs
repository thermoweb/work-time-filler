use crate::commands::Command;
use async_trait::async_trait;
use chrono::Duration;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use colored::{ColoredString, Colorize};
use log::debug;
use std::collections::HashMap;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Color, Modify, Style};
use tabled::{Table, Tabled};
use wtf_lib::common::common::Common;
use wtf_lib::models::data::{LocalWorklog, LocalWorklogHistory, LocalWorklogState};
use wtf_lib::services::jira_service::IssueService;
use wtf_lib::services::worklogs_service::LocalWorklogService;
use LocalWorklogState::Created;
use LocalWorklogState::Pushed;
use LocalWorklogState::Staged;

pub struct LogCommand;

#[async_trait]
impl Command for LogCommand {
    fn name(&self) -> &'static str {
        "worklog"
    }

    async fn execute(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            Some(("reset", sub_m)) => LogResetCommand.execute(sub_m).await,
            Some(("add", sub_m)) => LogAddCommand.execute(sub_m).await,
            Some(("rm", sub_m)) => LogRemoveCommand.execute(sub_m).await,
            Some(("push", sub_m)) => LogPushCommand.execute(sub_m).await,
            Some(("list", sub_m)) => LogListCommand.execute(sub_m).await,
            Some(("revert", sub_m)) => LogRevertCommand.execute(sub_m).await,
            Some(("history", sub_m)) => LogHistoryCommand.execute(sub_m).await,
            Some(("cleanup", sub_m)) => LogCleanupCommand.execute(sub_m).await,
            _ => LogListCommand.execute(matches).await,
        }
    }

    fn clap_command(&self) -> clap::Command {
        clap::Command::new(self.name())
            .about("worklog management")
            .alias("wl")
            .arg(
                Arg::new("all")
                    .short('a')
                    .long("all")
                    .help("list all worklogs")
                    .action(ArgAction::SetTrue),
            )
            .subcommand(LogResetCommand.clap_command())
            .subcommand(LogAddCommand.clap_command())
            .subcommand(LogRemoveCommand.clap_command())
            .subcommand(LogPushCommand.clap_command())
            .subcommand(LogListCommand.clap_command())
            .subcommand(LogRevertCommand.clap_command())
            .subcommand(LogHistoryCommand.clap_command())
            .subcommand(LogCleanupCommand.clap_command())
    }
}

pub struct LogListCommand;

#[async_trait]
impl Command for LogListCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let list_all = matches.get_flag("all");
        let mut db_wl = if list_all {
            LocalWorklogService::get_all_local_worklogs()
        } else {
            LocalWorklogService::get_all_local_worklogs_by_status(vec![Created, Staged])
        };
        db_wl.sort_by(|a, b| a.started.cmp(&b.started));
        let (status_stats, total_time_spent) = compute_worklogs_stats(db_wl.clone());

        let wl: Vec<WorklogInfo> = db_wl.iter().map(|w| WorklogInfo::new(w.clone())).collect();

        let mut table = Table::new(wl);
        table.with(Style::modern().remove_horizontal());
        table.with(Modify::new(Columns::new(..)).with(Alignment::center()));
        table.with(
            Modify::new(Columns::first())
                .with(Color::BOLD | Color::FG_WHITE)
                .with(Alignment::center()),
        );
        println!("{table}");
        println!("{} worklogs", db_wl.len());
        for (status, stat) in status_stats {
            println!("\t{} {:?},", stat, status);
        }
        println!(
            "\ttotal time spent: {}",
            Common::readable_time_spent(total_time_spent)
        );
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("list worklogs").arg(
            Arg::new("all")
                .short('a')
                .long("all")
                .help("list all worklogs")
                .action(ArgAction::SetTrue),
        )
    }
}

fn compute_worklogs_stats(worklogs: Vec<LocalWorklog>) -> (HashMap<LocalWorklogState, usize>, i64) {
    let mut status_count = HashMap::new();
    let mut total_time_spent = 0;

    for worklog in worklogs {
        *status_count.entry(worklog.status.clone()).or_insert(0) += 1;
        total_time_spent += worklog.time_spent_seconds;
    }
    (status_count, total_time_spent)
}

#[derive(Debug, Tabled)]
struct WorklogInfo {
    id: String,
    status: ColoredString,
    issue: String,
    time_spent: String,
    start: String,
    comment: String,
}

impl WorklogInfo {
    fn new(worklog: LocalWorklog) -> WorklogInfo {
        WorklogInfo {
            id: worklog.id,
            status: WorklogInfo::status(worklog.status),
            issue: worklog.issue_id,
            time_spent: Common::readable_time_spent(worklog.time_spent_seconds),
            start: Common::format_date_time(&worklog.started),
            comment: worklog.comment,
        }
    }

    fn status(status: LocalWorklogState) -> ColoredString {
        match status {
            Created => String::from("Created").red(),
            Staged => String::from("Staged").green(),
            Pushed => String::from("Pushed").dimmed(),
        }
    }
}

struct LogResetCommand;

#[async_trait]
impl Command for LogResetCommand {
    fn name(&self) -> &'static str {
        "reset"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        let wl = LocalWorklogService::get_all_local_worklogs();
        let wl_to_remove: Vec<LocalWorklog> = wl
            .iter()
            .filter(|w| w.status == Created || w.status == Staged)
            .cloned()
            .collect();
        let wl_removed = wl_to_remove.len();
        for w in wl_to_remove {
            debug!("removing unpushed worklog '{}'", w.id.as_str());
            LocalWorklogService::remove_local_worklog(&w);
        }
        println!("{} worklogs removed", wl_removed);
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("reset unpushed worklogs")
    }
}

struct LogAddCommand;

#[async_trait]
impl Command for LogAddCommand {
    fn name(&self) -> &'static str {
        "add"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let ids: Vec<String> = matches
            .get_many("worklog-ids")
            .expect("required worklog-ids")
            .cloned()
            .collect();
        let worklogs_to_add = if !ids.is_empty() && ids.contains(&"all".to_string()) {
            LocalWorklogService::get_all_local_worklogs_by_status(vec![Created])
        } else {
            LocalWorklogService::get_all_local_worklogs_by_status(vec![Created])
                .iter()
                .filter(|w| ids.iter().any(|i| w.id.starts_with(i)))
                .cloned()
                .collect()
        };
        for mut w in worklogs_to_add {
            w.status = Staged;
            LocalWorklogService::save_local_worklog(w.clone());
            println!("Worklog '{}' added", w.id);
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("add worklog to the staging")
            .arg(
                Arg::new("worklog-ids")
                    .long("worklog-ids")
                    .alias("wid")
                    .short('w')
                    .value_parser(clap::value_parser!(String))
                    .num_args(1..)
                    .required(true)
                    .help("ids of a specific local worklogs to stage"),
            )
    }
}

struct LogRemoveCommand;

#[async_trait]
impl Command for LogRemoveCommand {
    fn name(&self) -> &'static str {
        "rm"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let ids: Vec<String> = matches
            .get_many("worklog-ids")
            .expect("required worklog-ids")
            .cloned()
            .collect();
        let worklogs_to_remove = if !ids.is_empty() && ids.contains(&"all".to_string()) {
            LocalWorklogService::get_all_local_worklogs_by_status(vec![Staged])
        } else {
            LocalWorklogService::get_all_local_worklogs_by_status(vec![Staged])
                .iter()
                .filter(|w| ids.iter().any(|i| w.id.starts_with(i)))
                .cloned()
                .collect()
        };
        for mut w in worklogs_to_remove {
            w.status = Created;
            LocalWorklogService::save_local_worklog(w.clone());
            println!("Worklog '{}' removed", w.id);
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("remove worklog from the staging")
            .arg(
                Arg::new("worklog-ids")
                    .long("worklog-ids")
                    .alias("wid")
                    .short('w')
                    .value_parser(clap::value_parser!(String))
                    .num_args(1..)
                    .required(true)
                    .help("ids of a specific local worklogs to remove"),
            )
    }
}

struct LogPushCommand;

#[async_trait]
impl Command for LogPushCommand {
    fn name(&self) -> &'static str {
        "push"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        let worklogs = LocalWorklogService::get_all_local_worklogs_by_status(vec![Staged]);
        let mut local_worklogs_id: Vec<String> = Vec::new();
        for mut wl in worklogs {
            match IssueService::add_time(
                wl.issue_id.as_str(),
                Duration::seconds(wl.time_spent_seconds),
                wl.started,
                Some(wl.comment.clone()),
            )
            .await
            {
                Ok(result) => {
                    wl.status = Pushed;
                    if let Some(jira_worklog) = result {
                        wl.worklog_id = Some(jira_worklog.id);
                    }
                    LocalWorklogService::save_local_worklog(wl.clone());
                    local_worklogs_id.push(wl.id);
                }
                Err(err) => eprintln!("{:?}", err),
            }
        }
        LocalWorklogService::historize(local_worklogs_id.clone());
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("push worklog to jira")
    }
}

struct LogRevertCommand;

#[async_trait]
impl Command for LogRevertCommand {
    fn name(&self) -> &'static str {
        "revert"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let worklog_histories: Vec<LocalWorklogHistory> = matches
            .get_many::<String>("worklog-ids")
            .expect("where are my ids ??!")
            .filter_map(|whid| LocalWorklogService::get_worklog_history(whid))
            .collect();
        debug!("{:?} worklogs to revert", worklog_histories);
        for worklog_history in worklog_histories {
            LocalWorklogService::revert_worklog_history(&worklog_history).await;
            println!("{} worklog reverted", worklog_history.id.as_str());
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("revert worklogs").arg(
            Arg::new("worklog-ids")
                .help("worklog history id")
                .required(true)
                .num_args(1..),
        )
    }
}

struct LogHistoryCommand;

#[async_trait]
impl Command for LogHistoryCommand {
    fn name(&self) -> &'static str {
        "history"
    }

    async fn execute(&self, matches: &ArgMatches) {
        let history = LocalWorklogService::get_history();
        if history.is_empty() {
            println!("No worklogs in history");
        }
        for local_worklog_history in history {
            let mut worklogs = local_worklog_history
                .local_worklogs_id
                .iter()
                .filter_map(|wid| LocalWorklogService::get_worklog(wid))
                .collect::<Vec<_>>();
            worklogs.sort_by(|a, b| a.started.cmp(&b.started));
            let total_time = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>();
            println!(
                "[{:<8}] {:<16} - {:>3} worklogs - time logged: {:>5}",
                local_worklog_history.id.yellow(),
                Common::format_date_time(&local_worklog_history.date),
                worklogs.len(),
                Common::readable_time_spent(total_time).red()
            );
            if matches.get_flag("detailed") {
                for local_worklog_id in worklogs {
                    println!(
                        "\t- {id}: {date_time} {time_spent:>5} on {issue_id}",
                        id = local_worklog_id.id.magenta(),
                        time_spent =
                            Common::readable_time_spent(local_worklog_id.time_spent_seconds).red(),
                        date_time = Common::format_date_time(&local_worklog_id.started),
                        issue_id = local_worklog_id.issue_id.cyan(),
                    );
                }
            }
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("show worklogs history")
            .arg(
                Arg::new("detailed")
                    .short('d')
                    .long("detailed")
                    .action(ArgAction::SetTrue),
            )
    }
}

pub struct LogCleanupCommand;

#[async_trait]
impl Command for LogCleanupCommand {
    fn name(&self) -> &'static str {
        "cleanup"
    }

    async fn execute(&self, matches: &ArgMatches) {
        use std::collections::HashSet;
        use wtf_lib::services::worklogs_service::WorklogsService;

        let dry_run = matches.get_flag("dry-run");

        // Get all local worklogs and Jira worklogs
        let local_worklogs = LocalWorklogService::get_all_local_worklogs();
        let jira_worklogs = WorklogsService::get_all_worklogs();

        // Build set of Jira worklog IDs
        let jira_worklog_ids: HashSet<String> =
            jira_worklogs.iter().map(|w| w.id.clone()).collect();

        // Find local worklogs that are duplicates (have worklog_id that exists in Jira)
        let duplicates: Vec<&LocalWorklog> = local_worklogs
            .iter()
            .filter(|lw| {
                if let Some(ref worklog_id) = lw.worklog_id {
                    jira_worklog_ids.contains(worklog_id)
                } else {
                    false
                }
            })
            .collect();

        if duplicates.is_empty() {
            println!("✓ No duplicate worklogs found!");
            return;
        }

        println!(
            "Found {} duplicate local worklogs that already exist in Jira:",
            duplicates.len()
        );
        println!();

        let mut total_seconds = 0i64;
        for dup in &duplicates {
            let time_str = Common::readable_time_spent(dup.time_spent_seconds);
            println!(
                "  • {} - {} - {} ({})",
                dup.started.format("%Y-%m-%d %H:%M"),
                dup.issue_id,
                dup.comment,
                time_str
            );
            total_seconds += dup.time_spent_seconds;
        }

        let num_duplicates = duplicates.len();

        println!();
        println!(
            "Total duplicate time: {}",
            Common::readable_time_spent(total_seconds)
        );
        println!();

        if dry_run {
            println!(
                "🔍 DRY RUN - no changes made. Run without --dry-run to delete these duplicates."
            );
        } else {
            println!(
                "⚠️  Deleting {} duplicate local worklogs...",
                num_duplicates
            );
            for dup in duplicates {
                LocalWorklogService::remove_local_worklog(dup);
            }
            println!("✓ Cleanup complete! {} worklogs removed.", num_duplicates);
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Remove duplicate local worklogs that already exist in Jira")
            .arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .help("Show what would be deleted without actually deleting")
                    .action(ArgAction::SetTrue),
            )
    }
}
