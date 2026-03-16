use chrono::Timelike;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::GitHubIssueValidation;
use crate::tui::data::TuiData;
use crate::tui::helpers;
use crate::tui::tab_controller::TabController;
use crate::tui::theme::theme;
use crate::tui::ui_helpers::*;
use crate::tui::Tui;

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::tui) struct GitHubTab;

impl TabController for GitHubTab {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData) {
        render_github_tab(frame, area, data);
    }

    fn handle_key(&self, tui: &mut Tui, key: KeyEvent) {
        let sessions = &tui.data.github_sessions;

        match key.code {
            KeyCode::Char('u') | KeyCode::Char('U') => {
                tui.handle_github_sync();
                return;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if !sessions.is_empty() {
                    tui.handle_create_worklog_from_session();
                }
                return;
            }
            _ => {}
        }

        if sessions.is_empty() {
            return;
        }

        let max_index = sessions.len().saturating_sub(1);
        if tui.data.ui_state.selected_github_session_index > max_index {
            tui.data.ui_state.selected_github_session_index = max_index;
        }

        if helpers::handle_list_navigation(
            key,
            &mut tui.data.ui_state.selected_github_session_index,
            max_index,
        ) {
            return;
        }

        match key.code {
            KeyCode::PageUp => {
                tui.data.ui_state.selected_github_session_index = tui
                    .data
                    .ui_state
                    .selected_github_session_index
                    .saturating_sub(10);
            }
            KeyCode::PageDown => {
                tui.data.ui_state.selected_github_session_index =
                    (tui.data.ui_state.selected_github_session_index + 10).min(max_index);
            }
            _ => {}
        }
    }
}

/// GitHub tab - list and details extracted from ui.rs
pub(in crate::tui) fn render_github_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let selected_index = data.ui_state.selected_github_session_index;

    render_list_detail_layout(
        frame,
        area,
        |f, a| render_github_sessions_list(f, a, data, selected_index),
        |f, a| render_github_session_details(f, a, data, selected_index),
    );
}

fn render_github_sessions_list(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
) {
    use chrono::Timelike;
    use std::collections::BTreeMap;

    let sessions = &data.github_sessions;

    // Group sessions by date
    let mut sessions_by_date: BTreeMap<String, Vec<&wtf_lib::models::data::GitHubSession>> =
        BTreeMap::new();
    for session in sessions {
        let date_str = session.date.to_string();
        sessions_by_date
            .entry(date_str)
            .or_insert_with(Vec::new)
            .push(session);
    }

    // Build the content
    let mut lines = Vec::new();
    let mut session_index = 0; // Tracks actual session number
    let mut selected_line_index = 0;

    for (date, day_sessions) in sessions_by_date.iter().rev() {
        // Calculate total hours for the day
        let total_hours: f64 = day_sessions.iter().map(|s| s.duration_hours()).sum();

        // Date header
        lines.push(Line::from(vec![Span::styled(
            format!("📅 {} ({:.1}h)", date, total_hours),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));

        // Sessions for this day
        for session in day_sessions {
            let is_selected = session_index == selected_index;
            if is_selected {
                selected_line_index = lines.len();
            }

            let start_time = format!(
                "{:02}:{:02}",
                session.start_time.hour(),
                session.start_time.minute()
            );
            let end_time = format!(
                "{:02}:{:02}",
                session.end_time.hour(),
                session.end_time.minute()
            );
            let duration = session.duration_hours();

            // Extract short repo name (without org/)
            let repo_parts: Vec<&str> = session.repo.split('/').collect();
            let repo_short = repo_parts.last().unwrap_or(&"unknown");

            let cursor = if is_selected {
                theme().selector
            } else {
                theme().unselected_selector
            };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            // Build Jira issues string
            let issues_str = if session.jira_issues.is_empty() {
                String::new()
            } else {
                format!(" [{}]", session.jira_issues)
            };

            // Single line: time range, duration, repo, issues
            lines.push(Line::from(vec![
                Span::styled(cursor, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(
                    format!(
                        "{}-{} {:.1}h {}{}",
                        start_time, end_time, duration, repo_short, issues_str
                    ),
                    style,
                ),
            ]));

            session_index += 1;
        }

        // Empty line between dates
        lines.push(Line::from(""));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No GitHub sessions found",
            Style::default().fg(Color::DarkGray),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Run 'wtf github fetch' to sync events",
            Style::default().fg(Color::Gray),
        )]));
    }

    // Calculate scroll offset to keep selected item visible
    let visible_lines = area.height.saturating_sub(2) as usize; // Subtract border
    let scroll_offset = if selected_line_index >= visible_lines {
        (selected_line_index - visible_lines + 1).min(lines.len().saturating_sub(visible_lines))
    } else {
        0
    };

    // Build help text
    let shortcuts = build_shortcut_help(&[("C", " Create Worklog"), ("↑↓", " Navigate")]);
    let mut title_spans = vec![
        Span::raw("💻 GitHub Sessions ("),
        Span::raw(sessions.len().to_string()),
        Span::raw(") | "),
    ];
    title_spans.extend(shortcuts);

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(Line::from(title_spans))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme().border)),
        )
        .scroll((scroll_offset as u16, 0))
        .style(Style::default().bg(theme().bg_primary));

    frame.render_widget(paragraph, *area);
}

fn render_github_session_details(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
) {
    use chrono::Timelike;

    let sessions = &data.github_sessions;

    if sessions.is_empty() {
        let paragraph = Paragraph::new("No session selected")
            .block(
                Block::default()
                    .title("💻 Session Details")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme().border)),
            )
            .style(Style::default().bg(theme().bg_primary));
        frame.render_widget(paragraph, *area);
        return;
    }

    // Get sessions in reverse chronological order
    let mut sorted_sessions: Vec<_> = sessions.iter().collect();
    sorted_sessions.sort_by(|a, b| b.start_time.cmp(&a.start_time));

    let session = sorted_sessions
        .get(selected_index)
        .unwrap_or(&sorted_sessions[0]);
    let mut activity_events: Vec<_> = session
        .get_event_ids()
        .into_iter()
        .filter_map(|event_id| data.github_events_by_id.get(&event_id))
        .collect();
    activity_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    let activity_groups = group_activity_events(&activity_events);
    let session_repos = collect_session_repos(session, &activity_events);

    let mut lines = Vec::new();

    // Session info
    lines.push(Line::from(vec![Span::styled(
        "📊 Session Info",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Duration
    lines.push(Line::from(vec![
        Span::styled("⏱  Duration: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{:.1}h", session.duration_hours()),
            Style::default().fg(Color::White),
        ),
    ]));

    // Repo(s)
    lines.push(Line::from(vec![
        Span::styled("📁 Repo: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format_repo_summary(&session_repos, area.width.saturating_sub(12) as usize),
            Style::default().fg(Color::White),
        ),
    ]));

    // Date and time
    let start_time = format!(
        "{:02}:{:02}",
        session.start_time.hour(),
        session.start_time.minute()
    );
    let end_time = format!(
        "{:02}:{:02}",
        session.end_time.hour(),
        session.end_time.minute()
    );
    lines.push(Line::from(vec![
        Span::styled("📅 Date: ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("{} ({}-{})", session.date, start_time, end_time),
            Style::default().fg(Color::White),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("🧩 Events: ", Style::default().fg(Color::Gray)),
        Span::styled(
            activity_events.len().to_string(),
            Style::default().fg(Color::White),
        ),
    ]));

    // Jira issues
    let issues = session.get_jira_issues();
    lines.push(Line::from(vec![Span::styled(
        "🎫 Issues:",
        Style::default().fg(Color::Gray),
    )]));
    if issues.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  • None detected",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for issue_key in &issues {
            lines.push(render_issue_line(
                "  • ",
                issue_key,
                data.github_issue_validations.get(issue_key),
                data.issues_by_key.get(issue_key),
                area.width.saturating_sub(18) as usize,
            ));
        }
    }

    // Check if worklogs exist for this session's issues
    if !issues.is_empty() {
        let session_date = session.date;
        let worklogs_for_date: Vec<_> = data
            .all_worklogs
            .iter()
            .filter(|w| w.started.date_naive() == session_date)
            .filter(|w| issues.contains(&w.issue_id))
            .collect();

        if worklogs_for_date.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("💡 Worklog: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "Not created - Press [C] to create",
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        } else {
            let total_logged: i64 = worklogs_for_date.iter().map(|w| w.time_spent_seconds).sum();
            let total_logged_hours = total_logged as f64 / 3600.0;
            lines.push(Line::from(vec![
                Span::styled("✅ Worklog: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:.1}h logged on these issues", total_logged_hours),
                    Style::default().fg(Color::Green),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Description
    lines.push(Line::from(vec![Span::styled(
        "📝 Activity:",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    if activity_events.is_empty() {
        // Fall back to the merged session description when per-event records are unavailable.
        let max_desc_width = area.width.saturating_sub(4) as usize;
        for line in session.description.split(';') {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if trimmed.len() > max_desc_width {
                    lines.push(Line::from(vec![
                        Span::raw("  • "),
                        Span::styled(
                            &trimmed[..max_desc_width.saturating_sub(5)],
                            Style::default().fg(Color::Gray),
                        ),
                        Span::styled("...", Style::default().fg(Color::DarkGray)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("  • "),
                        Span::styled(trimmed, Style::default().fg(Color::Gray)),
                    ]));
                }
            }
        }
    } else {
        for group in activity_groups {
            lines.push(render_activity_line(&group));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title("💻 Session Details")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme().border)),
        )
        .style(Style::default().bg(theme().bg_primary));
    frame.render_widget(paragraph, *area);
}

fn render_issue_line(
    prefix: &str,
    issue_key: &str,
    validation: Option<&GitHubIssueValidation>,
    issue: Option<&wtf_lib::models::data::Issue>,
    title_max_len: usize,
) -> Line<'static> {
    let mut spans = vec![
        Span::raw(prefix.to_string()),
        Span::styled(issue_key.to_string(), Style::default().fg(Color::Green)),
    ];

    let (status_label, status_color) = match validation
        .copied()
        .unwrap_or(GitHubIssueValidation::Missing)
    {
        GitHubIssueValidation::Cached => ("db", Color::Green),
        GitHubIssueValidation::Remote => ("jira", Color::Blue),
        GitHubIssueValidation::Missing => ("missing", Color::Red),
    };

    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("[{}]", status_label),
        Style::default()
            .fg(status_color)
            .add_modifier(Modifier::BOLD),
    ));

    if let Some(issue) = issue {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            truncate_text(&issue.summary, title_max_len.max(12)),
            Style::default().fg(Color::White),
        ));
    } else if matches!(validation, Some(GitHubIssueValidation::Missing)) {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "not found in Jira",
            Style::default().fg(Color::DarkGray),
        ));
    }

    Line::from(spans)
}

#[derive(Debug)]
struct ActivityGroup {
    activity_name: String,
    start_time: chrono::DateTime<chrono::Utc>,
    end_time: chrono::DateTime<chrono::Utc>,
    issue_keys: Vec<String>,
    count: usize,
}

fn group_activity_events(events: &[&wtf_lib::models::data::GitHubEvent]) -> Vec<ActivityGroup> {
    let mut groups: Vec<ActivityGroup> = Vec::new();

    for event in events {
        let issue_keys = event.get_jira_issues();
        let activity_name = normalized_activity_name(&event.event_type);
        if let Some(last) = groups.last_mut() {
            if last.activity_name == activity_name && last.issue_keys == issue_keys {
                last.end_time = event.timestamp;
                last.count += 1;
                continue;
            }
        }

        groups.push(ActivityGroup {
            activity_name,
            start_time: event.timestamp,
            end_time: event.timestamp,
            issue_keys,
            count: 1,
        });
    }

    groups
}

fn normalized_activity_name(event_type: &str) -> String {
    match event_type {
        "PullRequestReviewEvent" | "PullRequestReviewCommentEvent" => "PR review".to_string(),
        "PullRequestEvent" => "Pull request".to_string(),
        "PushEvent" => "Push".to_string(),
        "IssuesEvent" => "Issue".to_string(),
        "IssueCommentEvent" => "Issue comment".to_string(),
        other => other.trim_end_matches("Event").to_string(),
    }
}

fn render_activity_line(group: &ActivityGroup) -> Line<'static> {
    let time_label = if group.start_time == group.end_time {
        format!(
            "{:02}:{:02}",
            group.start_time.hour(),
            group.start_time.minute()
        )
    } else {
        format!(
            "{:02}:{:02}-{:02}:{:02}",
            group.start_time.hour(),
            group.start_time.minute(),
            group.end_time.hour(),
            group.end_time.minute()
        )
    };

    let count_label = if group.count > 1 {
        format!(" ({})", group.count)
    } else {
        String::new()
    };

    let issue_label = if group.issue_keys.is_empty() {
        "-".to_string()
    } else {
        group.issue_keys.join(", ")
    };

    Line::from(vec![
        Span::raw("  • "),
        Span::styled(time_label, Style::default().fg(Color::Yellow)),
        Span::raw(" "),
        Span::styled(
            format!("{}{}", group.activity_name, count_label),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" "),
        Span::styled(issue_label, Style::default().fg(Color::Green)),
    ])
}

fn collect_session_repos(
    session: &wtf_lib::models::data::GitHubSession,
    activity_events: &[&wtf_lib::models::data::GitHubEvent],
) -> Vec<String> {
    let mut repos = std::collections::BTreeSet::new();

    for event in activity_events {
        repos.insert(event.repo.clone());
    }

    if repos.is_empty() {
        repos.insert(session.repo.clone());
    }

    repos.into_iter().collect()
}

fn format_repo_summary(repos: &[String], max_len: usize) -> String {
    match repos {
        [] => "-".to_string(),
        [single] => truncate_text(single, max_len.max(12)),
        _ => truncate_text(repos.join(", ").as_str(), max_len.max(12)),
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
