use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::ui_helpers::*;
use crate::tui::theme::theme;

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

            let cursor = if is_selected { theme().selector } else { theme().unselected_selector };
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
        Span::raw("GitHub Sessions ("),
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
                    .title("Session Details")
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

    // Repo
    lines.push(Line::from(vec![
        Span::styled("📁 Repo: ", Style::default().fg(Color::Gray)),
        Span::styled(&session.repo, Style::default().fg(Color::White)),
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

    // Jira issues
    let issues = session.get_jira_issues();
    lines.push(Line::from(vec![
        Span::styled("🎫 Issues: ", Style::default().fg(Color::Gray)),
        Span::styled(
            if issues.is_empty() {
                "None".to_string()
            } else {
                issues.join(", ")
            },
            if issues.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
    ]));

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

    // Show description (truncated to fit)
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

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Session Details")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme().border)),
        )
        .style(Style::default().bg(theme().bg_primary));

    frame.render_widget(paragraph, *area);
}
