use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::logger;
use crate::tui::data::TuiData;
use crate::tui::helpers;
use crate::tui::tab_controller::TabController;
use crate::tui::theme::theme;
use crate::tui::ui_helpers::*;
use crate::tui::{RevertConfirmationState, Tui};
use wtf_lib::models::data::{Sprint, Worklog};
use wtf_lib::services::worklogs_service::LocalWorklogService;

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::tui) struct HistoryTab;

/// Prefix for virtual entry IDs per sprint: "__jira_only__{sprint_id}"
pub(in crate::tui) const JIRA_ONLY_VIRTUAL_PREFIX: &str = "__jira_only__";

impl TabController for HistoryTab {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData) {
        render_history_tab(frame, area, data);
    }

    fn handle_key(&self, tui: &mut Tui, key: KeyEvent) {
        if tui.revert_confirmation_state.is_some() {
            let mut revert_history_id: Option<String> = None;

            {
                let state = tui.revert_confirmation_state.as_mut().expect("checked above");
                if state.reverting {
                    return;
                }

                match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                        state.user_input.push(c);
                    }
                    KeyCode::Backspace => {
                        state.user_input.pop();
                    }
                    KeyCode::Enter => {
                        if let Some(history) = tui
                            .data
                            .worklog_history
                            .iter()
                            .find(|history| history.id == state.history_id)
                        {
                            let worklogs: Vec<_> = history
                                .local_worklogs_id
                                .iter()
                                .filter_map(|worklog_id| {
                                    LocalWorklogService::production().get_worklog(worklog_id)
                                })
                                .collect();
                            let total_hours =
                                worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>() as f64
                                    / 3600.0;

                            if let Ok(user_hours) = state.user_input.parse::<f64>() {
                                if (user_hours - total_hours).abs() < 0.05 {
                                    revert_history_id = Some(state.history_id.clone());
                                } else {
                                    logger::log(format!(
                                        "❌ Incorrect hours entered. Expected {:.1}, got {:.1}",
                                        total_hours, user_hours
                                    ));
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        tui.revert_confirmation_state = None;
                    }
                    _ => {}
                }
            }

            if let Some(history_id) = revert_history_id {
                tui.revert_history(history_id);
            }
            return;
        }

        let (jira_only_count, virtual_ids, sprint_worklogs_for_import): (
            usize,
            Vec<String>,
            Vec<Vec<wtf_lib::models::data::Worklog>>,
        ) = {
            let entries = jira_only_by_sprint(&tui.data);
            let count = entries.len();
            let ids: Vec<String> = entries
                .iter()
                .map(|(sprint, _)| format!("{}{}", JIRA_ONLY_VIRTUAL_PREFIX, sprint.id))
                .collect();
            let worklogs: Vec<Vec<wtf_lib::models::data::Worklog>> = entries
                .iter()
                .map(|(_, sprint_worklogs)| {
                    sprint_worklogs.iter().map(|worklog| (*worklog).clone()).collect()
                })
                .collect();
            (count, ids, worklogs)
        };
        let has_jira_only = jira_only_count > 0;

        let max_index = if tui.data.worklog_history.is_empty() && !has_jira_only {
            0
        } else {
            tui.data.worklog_history.len() + jira_only_count - 1
        };

        if helpers::handle_list_navigation(
            key,
            &mut tui.data.ui_state.selected_history_index,
            max_index,
        ) {
            return;
        }

        let virtual_entry_idx = tui.data.worklog_history.len();
        let on_virtual_entry =
            has_jira_only && tui.data.ui_state.selected_history_index >= virtual_entry_idx;
        let virtual_sprint_i = tui
            .data
            .ui_state
            .selected_history_index
            .saturating_sub(virtual_entry_idx);
        let virtual_id = virtual_ids.get(virtual_sprint_i).cloned();

        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                if on_virtual_entry {
                    if let Some(virtual_id) = &virtual_id {
                        tui.data.ui_state.expanded_history_ids.remove(virtual_id);
                    }
                } else if let Some(history) = tui
                    .data
                    .worklog_history
                    .get(tui.data.ui_state.selected_history_index)
                {
                    tui.data.ui_state.expanded_history_ids.remove(&history.id);
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if on_virtual_entry {
                    if let Some(virtual_id) = virtual_id {
                        tui.data.ui_state.expanded_history_ids.insert(virtual_id);
                    }
                } else if let Some(history) = tui
                    .data
                    .worklog_history
                    .get(tui.data.ui_state.selected_history_index)
                {
                    tui.data
                        .ui_state
                        .expanded_history_ids
                        .insert(history.id.clone());
                }
            }
            KeyCode::Enter => {
                if on_virtual_entry {
                    if let Some(virtual_id) = virtual_id {
                        if tui.data.ui_state.expanded_history_ids.contains(&virtual_id) {
                            tui.data.ui_state.expanded_history_ids.remove(&virtual_id);
                        } else {
                            tui.data.ui_state.expanded_history_ids.insert(virtual_id);
                        }
                    }
                } else if let Some(history) = tui
                    .data
                    .worklog_history
                    .get(tui.data.ui_state.selected_history_index)
                {
                    if tui
                        .data
                        .ui_state
                        .expanded_history_ids
                        .contains(&history.id)
                    {
                        tui.data.ui_state.expanded_history_ids.remove(&history.id);
                    } else {
                        tui.data
                            .ui_state
                            .expanded_history_ids
                            .insert(history.id.clone());
                    }
                }
            }
            KeyCode::Delete => {
                if !on_virtual_entry {
                    if let Some(history) = tui
                        .data
                        .worklog_history
                        .get(tui.data.ui_state.selected_history_index)
                    {
                        tui.revert_confirmation_state = Some(RevertConfirmationState {
                            history_id: history.id.clone(),
                            user_input: String::new(),
                            reverting: false,
                        });
                    }
                }
            }
            KeyCode::Char('D') => {
                if !on_virtual_entry {
                    if let Some(history) = tui
                        .data
                        .worklog_history
                        .get(tui.data.ui_state.selected_history_index)
                    {
                        match LocalWorklogService::production().delete_history_from_db(&history.id) {
                            Ok(()) => {
                                logger::log(
                                    "🗑️ Deleted history entry from database (worklogs remain in Jira)"
                                        .to_string(),
                                );
                                tui.refresh_data();
                            }
                            Err(error) => {
                                logger::log(format!("❌ Failed to delete history: {}", error));
                            }
                        }
                    }
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if on_virtual_entry {
                    if let Some(sprint_worklogs) =
                        sprint_worklogs_for_import.into_iter().nth(virtual_sprint_i)
                    {
                        let count = LocalWorklogService::production()
                            .create_history_for_jira_only_worklogs(&sprint_worklogs);
                        if count > 0 {
                            logger::log(format!(
                                "☁ Imported {} Jira worklog(s) into a new history entry",
                                count
                            ));
                        } else {
                            logger::log("ℹ️  No untracked Jira worklogs to import".to_string());
                        }
                    }
                } else {
                    LocalWorklogService::production().create_history_for_pushed_worklogs();
                    logger::log(
                        "📝 Created recovery history for unhistorized pushed worklogs".to_string(),
                    );
                }
                tui.refresh_data();
            }
            KeyCode::PageUp => {
                tui.data.ui_state.selected_history_index =
                    tui.data.ui_state.selected_history_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                tui.data.ui_state.selected_history_index =
                    (tui.data.ui_state.selected_history_index + 10).min(max_index);
            }
            _ => {}
        }
    }
}

/// Returns one entry per followed sprint that has untracked Jira worklogs,
/// sorted by sprint start date (most recent first).
pub(in crate::tui) fn jira_only_by_sprint<'a>(
    data: &'a TuiData,
) -> Vec<(&'a Sprint, Vec<&'a Worklog>)> {
    use std::collections::HashSet;
    let tracked: HashSet<&str> = data
        .all_worklogs
        .iter()
        .filter_map(|w| w.worklog_id.as_deref())
        .collect();

    let untracked: Vec<&Worklog> = data
        .jira_worklogs
        .iter()
        .filter(|w| !tracked.contains(w.id.as_str()))
        .collect();

    let mut result: Vec<(&Sprint, Vec<&Worklog>)> = data
        .all_sprints
        .iter()
        .filter_map(|sprint| {
            let (start, end) = match (sprint.start, sprint.end) {
                (Some(s), Some(e)) => (s.date_naive(), e.date_naive()),
                _ => return None,
            };
            let sprint_wls: Vec<&Worklog> = untracked
                .iter()
                .filter(|w| {
                    let d = w.started.date_naive();
                    d >= start && d <= end
                })
                .copied()
                .collect();
            if sprint_wls.is_empty() {
                None
            } else {
                Some((sprint, sprint_wls))
            }
        })
        .collect();

    result.sort_by(|a, b| b.0.start.cmp(&a.0.start));
    result
}

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
    let sprint_entries = jira_only_by_sprint(data);
    let has_jira_only = !sprint_entries.is_empty();

    let shortcuts = build_shortcut_help(&[
        ("→", " Expand"),
        ("Del", "ete"),
        ("C", "reate recovery / import"),
    ]);
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

    if history.is_empty() && !has_jira_only {
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
            .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
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
        let selection_icon = if is_selected {
            theme().selector
        } else {
            theme().unselected_selector
        };

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

    // Virtual "Untracked Jira Worklogs" entries — one per sprint
    for (i, (sprint, sprint_wls)) in sprint_entries.iter().enumerate() {
        let virtual_idx = history.len() + i;
        let is_selected = virtual_idx == selected_index;
        let virtual_id = format!("{}{}", JIRA_ONLY_VIRTUAL_PREFIX, sprint.id);
        let is_expanded = expanded_history_ids.contains(&virtual_id);
        let total_seconds: u64 = sprint_wls.iter().map(|w| w.time_spent_seconds).sum();
        let total_hours = total_seconds as f64 / 3600.0;
        let expand_icon = if is_expanded { "🔽" } else { "🔸" };
        let selection_icon = if is_selected {
            theme().selector
        } else {
            theme().unselected_selector
        };

        lines.push(Line::from(vec![
            Span::raw(selection_icon),
            Span::raw(expand_icon),
            Span::raw(" "),
            Span::styled(
                "☁",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                truncate_string(&sprint.name, 30),
                Style::default().fg(Color::Blue),
            ),
            Span::raw(" • "),
            Span::styled(
                format!("{} WL", sprint_wls.len()),
                Style::default().fg(Color::White),
            ),
            Span::raw(" • "),
            Span::styled(
                format!("{:.1}h", total_hours),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled("[C] import", Style::default().fg(Color::DarkGray)),
        ]));

        if is_expanded {
            let visible_count = 5.min(sprint_wls.len());
            let total_count = sprint_wls.len();
            for (j, wl) in sprint_wls.iter().take(visible_count).enumerate() {
                let is_last = j == visible_count - 1 && total_count <= 5;
                let tree_char = if is_last { "└─" } else { "├─" };
                let hours = wl.time_spent_seconds as f64 / 3600.0;
                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled(tree_char, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(wl.issue_id.clone(), Style::default().fg(Color::Cyan)),
                    Span::raw(" • "),
                    Span::styled(
                        format!("{:.1}h", hours),
                        Style::default().fg(Color::DarkGray),
                    ),
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

    // Virtual sprint entries start at index history.len()
    if selected_index >= history.len() {
        let sprint_entries = jira_only_by_sprint(data);
        let virtual_i = selected_index - history.len();
        if let Some((sprint, sprint_wls)) = sprint_entries.into_iter().nth(virtual_i) {
            render_jira_only_details(frame, area, sprint, &sprint_wls, data);
        }
        return;
    }

    if history.is_empty() {
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
        .title("⚠  Revert Preview")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Red).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Get all worklogs for this history
    let worklogs: Vec<_> = history_entry
        .local_worklogs_id
        .iter()
        .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
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
        .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
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

fn render_jira_only_details(
    frame: &mut Frame,
    area: &Rect,
    sprint: &Sprint,
    sprint_wls: &[&Worklog],
    _data: &TuiData,
) {
    use std::collections::HashMap;

    let total_seconds: u64 = sprint_wls.iter().map(|w| w.time_spent_seconds).sum();
    let total_hours = total_seconds as f64 / 3600.0;

    let mut by_issue: HashMap<&str, (f64, usize)> = HashMap::new();
    for wl in sprint_wls {
        let e = by_issue.entry(wl.issue_id.as_str()).or_insert((0.0, 0));
        e.0 += wl.time_spent_seconds as f64 / 3600.0;
        e.1 += 1;
    }

    let block = Block::default()
        .title(format!("☁ {} — Untracked Jira Worklogs", sprint.name))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Blue).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    let mut lines = vec![
        Line::from(vec![
            Span::raw("These Jira worklogs are not tracked locally. "),
            Span::styled(
                "[C]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" to create a revertable history entry."),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Total: "),
            Span::styled(
                format!("{} worklogs  •  {:.1}h", sprint_wls.len(), total_hours),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "By Issue:",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ];

    let mut issue_sorted: Vec<_> = by_issue.iter().collect();
    issue_sorted.sort_by(|a, b| {
        b.1 .0
            .partial_cmp(&a.1 .0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (issue_id, (hours, count)) in issue_sorted.iter().take(10) {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(issue_id.to_string(), Style::default().fg(Color::Cyan)),
            Span::raw(": "),
            Span::styled(
                format!("{:.1}h ({} WL)", hours, count),
                Style::default().fg(Color::Gray),
            ),
        ]));
    }
    if by_issue.len() > 10 {
        lines.push(Line::from(vec![Span::styled(
            format!("  … ({} more issues)", by_issue.len() - 10),
            Style::default().fg(Color::DarkGray),
        )]));
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}
