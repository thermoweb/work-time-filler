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
use wtf_lib::services::worklogs_service::LocalWorklogService;

pub(in crate::tui) fn render_history_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let selected_index = data.ui_state.selected_history_index;
    let expanded_history_ids = &data.ui_state.expanded_history_ids;
    
    render_list_detail_layout(
        frame,
        area,
        |f, a| render_history_list(f, a, data, selected_index, expanded_history_ids),
        |f, a| render_history_details(f, a, data, selected_index, expanded_history_ids),
    );
}

fn render_history_list(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    expanded_history_ids: &std::collections::HashSet<String>,
) {
    use chrono::{Datelike, Timelike};

    let history = &data.worklog_history;

    let shortcuts =
        build_shortcut_help(&[("→", " Expand"), ("Del", "ete"), ("C", "reate recovery")]);
    let mut title_spans = vec![
        Span::raw("📜 History ("),
        Span::raw(history.len().to_string()),
        Span::raw(") | "),
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

    if history.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No history entries",
                Style::default().fg(Color::Yellow),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    // Build tree lines
    let mut lines = Vec::new();

    for (idx, history_entry) in history.iter().enumerate() {
        let is_selected = idx == selected_index;
        let is_expanded = expanded_history_ids.contains(&history_entry.id);

        // Get worklogs for this history entry
        let worklogs: Vec<_> = history_entry
            .local_worklogs_id
            .iter()
            .filter_map(|wid| LocalWorklogService::get_worklog(wid))
            .collect();

        let total_time = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>();
        let total_hours = total_time as f64 / 3600.0;

        // Color based on size
        let count_color = if worklogs.len() > 100 {
            Color::Red
        } else if worklogs.len() > 50 {
            Color::Yellow
        } else if worklogs.len() > 10 {
            Color::White
        } else {
            Color::Gray
        };

        let expand_icon = if is_expanded { "🔽" } else { "🔸" };
        let selection_icon = if is_selected { theme().selector } else { theme().unselected_selector };

        let date_str = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            history_entry.date.year(),
            history_entry.date.month(),
            history_entry.date.day(),
            history_entry.date.hour(),
            history_entry.date.minute()
        );

        // Parent line
        lines.push(Line::from(vec![
            Span::raw(selection_icon),
            Span::raw(expand_icon),
            Span::raw(" "),
            Span::styled(
                format!("[{}]", &history_entry.id[..8]),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
            Span::styled(date_str, Style::default().fg(Color::White)),
            Span::raw(" • "),
            Span::styled(
                format!("{} WL", worklogs.len()),
                Style::default().fg(count_color),
            ),
            Span::raw(" • "),
            Span::styled(
                format!("{:.1}h", total_hours),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        // If expanded, show child worklogs (top 5)
        if is_expanded {
            let mut sorted_worklogs = worklogs.clone();
            sorted_worklogs.sort_by(|a, b| b.time_spent_seconds.cmp(&a.time_spent_seconds));

            let visible_count = 5.min(sorted_worklogs.len());
            let total_count = sorted_worklogs.len();

            for (i, worklog) in sorted_worklogs.into_iter().take(visible_count).enumerate() {
                let is_last = i == visible_count - 1 && total_count <= 5;
                let tree_char = if is_last { "└─" } else { "├─" };
                let hours = worklog.time_spent_seconds as f64 / 3600.0;

                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled(tree_char, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(worklog.issue_id, Style::default().fg(Color::Cyan)),
                    Span::raw(" • "),
                    Span::styled(format!("{:.1}h", hours), Style::default().fg(Color::Gray)),
                ]));
            }

            if total_count > visible_count {
                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled("└─", Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        format!("... ({} more)", total_count - visible_count),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_history_details(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    _expanded_history_ids: &std::collections::HashSet<String>,
) {
    let history = &data.worklog_history;

    if history.is_empty() || selected_index >= history.len() {
        let block = Block::default()
            .title("!! Revert Preview")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Red).bg(theme().bg_primary));

        let inner = block.inner(*area);
        frame.render_widget(block, *area);

        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No history selected",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let history_entry = &history[selected_index];

    // Split the area: 70% revert preview, 30% selected item
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70), // Revert preview
            Constraint::Percentage(30), // Selected item
        ])
        .split(*area);

    // Render revert preview
    render_revert_preview(frame, &chunks[0], data, history_entry);

    // Render selected item details
    render_selected_history_item(frame, &chunks[1], data, history_entry);
}

fn render_revert_preview(
    frame: &mut Frame,
    area: &Rect,
    _data: &TuiData,
    history_entry: &wtf_lib::models::data::LocalWorklogHistory,
) {
    use chrono::{Datelike, Timelike};
    use std::collections::HashMap;

    let block = Block::default()
        .title("!! Revert Preview")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Red).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Get all worklogs for this history
    let worklogs: Vec<_> = history_entry
        .local_worklogs_id
        .iter()
        .filter_map(|wid| LocalWorklogService::get_worklog(wid))
        .collect();

    if worklogs.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No worklogs in this history",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let total_time = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>();
    let total_hours = total_time as f64 / 3600.0;

    // Group by day
    let mut daily_totals: HashMap<chrono::NaiveDate, (i64, usize)> = HashMap::new();
    for worklog in &worklogs {
        let date = worklog.started.date_naive();
        let entry = daily_totals.entry(date).or_insert((0, 0));
        entry.0 += worklog.time_spent_seconds;
        entry.1 += 1;
    }

    // Group by issue
    let mut issue_totals: HashMap<String, (i64, usize)> = HashMap::new();
    for worklog in &worklogs {
        let entry = issue_totals
            .entry(worklog.issue_id.clone())
            .or_insert((0, 0));
        entry.0 += worklog.time_spent_seconds;
        entry.1 += 1;
    }

    let mut lines = vec![
        Line::from(vec![
            Span::raw("Selected: "),
            Span::styled(
                format!("[{}]", &history_entry.id[..8]),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::raw("Pushed: "),
            Span::styled(
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    history_entry.date.year(),
                    history_entry.date.month(),
                    history_entry.date.day(),
                    history_entry.date.hour(),
                    history_entry.date.minute()
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Will DELETE from Jira:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::raw("  • "),
            Span::styled(
                format!("{} worklogs", worklogs.len()),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::raw("  • "),
            Span::styled(
                format!("{:.1}h total", total_hours),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
    ];

    // Add daily breakdown (top 5 days)
    if !daily_totals.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "By Day:",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]));

        let mut daily_sorted: Vec<_> = daily_totals.iter().collect();
        // Sort by time descending, then by date descending for stable ordering
        daily_sorted.sort_by(|a, b| {
            match b.1 .0.cmp(&a.1 .0) {
                std::cmp::Ordering::Equal => b.0.cmp(&a.0), // If same time, newer date first
                other => other,
            }
        });

        for (date, (seconds, count)) in daily_sorted.iter().take(5) {
            let hours = *seconds as f64 / 3600.0;
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{:04}-{:02}-{:02}: {:.1}h ({} WL)",
                        date.year(),
                        date.month(),
                        date.day(),
                        hours,
                        count
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ]));
        }

        if daily_sorted.len() > 5 {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("... ({} more days)", daily_sorted.len() - 5),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        lines.push(Line::from(""));
    }

    // Add top issues
    if !issue_totals.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "Top Issues:",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]));

        let mut issue_sorted: Vec<_> = issue_totals.iter().collect();
        // Sort by time descending, then by issue_id alphabetically for stable ordering
        issue_sorted.sort_by(|a, b| {
            match b.1 .0.cmp(&a.1 .0) {
                std::cmp::Ordering::Equal => a.0.cmp(&b.0), // If same time, alphabetical order
                other => other,
            }
        });

        for (issue_id, (seconds, count)) in issue_sorted.iter().take(5) {
            let hours = *seconds as f64 / 3600.0;
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(issue_id.as_str(), Style::default().fg(Color::Cyan)),
                Span::raw(": "),
                Span::styled(
                    format!("{:.1}h ({} WL)", hours, count),
                    Style::default().fg(Color::Gray),
                ),
            ]));
        }

        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![Span::styled(
        "Press [Del] to delete this push from Jira",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_selected_history_item(
    frame: &mut Frame,
    area: &Rect,
    _data: &TuiData,
    history_entry: &wtf_lib::models::data::LocalWorklogHistory,
) {
    let block = Block::default()
        .title("📌 Selected")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    let worklogs: Vec<_> = history_entry
        .local_worklogs_id
        .iter()
        .filter_map(|wid| LocalWorklogService::get_worklog(wid))
        .collect();

    let total_time = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>();
    let total_hours = total_time as f64 / 3600.0;

    let lines = vec![
        Line::from(vec![
            Span::raw("ID: "),
            Span::styled(&history_entry.id[..8], Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::raw("Count: "),
            Span::styled(
                format!("{} worklogs", worklogs.len()),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::raw("Total: "),
            Span::styled(
                format!("{:.1}h", total_hours),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}
