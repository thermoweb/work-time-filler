// Meeting operations: link, unlink, auto-link

use std::collections::HashMap;
use std::sync::mpsc::channel;
use std::thread;

use regex::Regex;

use wtf_lib::services::jira_service::{IssueService, JiraService};
use wtf_lib::services::meetings_service::MeetingsService;

use crate::logger;
use crate::tasks::worklog_tasks::MeetingWorklogTask;
use crate::tasks::Task;

use super::super::{
    types::{FetchStatus, IssueSelectionState},
    Tui,
};

impl Tui {
    pub(in crate::tui) fn handle_meeting_log(&mut self) {
        // Don't start a new operation if fetch is already in progress
        if matches!(self.fetch_status, FetchStatus::Fetching(_)) {
            return;
        }

        let (sender, receiver) = channel();
        self.fetch_receiver = Some(receiver);

        // Spawn background thread to run async meeting log
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();

            runtime.block_on(async {
                let _ = sender.send(FetchStatus::Fetching(
                    "Creating worklogs from meetings...".to_string(),
                ));

                let sprints = JiraService::get_followed_sprint();
                match MeetingWorklogTask::new(sprints).execute().await {
                    Ok(_) => {
                        let _ = sender.send(FetchStatus::Complete);
                    }
                    Err(e) => {
                        let _ = sender
                            .send(FetchStatus::Error(format!("Failed to log meetings: {}", e)));
                    }
                }
            });
        });
    }

    pub(in crate::tui) fn unlink_meeting(&mut self, meeting_id: String) {
        if let Some(mut meeting) = MeetingsService::get_meeting_by_id(meeting_id) {
            meeting.jira_link = None;
            MeetingsService::save(&meeting);
            self.refresh_data();
        }
    }

    pub(in crate::tui) fn link_meeting(&mut self, meeting_id: String) {
        // Get the meeting to extract potential Jira IDs
        let meeting = match MeetingsService::get_meeting_by_id(meeting_id.clone()) {
            Some(m) => m,
            None => return,
        };

        // Extract Jira issue IDs from meeting title and description
        let jira_pattern = Regex::new(r"(?i)\b([A-Z][A-Z0-9]+-\d+)\b").unwrap();
        let mut potential_issues: Vec<String> = Vec::new();

        // Search in title
        if let Some(title) = &meeting.title {
            for cap in jira_pattern.captures_iter(title) {
                if let Some(issue) = cap.get(1) {
                    potential_issues.push(issue.as_str().to_uppercase());
                }
            }
        }

        // Search in description
        if let Some(desc) = &meeting.description {
            for cap in jira_pattern.captures_iter(desc) {
                if let Some(issue) = cap.get(1) {
                    potential_issues.push(issue.as_str().to_uppercase());
                }
            }
        }

        // Deduplicate
        potential_issues.sort();
        potential_issues.dedup();

        // If exactly one issue found, auto-link it
        if potential_issues.len() == 1 {
            let issue_key = &potential_issues[0];
            // Verify the issue exists in our database
            if self.data.issues_by_key.contains_key(issue_key) {
                // Auto-link
                let mut meeting = meeting;
                meeting.jira_link = Some(issue_key.clone());
                MeetingsService::save(&meeting);
                logger::log(format!("✅ Auto-linked meeting to {}", issue_key));
                self.refresh_data();
                return;
            }
        }

        // Get all issues
        let all_issues = IssueService::get_all_issues();

        if all_issues.is_empty() {
            return; // No issues to link to
        }

        // Count usage of each issue across all meetings
        let mut issue_usage: HashMap<String, usize> = HashMap::new();
        for m in &self.data.all_meetings {
            if let Some(ref link) = m.jira_link {
                *issue_usage.entry(link.clone()).or_insert(0) += 1;
            }
        }

        // Sort issues: extracted issues first, then by usage count
        let mut sorted_issues = all_issues;
        sorted_issues.sort_by(|a, b| {
            let a_extracted = potential_issues.contains(&a.key);
            let b_extracted = potential_issues.contains(&b.key);

            // Extracted issues first
            if a_extracted && !b_extracted {
                return std::cmp::Ordering::Less;
            } else if !a_extracted && b_extracted {
                return std::cmp::Ordering::Greater;
            }

            // Then by usage count
            let a_count = issue_usage.get(&a.key).copied().unwrap_or(0);
            let b_count = issue_usage.get(&b.key).copied().unwrap_or(0);
            b_count.cmp(&a_count).then_with(|| a.key.cmp(&b.key))
        });

        // Show message if issues were found but not auto-linked
        if !potential_issues.is_empty() {
            logger::log(format!(
                "💡 Found {} potential issue(s): {}",
                potential_issues.len(),
                potential_issues.join(", ")
            ));
        }

        // Open issue selection dialog
        self.issue_selection_state = Some(IssueSelectionState {
            meeting_id,
            all_issues: sorted_issues,
            selected_issue_index: 0,
            search_query: String::new(),
        });
    }

    pub(in crate::tui) fn auto_link_meetings(&mut self) {
        logger::log("🔗 Auto-linking meetings...".to_string());

        let jira_regex = Regex::new(r"([A-Z]+-\d+)").unwrap();
        let mut linked_count = 0;

        // Get all unlinked meetings
        let unlinked_meetings: Vec<_> = self
            .data
            .all_meetings
            .iter()
            .filter(|m| m.jira_link.is_none())
            .collect();

        for meeting in unlinked_meetings {
            // Try to extract Jira issue key from title or description
            let empty_string = String::new();
            let title = meeting.title.as_ref().unwrap_or(&empty_string);
            let description = meeting.description.as_ref().unwrap_or(&empty_string);
            let combined = format!("{} {}", title, description);

            if let Some(captures) = jira_regex.captures(&combined) {
                if let Some(issue_key) = captures.get(1) {
                    let key = issue_key.as_str().to_string();

                    // Check if issue exists in our data
                    if self.data.issues_by_key.contains_key(&key) {
                        // Link the meeting by updating jira_link field
                        if let Some(mut meeting) =
                            MeetingsService::get_meeting_by_id(meeting.id.clone())
                        {
                            meeting.jira_link = Some(key.clone());
                            MeetingsService::save(&meeting);
                            linked_count += 1;
                        }
                    }
                }
            }
        }

        logger::log(format!("✅ Auto-linked {} meeting(s)", linked_count));

        // Refresh data to show the updates
        self.refresh_data();
    }
}
