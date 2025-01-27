use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::ui_helpers::*;
use crate::tui::theme::theme;
use wtf_lib::models::data::LocalWorklogState;

/// Worklogs tab - Split view with list and details
pub(in crate::tui) fn render_worklogs_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let selected_index = data.ui_state.selected_worklog_index;
    let filter_staged_only = data.ui_state.filter_staged_only;
    
    render_list_detail_layout(
        frame,
        area,
        |f, a| render_worklogs_list(f, a, data, selected_index, filter_staged_only),
        |f, a| render_worklog_details(f, a, data, selected_index, filter_staged_only),
    );
}

fn render_worklogs_list(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    filter_staged_only: bool,
) {
    use chrono::{Datelike, Timelike};

    // Sort and filter worklogs
    let mut sorted_worklogs = data.all_worklogs.clone();
    sorted_worklogs.sort_by(|a, b| b.started.cmp(&a.started));

    let worklogs: Vec<_> = if filter_staged_only {
        sorted_worklogs
            .into_iter()
            .filter(|w| {
                w.status == LocalWorklogState::Staged || w.status == LocalWorklogState::Created
            })
            .collect()
    } else {
        sorted_worklogs
    };

    let filter_text = if filter_staged_only {
        " [FILTERED: Unpushed Only]"
    } else {
        ""
    };

    // Build contextual help text
    let shortcuts_data = vec![
        ("A", " Stage/Unstage"),
        ("Ctrl+A", " Stage All"),
        ("P", "ush"),
        ("Del", " Delete"),
        ("X", " Reset"),
        ("F", "ilter"),
    ];
    let shortcuts = build_shortcut_help(&shortcuts_data);

    let mut title_spans = vec![
        Span::raw("📊 Worklogs ("),
        Span::raw(worklogs.len().to_string()),
        Span::raw(")"),
        Span::raw(filter_text),
        Span::raw(" | "),
    ];
    title_spans.extend(shortcuts);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(title_spans))
        .title_alignment(Alignment::Left)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    if worklogs.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No worklogs",
                Style::default().fg(Color::Yellow),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    // Calculate scroll offset to keep selected item visible
    let visible_height = inner.height as usize;
    let total_worklogs = worklogs.len();

    let scroll_offset = if total_worklogs <= visible_height {
        0
    } else if selected_index >= total_worklogs.saturating_sub(visible_height / 2) {
        total_worklogs.saturating_sub(visible_height)
    } else {
        selected_index.saturating_sub(visible_height / 2)
    };

    // Render visible worklogs
    let visible_worklogs = worklogs
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height);

    let mut lines = Vec::new();

    for (idx, worklog) in visible_worklogs {
        let is_selected = idx == selected_index;

        // Status indicator - using consistent size characters
        let (status_icon, status_color) = match worklog.status {
            LocalWorklogState::Staged => ("●", Color::Yellow), // Filled circle
            LocalWorklogState::Pushed => ("✓", Color::Green),  // Checkmark
            LocalWorklogState::Created => ("○", Color::Gray),  // Hollow circle
        };

        let date_str = format!(
            "{}-{:02}-{:02}",
            worklog.started.year(),
            worklog.started.month(),
            worklog.started.day()
        );
        let time_str = format!(
            "{:02}:{:02}",
            worklog.started.hour(),
            worklog.started.minute()
        );
        let hours = worklog.time_spent_seconds as f64 / 3600.0;

        // Get issue title if available
        let issue_title = data
            .issues_by_key
            .get(&worklog.issue_id)
            .map(|issue| truncate_string(&issue.summary, 40))
            .unwrap_or_else(|| String::from(""));

        // Single line: status, date, time, issue, title, hours
        let line = Line::from(vec![
            Span::styled(
                if is_selected { theme().selector } else { theme().unselected_selector },
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                status_icon,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:<10}", date_str),
                Style::default().fg(Color::White),
            ),
            Span::raw(" "),
            Span::styled(format!("{:<5}", time_str), Style::default().fg(Color::Gray)),
            Span::raw("  "),
            Span::styled(
                format!("{:<15}", truncate_string(&worklog.issue_id, 15)),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:<40}", issue_title),
                Style::default().fg(Color::Gray),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:>5.1}h", hours),
                Style::default().fg(Color::Yellow),
            ),
        ]);

        lines.push(line);
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_daily_summary(frame: &mut Frame, area: &Rect, data: &TuiData) {
    use chrono::Datelike;
    use std::collections::{BTreeMap, HashSet};

    let block = Block::default()
        .title("📅 Daily Summary (Staged Worklogs)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Track which dates we've already warned about (to avoid spam)
    let mut logged_overwork_dates: HashSet<chrono::NaiveDate> = HashSet::new();

    // Group worklogs by day
    let mut daily_data: BTreeMap<chrono::NaiveDate, (i64, i64)> = BTreeMap::new(); // (staged_seconds, pushed_seconds)

    for worklog in &data.all_worklogs {
        let date = worklog.started.date_naive();
        let entry = daily_data.entry(date).or_insert((0, 0));

        match worklog.status {
            LocalWorklogState::Staged => entry.0 += worklog.time_spent_seconds,
            LocalWorklogState::Pushed => entry.1 += worklog.time_spent_seconds,
            LocalWorklogState::Created => {} // Don't count created (not staged yet)
        }
    }

    // Filter to only days with staged worklogs
    let days_with_staged: Vec<_> = daily_data
        .iter()
        .filter(|(_, (staged, _))| *staged > 0)
        .collect();

    if days_with_staged.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No staged worklogs",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "When you push, the following time will be logged:",
            Style::default().fg(Color::Gray),
        )]),
        Line::from(""),
    ];

    // Calculate totals
    let mut total_staged_seconds = 0i64;

    for (date, (staged_seconds, pushed_seconds)) in &days_with_staged {
        total_staged_seconds += *staged_seconds;

        let staged_hours = *staged_seconds as f64 / 3600.0;
        let pushed_hours = *pushed_seconds as f64 / 3600.0;
        let total_hours = staged_hours + pushed_hours;

        // Warn if total exceeds 10h (with Chronie's wisdom!)
        let warning_icon = if total_hours > 10.0 { " ⚠️ " } else { "" };
        let date_color = if total_hours > 10.0 {
            Color::Red
        } else {
            Color::White
        };

        // Add Chronie warning to logs for overwork (once per day that exceeds limit)
        if total_hours > 12.0 && !logged_overwork_dates.contains(date) {
            crate::tui::log_chronie_message("overwork", "⚠️");
            logged_overwork_dates.insert(**date);
        }

        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "{:>10}",
                    format!("{}-{:02}-{:02}", date.year(), date.month(), date.day())
                ),
                Style::default().fg(date_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("+ {:>5.1}h", staged_hours),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  ("),
            Span::styled(
                format!("{:.1}h", pushed_hours),
                Style::default().fg(Color::Green),
            ),
            Span::raw(" already)  "),
            Span::styled(
                format!("= {:>5.1}h", total_hours),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(warning_icon, Style::default().fg(Color::Red)),
        ]));
    }

    // Add summary
    lines.push(Line::from(""));
    lines.push(Line::from("─────────────────────────────────────────"));
    lines.push(Line::from(""));

    let total_staged_hours = total_staged_seconds as f64 / 3600.0;

    lines.push(Line::from(vec![
        Span::raw("Total to push: "),
        Span::styled(
            format!("{:.1}h", total_staged_hours),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" ({} days)", days_with_staged.len())),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Legend:",
        Style::default().fg(Color::Gray),
    )]));
    lines.push(Line::from(vec![
        Span::styled("  + X.Xh", Style::default().fg(Color::Yellow)),
        Span::raw(" = staged (will be pushed)"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  (X.Xh already)", Style::default().fg(Color::Green)),
        Span::raw(" = already pushed"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  = X.Xh", Style::default().fg(Color::Cyan)),
        Span::raw(" = total for the day"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ⚠️ ", Style::default().fg(Color::Red)),
        Span::raw(" = exceeds 10h"),
    ]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_selected_worklog_info(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    filter_staged_only: bool,
) {
    use chrono::{Datelike, Timelike};

    let block = Block::default()
        .title("📝 Selected Worklog")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Get the selected worklog
    let mut sorted_worklogs = data.all_worklogs.clone();
    sorted_worklogs.sort_by(|a, b| b.started.cmp(&a.started));

    let worklogs: Vec<_> = if filter_staged_only {
        sorted_worklogs
            .into_iter()
            .filter(|w| {
                w.status == LocalWorklogState::Staged || w.status == LocalWorklogState::Created
            })
            .collect()
    } else {
        sorted_worklogs
    };

    if worklogs.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled("No worklog", Style::default().fg(Color::Gray))),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let worklog = if let Some(w) = worklogs.get(selected_index) {
        w
    } else {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No worklog selected",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    };

    // Build condensed details
    let status_text = match worklog.status {
        LocalWorklogState::Created => ("Created", Color::Gray),
        LocalWorklogState::Staged => ("Staged", Color::Yellow),
        LocalWorklogState::Pushed => ("Pushed", Color::Green),
    };

    let date_str = format!(
        "{}-{:02}-{:02}",
        worklog.started.year(),
        worklog.started.month(),
        worklog.started.day()
    );
    let time_str = format!(
        "{:02}:{:02}",
        worklog.started.hour(),
        worklog.started.minute()
    );
    let hours = worklog.time_spent_seconds as f64 / 3600.0;

    // Get issue title if available
    let issue_title = data
        .issues_by_key
        .get(&worklog.issue_id)
        .map(|issue| &issue.summary)
        .map(|s| {
            if s.len() > 50 {
                format!("{}...", &s[..47])
            } else {
                s.clone()
            }
        })
        .unwrap_or_else(|| String::from("No title"));

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                status_text.0,
                Style::default()
                    .fg(status_text.1)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" • "),
            Span::styled(&worklog.issue_id, Style::default().fg(Color::Cyan)),
            Span::raw(" • "),
            Span::styled(format!("{:.1}h", hours), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![Span::styled(
            format!("{} at {}", date_str, time_str),
            Style::default().fg(Color::White),
        )]),
        Line::from(vec![Span::styled(
            &issue_title,
            Style::default().fg(Color::Gray),
        )]),
    ];

    // Add meeting info if linked
    if let Some(ref meeting_id) = worklog.meeting_id {
        lines.push(Line::from(vec![
            Span::raw("From: "),
            Span::styled(
                if meeting_id.len() > 35 {
                    format!("{}...", &meeting_id[..32])
                } else {
                    meeting_id.clone()
                },
                Style::default().fg(Color::Blue),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_worklog_details(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    filter_staged_only: bool,
) {
    use chrono::{Datelike, Timelike};

    // Check if there are any staged worklogs
    let staged_worklogs: Vec<_> = data
        .all_worklogs
        .iter()
        .filter(|w| w.status == LocalWorklogState::Staged)
        .collect();

    // If there are staged worklogs, split the panel into summary (top) and details (bottom)
    if !staged_worklogs.is_empty() {
        // Split the area vertically: 70% for daily summary, 30% for selected worklog details
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(70), // Daily summary
                Constraint::Percentage(30), // Selected worklog details
            ])
            .split(*area);

        render_daily_summary(frame, &chunks[0], data);
        render_selected_worklog_info(frame, &chunks[1], data, selected_index, filter_staged_only);
        return;
    }

    // Otherwise, show individual worklog details as before (full panel)
    let block = Block::default()
        .title("Details")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Get the selected worklog
    let mut sorted_worklogs = data.all_worklogs.clone();
    sorted_worklogs.sort_by(|a, b| b.started.cmp(&a.started));

    let worklogs: Vec<_> = if filter_staged_only {
        sorted_worklogs
            .into_iter()
            .filter(|w| {
                w.status == LocalWorklogState::Staged || w.status == LocalWorklogState::Created
            })
            .collect()
    } else {
        sorted_worklogs
    };

    if worklogs.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No worklog selected",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let worklog = if let Some(w) = worklogs.get(selected_index) {
        w
    } else {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No worklog selected",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    };

    // Build details
    let status_text = match worklog.status {
        LocalWorklogState::Created => ("Created", Color::Gray),
        LocalWorklogState::Staged => ("Staged", Color::Yellow),
        LocalWorklogState::Pushed => ("Pushed", Color::Green),
    };

    let date_str = format!(
        "{}-{:02}-{:02}",
        worklog.started.year(),
        worklog.started.month(),
        worklog.started.day()
    );
    let time_str = format!(
        "{:02}:{:02}",
        worklog.started.hour(),
        worklog.started.minute()
    );
    let hours = worklog.time_spent_seconds as f64 / 3600.0;

    // Calculate stats for summary
    let total_worklogs = data.all_worklogs.len();
    let unpushed_count = data
        .all_worklogs
        .iter()
        .filter(|w| w.status != LocalWorklogState::Pushed)
        .count();
    let pushed_count = total_worklogs - unpushed_count;

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                status_text.0,
                Style::default()
                    .fg(status_text.1)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Date: "),
            Span::styled(
                format!("{} at {}", date_str, time_str),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::raw("Issue: "),
            Span::styled(&worklog.issue_id, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Time: "),
            Span::styled(format!("{:.1}h", hours), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![Span::raw("Comment:")]),
        Line::from(vec![Span::styled(
            &worklog.comment,
            Style::default().fg(Color::Gray),
        )]),
    ];

    // Add meeting info if linked
    if let Some(ref meeting_id) = worklog.meeting_id {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("From meeting: "),
            Span::styled(
                truncate_string(meeting_id, 30),
                Style::default().fg(Color::Blue),
            ),
        ]));
    }

    // Add summary stats
    lines.push(Line::from(""));
    lines.push(Line::from("─────────────────────────"));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("Total: "),
        Span::styled(
            format!("{} worklogs", total_worklogs),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("Unpushed: "),
        Span::styled(
            format!("{}", unpushed_count),
            Style::default().fg(Color::Yellow),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("Pushed: "),
        Span::styled(
            format!("{}", pushed_count),
            Style::default().fg(Color::Green),
        ),
    ]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}
