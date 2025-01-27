use crate::logger;
use crate::tasks::Task;
use chrono::NaiveDate;
use std::error::Error;
use wtf_lib::services::github_service::GitHubService;
use wtf_lib::services::jira_service::{IssueService, JiraService};
use wtf_lib::services::worklogs_service::LocalWorklogService;

pub struct FetchGithubEventsTask;

impl FetchGithubEventsTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Task for FetchGithubEventsTask {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        logger::log("Syncing GitHub events for followed sprints...".to_string());

        // Check if GitHub CLI is configured
        if !GitHubService::is_configured() {
            logger::log("❌ GitHub CLI is not installed or configured".to_string());
            logger::log("Please install gh CLI: https://cli.github.com/".to_string());
            logger::log("Then run: gh auth login".to_string());
            return Err("GitHub CLI not configured".into());
        }

        // Get followed sprints
        let sprints = JiraService::get_followed_sprint();
        if sprints.is_empty() {
            logger::log(
                "No followed sprints found. Use 'wtf sprint follow' to follow sprints.".to_string(),
            );
            return Ok(());
        }

        logger::log(format!(
            "Fetching events for {} sprint(s)...",
            sprints.len()
        ));

        // Sync events and sessions to database
        let (events_saved, sessions_saved) = GitHubService::sync_events_for_sprints(&sprints)?;

        if events_saved == 0 {
            logger::log("No GitHub events found in sprint date ranges.".to_string());
            return Ok(());
        }

        logger::log(format!(
            "✅ Saved {} events and {} sessions to database",
            events_saved, sessions_saved
        ));

        // Get sessions from database to display summary
        let all_sessions = GitHubService::get_all_sessions()?;
        let mut sessions_by_day: std::collections::HashMap<String, Vec<_>> =
            std::collections::HashMap::new();

        for session in all_sessions {
            sessions_by_day
                .entry(session.date.to_string())
                .or_insert_with(Vec::new)
                .push(session);
        }

        logger::log(format!("Calculated {} work days", sessions_by_day.len()));

        // Display summary
        logger::log("\n📊 GitHub Activity Summary:\n".to_string());

        let mut days: Vec<_> = sessions_by_day.keys().collect();
        days.sort();

        for day in days {
            if let Some(sessions) = sessions_by_day.get(day) {
                let total_hours: f64 = sessions.iter().map(|s| s.duration_hours()).sum();

                logger::log(format!(
                    "  {} - {:.1}h ({} sessions)",
                    day,
                    total_hours,
                    sessions.len()
                ));

                for session in sessions {
                    let issues_str = if !session.jira_issues.is_empty() {
                        format!(" [{}]", session.jira_issues)
                    } else {
                        String::new()
                    };

                    logger::log(format!(
                        "    • {:.1}h - {} on {}{}",
                        session.duration_hours(),
                        session.description,
                        session.repo,
                        issues_str
                    ));
                }
            }
        }

        logger::log("\n✅ Use 'wtf github log' to create worklogs from these events".to_string());

        Ok(())
    }
}

pub struct LogGithubEventsTask;

impl LogGithubEventsTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Task for LogGithubEventsTask {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        logger::log("Creating worklogs from GitHub activity...".to_string());

        // Check if GitHub CLI is configured
        if !GitHubService::is_configured() {
            logger::log("❌ GitHub CLI is not installed or configured".to_string());
            return Err("GitHub CLI not configured".into());
        }

        // Get followed sprints
        let sprints = JiraService::get_followed_sprint();
        if sprints.is_empty() {
            logger::log("No followed sprints found.".to_string());
            return Ok(());
        }

        // Fetch events
        let events = GitHubService::fetch_events_for_sprints(&sprints)?;

        if events.is_empty() {
            logger::log("No GitHub events found.".to_string());
            return Ok(());
        }

        // Calculate work sessions
        let sessions_by_day = GitHubService::calculate_work_sessions(&events);

        let mut worklogs_created = 0;
        let mut worklogs_skipped = 0;

        for (_day, sessions) in sessions_by_day.iter() {
            for session in sessions {
                // Skip if no Jira issue found
                if session.jira_issues.is_empty() {
                    logger::log(format!(
                        "⚠️  Skipping session (no Jira issue): {} on {}",
                        session.description, session.repo
                    ));
                    worklogs_skipped += 1;
                    continue;
                }

                let issue_id = session.jira_issues[0].clone();

                // Check if issue exists in database
                if IssueService::get_by_key(&issue_id).is_none() {
                    logger::log(format!(
                        "⚠️  Skipping {} - issue not found in local database (run 'wtf fetch' first)", 
                        issue_id
                    ));
                    worklogs_skipped += 1;
                    continue;
                }

                // Create worklog using the service method
                let comment = session.description.clone();
                LocalWorklogService::create_new_local_worklogs(
                    session.start_time,
                    session.duration_seconds(),
                    &issue_id,
                    Some(&comment),
                    None, // No meeting_id for GitHub events
                );

                let hours = session.duration_seconds() as f64 / 3600.0;
                logger::log(format!(
                    "✅ Created worklog: {} - {:.1}h - {}",
                    issue_id, hours, session.description
                ));

                worklogs_created += 1;
            }
        }

        logger::log(format!(
            "\n📝 Summary: {} worklogs created, {} skipped",
            worklogs_created, worklogs_skipped
        ));
        logger::log("Use 'wtf dashboard' to review and stage worklogs".to_string());

        Ok(())
    }
}

pub struct ShowGithubSessionsTask {
    date_filter: Option<String>,
}

impl ShowGithubSessionsTask {
    pub fn new(date: Option<&str>) -> Self {
        Self {
            date_filter: date.map(|s| s.to_string()),
        }
    }
}

impl Task for ShowGithubSessionsTask {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let sessions = if let Some(date_str) = &self.date_filter {
            let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date format: {}. Expected YYYY-MM-DD", e))?;
            GitHubService::get_sessions_by_date(date)?
        } else {
            GitHubService::get_all_sessions()?
        };

        if sessions.is_empty() {
            logger::log("No GitHub sessions found in database.".to_string());
            logger::log("Use 'wtf github fetch' to fetch events first.".to_string());
            return Ok(());
        }

        logger::log(format!(
            "📊 Found {} GitHub work session(s)",
            sessions.len()
        ));
        if let Some(date_str) = &self.date_filter {
            logger::log(format!("   Filtered by date: {}", date_str));
        }
        logger::log("".to_string());

        // Group by date
        let mut sessions_by_date: std::collections::HashMap<NaiveDate, Vec<_>> =
            std::collections::HashMap::new();

        for session in sessions {
            sessions_by_date
                .entry(session.date)
                .or_insert_with(Vec::new)
                .push(session);
        }

        // Sort dates
        let mut dates: Vec<_> = sessions_by_date.keys().cloned().collect();
        dates.sort();

        for date in dates {
            let sessions = &sessions_by_date[&date];
            let total_hours: f64 = sessions.iter().map(|s| s.duration_hours()).sum();

            logger::log(format!("📅 {} ({:.1}h total)", date, total_hours));

            for session in sessions {
                let jira_issues = session.get_jira_issues();
                let issues_str = if jira_issues.is_empty() {
                    "No Jira issues".to_string()
                } else {
                    jira_issues.join(", ")
                };

                logger::log(format!(
                    "   ⏱  {:.1}h - {} - {}",
                    session.duration_hours(),
                    session.repo,
                    issues_str
                ));

                // Show description if available
                if !session.description.is_empty() {
                    let short_desc = if session.description.len() > 60 {
                        format!("{}...", &session.description[..60])
                    } else {
                        session.description.clone()
                    };
                    logger::log(format!("      {}", short_desc));
                }
            }
            logger::log("".to_string());
        }

        Ok(())
    }
}

pub struct ShowGithubEventsTask {
    date_filter: Option<String>,
}

impl ShowGithubEventsTask {
    pub fn new(date: Option<&str>) -> Self {
        Self {
            date_filter: date.map(|s| s.to_string()),
        }
    }
}

impl Task for ShowGithubEventsTask {
    async fn execute(&self) -> Result<(), Box<dyn Error>> {
        let events = if let Some(date_str) = &self.date_filter {
            let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date format: {}. Expected YYYY-MM-DD", e))?;
            GitHubService::get_events_by_date(date)?
        } else {
            GitHubService::get_all_events()?
        };

        if events.is_empty() {
            logger::log("No GitHub events found in database.".to_string());
            logger::log("Use 'wtf github fetch' to fetch events first.".to_string());
            return Ok(());
        }

        logger::log(format!("📊 Found {} GitHub event(s)", events.len()));
        if let Some(date_str) = &self.date_filter {
            logger::log(format!("   Filtered by date: {}", date_str));
        }
        logger::log("".to_string());

        // Group by date
        let mut events_by_date: std::collections::HashMap<NaiveDate, Vec<_>> =
            std::collections::HashMap::new();

        for event in events {
            events_by_date
                .entry(event.date)
                .or_insert_with(Vec::new)
                .push(event);
        }

        // Sort dates
        let mut dates: Vec<_> = events_by_date.keys().cloned().collect();
        dates.sort();

        for date in dates {
            let events = &events_by_date[&date];

            logger::log(format!("📅 {} ({} events)", date, events.len()));

            for event in events {
                let jira_issues = event.get_jira_issues();
                let issues_str = if jira_issues.is_empty() {
                    "No issues".to_string()
                } else {
                    jira_issues.join(", ")
                };

                logger::log(format!(
                    "   {} - {} - {} - {}",
                    event.timestamp.format("%H:%M"),
                    event.event_type,
                    event.repo,
                    issues_str
                ));

                // Show description if available
                if !event.description.is_empty() {
                    let short_desc = if event.description.len() > 80 {
                        format!("{}...", &event.description[..80])
                    } else {
                        event.description.clone()
                    };
                    logger::log(format!("      {}", short_desc));
                }
            }
            logger::log("".to_string());
        }

        Ok(())
    }
}
