use crate::tasks::Task;
use anyhow::Result;
use chrono::{DateTime, Datelike, Months, NaiveDate, Utc, Weekday};
use colored::Colorize;
use futures::future::join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{CustomUserError, Text};
use log::{debug, info};
use regex::Regex;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tabled::settings::object::Columns;
use tabled::settings::{Alignment, Color, Modify, Style};
use tabled::{Table, Tabled};
use tokio::sync::Semaphore;
use wtf_lib::client::jira_client::JiraClient;
use wtf_lib::config::Config;
use wtf_lib::models::data::SprintState::{Active, Closed, Future};
use wtf_lib::models::data::{Absence, Board, BoardType, Issue, Sprint, Worklog};
use wtf_lib::models::jira::{format_comment, JiraSprint};
use wtf_lib::services::jira_service::{BoardService, IssueService, JiraService, SprintService};
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::WorklogsService;

pub struct FetchJiraIssues {
    pub sprints: Vec<Sprint>,
    pub multi_progress: Option<MultiProgress>,
}

impl FetchJiraIssues {
    pub fn new(sprints: Vec<Sprint>) -> Self {
        Self {
            sprints: sprints.clone(),
            multi_progress: None,
        }
    }

    pub fn with_progress(mut self, progress: MultiProgress) -> Self {
        self.multi_progress = Some(progress);
        self
    }
}

impl Task for FetchJiraIssues {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let jira_client = JiraClient::create();
        let mp = match &self.multi_progress {
            None => MultiProgress::new(),
            Some(multi) => multi.clone(),
        };
        let sprint_progress = mp.add(ProgressBar::new(self.sprints.len() as u64));
        let default_style = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-");
        sprint_progress.set_style(default_style.clone());
        sprint_progress.enable_steady_tick(Duration::from_millis(100));
        let mut issues_to_store = Vec::new();
        for sprint in &self.sprints {
            sprint_progress.inc(1);
            sprint_progress.set_message(format!("fetching issues for sprint #{}", sprint.id));
            match jira_client.get_all_issues_v2(&sprint.id.to_string()).await {
                Ok(issue_fetcher) => {
                    if issue_fetcher.len() == 0 {
                        mp.println(format!("No issues found for sprint '{}'.", sprint.id))
                            .unwrap();
                    } else {
                        let issue_progress = mp.add(ProgressBar::new(issue_fetcher.len() as u64));
                        issue_progress.set_style(default_style.clone());
                        issue_progress.enable_steady_tick(Duration::from_millis(100));
                        for issue in issue_fetcher {
                            issue_progress.set_message(format!("issue {}", issue.key));
                            issues_to_store.push(Issue {
                                key: issue.key.clone(),
                                id: issue.id,
                                created: issue.fields.created,
                                status: issue.fields.status.name,
                                summary: issue.fields.summary,
                            });
                            issue_progress.inc(1);
                        }
                        issue_progress.finish_and_clear();
                    }
                }
                Err(e) => eprintln!("Error: {:?}", e),
            }
        }
        sprint_progress.finish_and_clear();

        for board in JiraService::get_followed_boards()
            .unwrap()
            .iter()
            .filter(|b| b.board_type == BoardType::Kanban || b.board_type == BoardType::Scrum)
            .cloned()
            .collect::<Vec<Board>>()
        {
            if let Some(project_name) = board.project_name {
                let start_date = Utc::now().checked_sub_months(Months::new(12));
                let fetch_result = match board.board_type {
                    // For Scrum boards, only fetch backlog issues (not in any sprint);
                    // sprint issues are already fetched above per sprint.
                    BoardType::Scrum => {
                        jira_client
                            .get_project_backlog_issues(project_name.as_str(), start_date)
                            .await
                    }
                    _ => {
                        jira_client
                            .get_project_issues(project_name.as_str(), start_date)
                            .await
                    }
                };
                match fetch_result {
                    Ok(issue_fetcher) => {
                        if issue_fetcher.len() == 0 {
                            mp.println(format!("No backlog issues found for project '{}'.", project_name))
                                .unwrap();
                        } else {
                            let issue_progress =
                                mp.add(ProgressBar::new(issue_fetcher.len() as u64));
                            issue_progress.set_style(default_style.clone());
                            issue_progress.enable_steady_tick(Duration::from_millis(100));
                            for issue in issue_fetcher {
                                issue_progress.set_message(format!("issue {}", issue.key));
                                issues_to_store.push(Issue {
                                    key: issue.key.clone(),
                                    id: issue.id,
                                    created: issue.fields.created,
                                    status: issue.fields.status.name,
                                    summary: issue.fields.summary,
                                });
                                issue_progress.inc(1);
                            }
                            issue_progress.finish_and_clear();
                        }
                    }
                    Err(e) => eprintln!("Error: {:?}", e),
                }
            }
        }

        mp.println(format!("{} issues fetched", issues_to_store.len()))
            .unwrap();
        IssueService::save_all_issues(issues_to_store);
        Ok(())
    }
}

pub struct FetchJiraBoard {
    multi_progress: Option<MultiProgress>,
    skip_follow_prompt: bool,
}

impl FetchJiraBoard {
    pub fn new() -> Self {
        Self {
            multi_progress: None,
            skip_follow_prompt: false,
        }
    }

    pub fn without_follow_prompt(mut self) -> Self {
        self.skip_follow_prompt = true;
        self
    }

    pub fn with_progress(mut self, multi: MultiProgress) -> Self {
        self.multi_progress = Some(multi);
        self
    }
}

impl Task for FetchJiraBoard {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let mp = match &self.multi_progress {
            None => MultiProgress::new(),
            Some(multi) => multi.clone(),
        };
        let mut boards_added = 0;
        let jira_client = JiraClient::create();
        match jira_client.get_all_boards().await {
            Ok(board_fetcher) => {
                if board_fetcher.len() == 0 {
                    info!("No board found.");
                } else {
                    let progress_bar = mp.add(ProgressBar::new(board_fetcher.len() as u64));
                    progress_bar.set_style(ProgressStyle::default_bar()
                        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} boards ({percent}%)")
                        .unwrap());
                    for jira_board in board_fetcher {
                        let mut board_to_store = Board::from_jira(jira_board.clone());
                        progress_bar.inc(1);
                        if let Some(db_board) =
                            BoardService::get_by_id(jira_board.id.to_string().as_str())
                        {
                            board_to_store.followed = db_board.followed;
                        }
                        BoardService::save_board(&board_to_store);
                        boards_added += 1;
                    }
                    progress_bar.finish_and_clear();
                }
            }
            Err(e) => eprintln!("Error: {:?}", e),
        }
        mp.println(format!("{} boards fetched", boards_added))
            .unwrap();
        let boards = BoardService::get_all_boards();
        if !self.skip_follow_prompt && boards
            .iter()
            .filter(|board| board.followed)
            .collect::<Vec<_>>()
            .is_empty()
        {
            let board = Text::new("Board to follow: ")
                .with_autocomplete(&board_suggestor)
                .with_page_size(5)
                .prompt()
                .unwrap();
            let regex = Regex::new(r"\[([0-9]+)]").unwrap();
            if let Some(caps) = regex.captures(&board) {
                if let Some(board_id) = caps.get(1) {
                    println!("board '{}' is now followed", board_id.as_str());
                    JiraService::follow_board(board_id.as_str()).unwrap();
                }
            }
        }
        Ok(())
    }
}

fn board_suggestor(input: &str) -> Result<Vec<String>, CustomUserError> {
    let input = input.to_lowercase();
    Ok(BoardService::get_all_boards()
        .iter()
        .filter(|board| {
            board.name.to_lowercase().contains(&input) || board.id.to_string().contains(&input)
        })
        .map(|board| format!("[{}] {}", board.id, board.name))
        .take(5)
        .collect())
}

#[derive(Debug)]
pub struct FetchJiraSprint {
    multi_bar: Option<MultiProgress>,
}

impl FetchJiraSprint {
    pub fn new() -> Self {
        Self { multi_bar: None }
    }

    pub fn with_progress(mut self, multi: MultiProgress) -> Self {
        self.multi_bar = Some(multi);
        self
    }
}

impl Task for FetchJiraSprint {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let boards = JiraService::get_followed_boards().unwrap();
        if boards.is_empty() {
            println!("No boards found.");
            return Ok(());
        }
        let mb = match &self.multi_bar {
            Some(pb) => pb,
            None => &MultiProgress::new(),
        };

        // Get the auto-follow pattern from config
        let auto_follow_pattern = Config::load()
            .ok()
            .and_then(|c| c.jira.auto_follow_sprint_pattern);

        let mut sprint_counts = 0;
        let mut auto_followed_count = 0;

        for board in boards {
            match board.board_type {
                BoardType::Scrum => {
                    let sprints: Vec<Sprint> = fetch_board_sprints(mb.clone(), board.id)
                        .await?
                        .iter()
                        .map(|s| into_sprint(s))
                        .map(|mut spr| {
                            if let Some(db_sprint) =
                                SprintService::get_sprint_by_id(spr.id.to_string().as_str())
                            {
                                spr.followed = db_sprint.followed;
                            }
                            // Auto-follow based on pattern (only Active and Future sprints)
                            if !spr.followed {
                                if let Some(ref pattern) = auto_follow_pattern {
                                    if spr.name.contains(pattern)
                                        && (spr.state == Active || spr.state == Future)
                                    {
                                        spr.followed = true;
                                        auto_followed_count += 1;
                                    }
                                }
                            }
                            spr
                        })
                        .collect();
                    let sprints_to_add = sprints.len();
                    SprintService::save_all_sprints(sprints);
                    sprint_counts += sprints_to_add;
                }
                _ => debug!("no sprints attached to this board"),
            }
        }
        mb.println(format!("{} sprints fetched", sprint_counts))
            .unwrap();
        if auto_followed_count > 0 {
            mb.println(format!(
                "Auto-followed {} sprints matching pattern",
                auto_followed_count
            ))
            .unwrap();
        }
        Ok(())
    }
}

fn into_sprint(sprint: &JiraSprint) -> Sprint {
    let bind = MeetingsService::get_absences();
    let absences = bind
        .iter()
        .filter(|a| {
            // Check if absence overlaps with sprint period
            let sprint_start = sprint.start_date.map(|d| d.date_naive());
            let sprint_end = sprint.end_date.map(|d| d.date_naive());
            let absence_start = a.start.date_naive();
            let absence_end = a.end.date_naive();

            // Overlap if: absence_start <= sprint_end AND absence_end >= sprint_start
            match (sprint_start, sprint_end) {
                (Some(s_start), Some(s_end)) => absence_start <= s_end && absence_end >= s_start,
                _ => false,
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    let workdays = sprint
        .start_date
        .zip(sprint.end_date)
        .map_or(0, |(start, end)| {
            count_workdays(start.date_naive(), end.date_naive(), absences)
        });
    Sprint {
        id: sprint.id,
        state: match sprint.state.as_str() {
            "active" => Active,
            "closed" => Closed,
            "future" => Future,
            _ => panic!(),
        },
        name: sprint.name.clone(),
        start: sprint.start_date,
        end: sprint.end_date,
        followed: false,
        workdays,
    }
}

fn count_workdays(start: NaiveDate, end: NaiveDate, absences: Vec<Absence>) -> i64 {
    let mut workdays = 0;
    let mut current_date = start;
    let abs = absences
        .iter()
        .map(|a| (a.start.date_naive(), a.end.date_naive()))
        .collect::<Vec<(NaiveDate, NaiveDate)>>();
    while current_date <= end {
        let is_dayoff = abs
            .iter()
            .find(|(s, e)| &current_date <= e && &current_date >= s);
        if current_date.weekday() != Weekday::Sat
            && current_date.weekday() != Weekday::Sun
            && is_dayoff.is_none()
        {
            workdays += 1;
        }
        current_date = current_date.succ_opt().unwrap();
    }
    workdays
}

async fn fetch_board_sprints(mp: MultiProgress, board_id: usize) -> Result<Vec<JiraSprint>> {
    let jira_client = JiraClient::create();
    let mut sprints_to_store = Vec::new();
    match jira_client.get_all_sprint(board_id).await {
        Ok(sprints_fetcher) => {
            let sprint_progress_style = ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )?
                .progress_chars("#>-");
            let sprint_bar = mp.add(ProgressBar::new(sprints_fetcher.len() as u64));
            sprint_bar.set_style(sprint_progress_style);
            for sprint in sprints_fetcher {
                sprint_bar.tick();
                sprint_bar.set_message(format!("sprint #{}", sprint.id));
                sprints_to_store.push(sprint);
                sprint_bar.inc(1);
            }
            sprint_bar.finish_and_clear();
        }
        Err(e) => eprintln!("Error: {:?}", e),
    }
    Ok(sprints_to_store)
}

pub struct ListJiraSprints {
    fetch_all: bool,
}

impl ListJiraSprints {
    pub fn new(fetch_all: bool) -> Self {
        Self { fetch_all }
    }
}

impl Task for ListJiraSprints {
    async fn execute(&self) -> std::result::Result<(), Box<dyn Error>> {
        let mut sprints = if self.fetch_all {
            println!("Listing all available sprints:");
            JiraService::get_available_sprints()
        } else {
            println!("Listing followed sprints:");
            JiraService::get_followed_sprint()
        };
        if sprints.is_empty() {
            println!("No sprint found.");
            return Ok(());
        }
        sprints.sort_by(|a, b| a.start.cmp(&b.start));
        let sprints_data = sprints
            .iter()
            .map(|s| {
                let time_spent = WorklogsService::get_all_worklogs()
                    .iter()
                    .filter(|wl| {
                        let worklog_date = wl.started;
                        let is_after_start = s.start.map_or(false, |start| worklog_date >= start);
                        let is_before_end = s.end.map_or(false, |end| worklog_date <= end);
                        is_after_start && is_before_end
                    })
                    .map(|wl| wl.time_spent_seconds)
                    .sum::<u64>();

                SprintInfo::from_data(s, time_spent)
            })
            .collect::<Vec<_>>();

        let mut table = Table::new(sprints_data);
        table.with(Style::modern().remove_horizontal());
        table.with(Modify::new(Columns::new(..)).with(Alignment::center()));
        table.with(
            Modify::new(Columns::first())
                .with(Color::BOLD | Color::FG_WHITE)
                .with(Alignment::center()),
        );
        println!("{table}");
        Ok(())
    }
}

#[derive(Debug, Tabled)]
enum SprintStatus {
    Active,
    Closed,
    Future,
}

impl fmt::Display for SprintStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            SprintStatus::Active => "Active".green(),
            SprintStatus::Closed => "Closed".dimmed(),
            SprintStatus::Future => "Future".yellow(),
        };
        write!(f, "{}", status_str)
    }
}

#[derive(Debug, Tabled)]
struct SprintInfo {
    id: usize,
    name: String,
    followed: bool,
    start: String,
    end: String,
    status: SprintStatus,
    hours_spent: String,
    workdays: i64,
}

impl SprintInfo {
    fn from_data(sprint_info: &Sprint, time_spent_seconds: u64) -> Self {
        let format_date = |date: &DateTime<Utc>| date.format("%d-%m-%Y").to_string();
        let time_spent = Self::get_time_spent(sprint_info, time_spent_seconds);
        Self {
            id: sprint_info.id.clone(),
            name: sprint_info.name.clone(),
            followed: sprint_info.followed,
            status: match sprint_info.state {
                Active => SprintStatus::Active,
                Closed => SprintStatus::Closed,
                Future => SprintStatus::Future,
            },
            start: sprint_info
                .start
                .map(|s| format_date(&s))
                .unwrap_or("".to_string()),
            end: sprint_info
                .end
                .map(|s| format_date(&s))
                .unwrap_or("".to_string()),
            hours_spent: time_spent,
            workdays: sprint_info.workdays,
        }
    }

    fn get_time_spent(sprint_info: &Sprint, time_spent_seconds: u64) -> String {
        let mut time_spent = format!("{:.1}", time_spent_seconds as f64 / 3_600.0);
        let time_expected = sprint_info.workdays as u64 * 7 * 3_600;
        let time_minimum = time_expected * 9 / 10;
        if time_spent_seconds < time_minimum {
            time_spent = time_spent.red().to_string();
        } else if time_spent_seconds < time_expected {
            time_spent = time_spent.yellow().to_string();
        }
        time_spent
    }
}

pub struct FetchJiraWorklogs {
    pub sprints: Vec<Sprint>,
    pub multi_progress: Option<MultiProgress>,
}

impl FetchJiraWorklogs {
    pub fn new(sprints: Vec<Sprint>) -> Self {
        Self {
            sprints: sprints.clone(),
            multi_progress: None,
        }
    }

    pub fn with_progress(mut self, progress: MultiProgress) -> Self {
        self.multi_progress = Some(progress);
        self
    }
}

impl Task for FetchJiraWorklogs {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let mp = self
            .multi_progress
            .clone()
            .unwrap_or_else(MultiProgress::new);
        let sprint_progress = mp.add(ProgressBar::new(self.sprints.len() as u64));
        let progress_style = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-");
        sprint_progress.set_style(progress_style);
        sprint_progress.enable_steady_tick(Duration::from_millis(100));

        let semaphore = Arc::new(Semaphore::new(5));
        let mut tasks = vec![];

        for sprint in self.sprints.clone() {
            let progress = sprint_progress.clone();
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let task = tokio::spawn(async move {
                let _permit = permit;

                if sprint.start.is_none() || sprint.end.is_none() {
                    debug!("Sprint '{}' has no start/end date", sprint.id);
                    return Ok::<Vec<Worklog>, String>(vec![]);
                }

                progress.set_message(format!("Fetching sprint #{}", sprint.id));
                tokio::time::sleep(Duration::from_secs(2)).await;

                debug!(
                    "getting worklogs between {} and {}",
                    sprint.start.unwrap(),
                    sprint.end.unwrap()
                );
                let result = tokio::time::timeout(
                    Duration::from_secs(15),
                    JiraClient::create()
                        .get_worklogs_between(sprint.start.unwrap(), sprint.end.unwrap()),
                )
                .await;

                debug!("parsing fetched worklogs");
                let worklogs = match result {
                    Ok(Ok(raw_worklogs)) => raw_worklogs
                        .iter()
                        .map(|wl| Worklog {
                            id: wl.id.clone(),
                            author: wl.author.email_address.clone(),
                            created: wl.created,
                            time_spent: wl.time_spent.clone(),
                            time_spent_seconds: wl.time_spent_seconds.clone(),
                            comment: wl.comment.clone().map(|c| format_comment(&c)),
                            issue_id: wl.issue_id.clone(), // Use the issue_id from JiraWorklog (set by get_issue_worklogs)
                            started: wl.started,
                        })
                        .collect(),
                    Ok(Err(e)) => {
                        debug!("Error from Jira for sprint {}: {:?}", sprint.id, e);
                        vec![]
                    }
                    Err(_) => {
                        debug!("Timeout while fetching sprint {}", sprint.id);
                        vec![]
                    }
                };

                progress.inc(1);
                Ok(worklogs)
            });

            tasks.push(task);
        }

        debug!("adding worklogs to database");
        let results = join_all(tasks).await;

        // Determine the overall date range from sprints (not worklogs)
        let mut min_date: Option<NaiveDate> = None;
        let mut max_date: Option<NaiveDate> = None;

        for sprint in &self.sprints {
            if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                let start_date = start.date_naive();
                let end_date = end.date_naive();
                min_date = Some(min_date.map_or(start_date, |d| d.min(start_date)));
                max_date = Some(max_date.map_or(end_date, |d| d.max(end_date)));
            }
        }

        // Collect all worklogs
        let mut all_fetched_worklogs = Vec::new();

        for result in results {
            match result {
                Ok(Ok(worklogs)) => {
                    all_fetched_worklogs.extend(worklogs);
                }
                Ok(Err(e)) => {
                    debug!("Task error (string): {}", e);
                }
                Err(join_err) => {
                    debug!("Join error: {}", join_err);
                }
            }
        }

        let total_worklogs = all_fetched_worklogs.len();

        // Replace worklogs for the sprints' date range (even if we fetched 0 worklogs)
        if let (Some(min), Some(max)) = (min_date, max_date) {
            debug!(
                "Replacing worklogs for sprint date range {} to {} with {} fresh worklogs",
                min, max, total_worklogs
            );
            WorklogsService::replace_worklogs_for_date_range(min, max, all_fetched_worklogs);
        } else {
            debug!("No sprints with dates found, not updating database");
        }

        sprint_progress.finish_and_clear();
        mp.println(format!("{} worklogs fetched.", total_worklogs))
            .ok();

        Ok(())
    }
}
