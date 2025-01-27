use chrono::{DateTime, Utc};
use log::debug;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitHubEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub created_at: DateTime<Utc>,
    pub repo: GitHubRepo,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitHubRepo {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitHubCommit {
    pub sha: String,
    pub message: String,
}

pub struct GitHubClient;

impl GitHubClient {
    /// Check if GitHub CLI is available
    pub fn is_available() -> bool {
        Command::new("gh")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Get the current authenticated GitHub user
    pub fn get_username() -> Result<String, String> {
        let output = Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .map_err(|e| format!("Failed to execute gh command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("GitHub CLI error: {}", stderr));
        }

        let username = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(username)
    }

    /// Fetch GitHub events for a user within a date range
    pub fn fetch_events(
        username: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<GitHubEvent>, String> {
        debug!(
            "Fetching GitHub events for {} from {} to {}",
            username, from, to
        );

        // GitHub API only returns the last 90 days of events
        // We'll fetch recent events and filter by date
        let mut all_events = Vec::new();
        let mut page = 1;
        let per_page = 100;

        loop {
            let output = Command::new("gh")
                .args([
                    "api",
                    &format!(
                        "/users/{}/events?per_page={}&page={}",
                        username, per_page, page
                    ),
                ])
                .output()
                .map_err(|e| format!("Failed to execute gh command: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // GitHub API pagination limit is expected when fetching old data
                if stderr.contains("pagination is limited") || stderr.contains("HTTP 422") {
                    debug!(
                        "Reached GitHub API pagination limit at page {} (this is normal)",
                        page
                    );
                } else {
                    debug!("GitHub API error at page {}: {}", page, stderr.trim());
                }
                break;
            }

            let json = String::from_utf8_lossy(&output.stdout);
            let events: Vec<GitHubEvent> = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse GitHub events: {}", e))?;

            if events.is_empty() {
                break;
            }

            // Check if we've gone past our date range
            if let Some(last_event) = events.last() {
                if last_event.created_at < from {
                    // Add events within range and stop
                    all_events.extend(
                        events
                            .into_iter()
                            .filter(|e| e.created_at >= from && e.created_at <= to),
                    );
                    break;
                }
            }

            all_events.extend(
                events
                    .into_iter()
                    .filter(|e| e.created_at >= from && e.created_at <= to),
            );

            page += 1;

            // Safety limit
            if page > 10 {
                break;
            }
        }

        debug!("Fetched {} GitHub events", all_events.len());
        Ok(all_events)
    }

    /// Extract Jira issue IDs from various GitHub event sources
    pub fn extract_jira_issues(event: &GitHubEvent) -> Vec<String> {
        let mut issues = Vec::new();
        // Case-insensitive regex to match issue IDs like "PAT-11" or "pat-11"
        let jira_pattern = regex::Regex::new(r"(?i)\b([A-Z][A-Z0-9]+-\d+)\b").unwrap();

        // Helper function to extract issues from text
        let extract_from_text = |text: &str, issues: &mut Vec<String>| {
            for cap in jira_pattern.captures_iter(text) {
                if let Some(issue) = cap.get(1) {
                    // Convert to uppercase for consistency (PAT-11, ETECH-123, etc.)
                    issues.push(issue.as_str().to_uppercase());
                }
            }
        };

        // Extract from repo name (e.g., "org/PROJ-123-feature")
        extract_from_text(&event.repo.name, &mut issues);

        // Extract from event payload based on type
        match event.event_type.as_str() {
            "PushEvent" => {
                // Extract from all commit messages (including multi-line)
                if let Some(commits) = event.payload.get("commits").and_then(|c| c.as_array()) {
                    for commit in commits {
                        if let Some(message) = commit.get("message").and_then(|m| m.as_str()) {
                            extract_from_text(message, &mut issues);
                        }
                    }
                }
                // Extract from branch name (e.g., "refs/heads/feature/PROJ-123")
                if let Some(ref_str) = event.payload.get("ref").and_then(|r| r.as_str()) {
                    extract_from_text(ref_str, &mut issues);
                }
            }
            "PullRequestEvent" | "PullRequestReviewEvent" | "PullRequestReviewCommentEvent" => {
                if let Some(pr) = event.payload.get("pull_request") {
                    // Extract from PR title
                    if let Some(title) = pr.get("title").and_then(|t| t.as_str()) {
                        extract_from_text(title, &mut issues);
                    }
                    // Extract from PR body/description
                    if let Some(body) = pr.get("body").and_then(|b| b.as_str()) {
                        extract_from_text(body, &mut issues);
                    }
                    // Extract from PR head branch name
                    if let Some(head) = pr.get("head") {
                        if let Some(ref_str) = head.get("ref").and_then(|r| r.as_str()) {
                            extract_from_text(ref_str, &mut issues);
                        }
                    }
                    // Extract from PR base branch name
                    if let Some(base) = pr.get("base") {
                        if let Some(ref_str) = base.get("ref").and_then(|r| r.as_str()) {
                            extract_from_text(ref_str, &mut issues);
                        }
                    }
                }
                // For review comments, also check the comment body
                if event.event_type == "PullRequestReviewCommentEvent" {
                    if let Some(comment) = event.payload.get("comment") {
                        if let Some(body) = comment.get("body").and_then(|b| b.as_str()) {
                            extract_from_text(body, &mut issues);
                        }
                    }
                }
            }
            "IssuesEvent" | "IssueCommentEvent" => {
                if let Some(issue) = event.payload.get("issue") {
                    // Extract from issue title
                    if let Some(title) = issue.get("title").and_then(|t| t.as_str()) {
                        extract_from_text(title, &mut issues);
                    }
                    // Extract from issue body
                    if let Some(body) = issue.get("body").and_then(|b| b.as_str()) {
                        extract_from_text(body, &mut issues);
                    }
                }
                // For issue comments, check the comment body
                if event.event_type == "IssueCommentEvent" {
                    if let Some(comment) = event.payload.get("comment") {
                        if let Some(body) = comment.get("body").and_then(|b| b.as_str()) {
                            extract_from_text(body, &mut issues);
                        }
                    }
                }
            }
            "CreateEvent" | "DeleteEvent" => {
                // Extract from ref (branch/tag name)
                if let Some(ref_str) = event.payload.get("ref").and_then(|r| r.as_str()) {
                    extract_from_text(ref_str, &mut issues);
                }
            }
            _ => {}
        }

        // Deduplicate and sort
        issues.sort();
        issues.dedup();
        issues
    }

    /// Extract a human-readable description from a GitHub event
    pub fn extract_description(event: &GitHubEvent) -> String {
        match event.event_type.as_str() {
            "PushEvent" => {
                if let Some(commits) = event.payload.get("commits").and_then(|c| c.as_array()) {
                    if let Some(first_commit) = commits.first() {
                        if let Some(message) = first_commit.get("message").and_then(|m| m.as_str())
                        {
                            // Take first line only
                            return message.lines().next().unwrap_or("Push").to_string();
                        }
                    }
                }
                "Push to repository".to_string()
            }
            "PullRequestEvent" => {
                if let Some(pr) = event.payload.get("pull_request") {
                    if let Some(title) = pr.get("title").and_then(|t| t.as_str()) {
                        return format!("PR: {}", title);
                    }
                }
                "Pull Request".to_string()
            }
            "PullRequestReviewEvent" => "PR Review".to_string(),
            "IssuesEvent" => {
                if let Some(action) = event.payload.get("action").and_then(|a| a.as_str()) {
                    if let Some(issue) = event.payload.get("issue") {
                        if let Some(title) = issue.get("title").and_then(|t| t.as_str()) {
                            return format!("{} issue: {}", action, title);
                        }
                    }
                    return format!("{} issue", action);
                }
                "Issue activity".to_string()
            }
            "IssueCommentEvent" => "Issue comment".to_string(),
            _ => event.event_type.clone(),
        }
    }
}
