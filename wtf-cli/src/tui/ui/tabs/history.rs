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

#[derive(Debug, Clone, PartialEq)]
enum HistoryRow {
    Batch(usize),
    Day(usize, chrono::NaiveDate),
    JiraOnly(usize),
}

impl TabController for HistoryTab {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData) {
        render_history_tab(frame, area, data);
    }

    fn handle_key(&self, tui: &mut Tui, key: KeyEvent) {
        if tui.revert_confirmation_state.is_some() {
            let mut revert_history_id: Option<String> = None;

            {
                let state = tui
                    .revert_confirmation_state
                    .as_mut()
                    .expect("checked above");
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
                                if format!("{:.1}", user_hours) == format!("{:.1}", total_hours) {
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

        let (jira_only_count, jira_only_sprint_ids, jira_only_sprint_worklogs) = {
            let entries = jira_only_by_sprint(&tui.data);
            let count = entries.len();
            let ids: Vec<String> = entries
                .iter()
                .map(|(sprint, _)| format!("{}{}", JIRA_ONLY_VIRTUAL_PREFIX, sprint.id))
                .collect();
            let worklogs: Vec<Vec<wtf_lib::models::data::Worklog>> = entries
                .iter()
                .map(|(_, wls)| wls.iter().map(|wl| (*wl).clone()).collect())
                .collect();
            (count, ids, worklogs)
        };

        if tui.data.worklog_history.is_empty() && jira_only_count == 0 {
            return;
        }

        let flat_rows = build_flat_rows(
            &tui.data,
            &tui.data.ui_state.expanded_history_ids,
            jira_only_count,
        );
        let max_index = flat_rows.len().saturating_sub(1);

        if helpers::handle_list_navigation(
            key,
            &mut tui.data.ui_state.selected_history_index,
            max_index,
        ) {
            return;
        }

        let selected_row = flat_rows
            .get(tui.data.ui_state.selected_history_index)
            .cloned();

        match key.code {
            KeyCode::Left | KeyCode::Char('h') => match &selected_row {
                Some(HistoryRow::Batch(batch_idx)) => {
                    if let Some(history) = tui.data.worklog_history.get(*batch_idx) {
                        tui.data.ui_state.expanded_history_ids.remove(&history.id);
                    }
                }
                Some(HistoryRow::Day(batch_idx, _)) => {
                    // Collapse parent and jump to its header
                    if let Some(history) = tui.data.worklog_history.get(*batch_idx) {
                        tui.data.ui_state.expanded_history_ids.remove(&history.id);
                    }
                    let batch_idx = *batch_idx;
                    let new_rows = build_flat_rows(
                        &tui.data,
                        &tui.data.ui_state.expanded_history_ids,
                        jira_only_count,
                    );
                    if let Some(pos) = new_rows
                        .iter()
                        .position(|r| matches!(r, HistoryRow::Batch(i) if *i == batch_idx))
                    {
                        tui.data.ui_state.selected_history_index = pos;
                    }
                }
                Some(HistoryRow::JiraOnly(sprint_i)) => {
                    if let Some(vid) = jira_only_sprint_ids.get(*sprint_i) {
                        tui.data.ui_state.expanded_history_ids.remove(vid);
                    }
                }
                None => {}
            },
            KeyCode::Right | KeyCode::Char('l') => match &selected_row {
                Some(HistoryRow::Batch(batch_idx)) => {
                    if let Some(history) = tui.data.worklog_history.get(*batch_idx) {
                        tui.data
                            .ui_state
                            .expanded_history_ids
                            .insert(history.id.clone());
                    }
                }
                Some(HistoryRow::Day(_, _)) => {}
                Some(HistoryRow::JiraOnly(sprint_i)) => {
                    if let Some(vid) = jira_only_sprint_ids.get(*sprint_i) {
                        tui.data
                            .ui_state
                            .expanded_history_ids
                            .insert(vid.clone());
                    }
                }
                None => {}
            },
            KeyCode::Enter => match &selected_row {
                Some(HistoryRow::Batch(batch_idx)) => {
                    if let Some(history) = tui.data.worklog_history.get(*batch_idx) {
                        if tui
                            .data
                            .ui_state
                            .expanded_history_ids
                            .contains(&history.id)
                        {
                            tui.data.ui_state.expanded_history_ids.remove(&history.id);
                            let new_rows = build_flat_rows(
                                &tui.data,
                                &tui.data.ui_state.expanded_history_ids,
                                jira_only_count,
                            );
                            let new_max = new_rows.len().saturating_sub(1);
                            tui.data.ui_state.selected_history_index =
                                tui.data.ui_state.selected_history_index.min(new_max);
                        } else {
                            tui.data
                                .ui_state
                                .expanded_history_ids
                                .insert(history.id.clone());
                        }
                    }
                }
                Some(HistoryRow::Day(_, _)) => {}
                Some(HistoryRow::JiraOnly(sprint_i)) => {
                    if let Some(vid) = jira_only_sprint_ids.get(*sprint_i) {
                        if tui.data.ui_state.expanded_history_ids.contains(vid) {
                            tui.data.ui_state.expanded_history_ids.remove(vid);
                        } else {
                            tui.data
                                .ui_state
                                .expanded_history_ids
                                .insert(vid.clone());
                        }
                    }
                }
                None => {}
            },
            KeyCode::Delete => {
                if let Some(HistoryRow::Batch(batch_idx)) = &selected_row {
                    if let Some(history) = tui.data.worklog_history.get(*batch_idx) {
                        tui.revert_confirmation_state = Some(RevertConfirmationState {
                            history_id: history.id.clone(),
                            user_input: String::new(),
                            reverting: false,
                        });
                    }
                }
            }
            KeyCode::Char('D') => {
                if let Some(HistoryRow::Batch(batch_idx)) = &selected_row {
                    if let Some(history) = tui.data.worklog_history.get(*batch_idx) {
                        match LocalWorklogService::production().delete_history_from_db(&history.id)
                        {
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
                let jira_only_sprint_i = match &selected_row {
                    Some(HistoryRow::JiraOnly(i)) => Some(*i),
                    _ => None,
                };
                if let Some(sprint_i) = jira_only_sprint_i {
                    if let Some(owned_wls) = jira_only_sprint_worklogs.into_iter().nth(sprint_i) {
                        let count = LocalWorklogService::production()
                            .create_history_for_jira_only_worklogs(&owned_wls);
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

/// Builds the flat navigable row list from the current history + expansion state.
/// Batch headers always precede their day rows; JiraOnly entries come last.
fn build_flat_rows(
    data: &TuiData,
    expanded_ids: &std::collections::HashSet<String>,
    jira_only_count: usize,
) -> Vec<HistoryRow> {
    let mut rows: Vec<HistoryRow> = Vec::new();

    for (batch_idx, history_entry) in data.worklog_history.iter().enumerate() {
        rows.push(HistoryRow::Batch(batch_idx));
        if expanded_ids.contains(&history_entry.id) {
            let days: std::collections::BTreeSet<chrono::NaiveDate> = history_entry
                .local_worklogs_id
                .iter()
                .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
                .map(|w| w.started.date_naive())
                .collect();
            for date in days {
                rows.push(HistoryRow::Day(batch_idx, date));
            }
        }
    }

    for i in 0..jira_only_count {
        rows.push(HistoryRow::JiraOnly(i));
    }

    rows
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
    use std::collections::HashMap;

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

    let flat_rows = build_flat_rows(data, expanded_history_ids, sprint_entries.len());

    // Cache: batch_idx -> (worklogs, day_secs_totals, day_counts, max_day_secs)
    type BatchCache = HashMap<
        usize,
        (
            Vec<wtf_lib::models::data::LocalWorklog>,
            HashMap<chrono::NaiveDate, i64>,
            HashMap<chrono::NaiveDate, usize>,
            i64,
        ),
    >;
    let mut batch_cache: BatchCache = HashMap::new();

    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line_idx: usize = 0;

    for (flat_idx, row) in flat_rows.iter().enumerate() {
        let is_selected = flat_idx == selected_index;
        if is_selected {
            selected_line_idx = lines.len();
        }
        let selection_icon = if is_selected {
            theme().selector
        } else {
            theme().unselected_selector
        };

        match row {
            HistoryRow::Batch(batch_idx) => {
                let history_entry = &history[*batch_idx];
                let is_expanded = expanded_history_ids.contains(&history_entry.id);

                let (worklogs, _, _, _) = batch_cache.entry(*batch_idx).or_insert_with(|| {
                    let wls: Vec<_> = history_entry
                        .local_worklogs_id
                        .iter()
                        .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
                        .collect();
                    let mut day_secs: HashMap<chrono::NaiveDate, i64> = HashMap::new();
                    let mut day_counts: HashMap<chrono::NaiveDate, usize> = HashMap::new();
                    for w in &wls {
                        let d = w.started.date_naive();
                        *day_secs.entry(d).or_insert(0) += w.time_spent_seconds;
                        *day_counts.entry(d).or_insert(0) += 1;
                    }
                    let max = day_secs.values().copied().max().unwrap_or(1).max(1);
                    (wls, day_secs, day_counts, max)
                });

                let total_secs = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>();
                let total_hours = total_secs as f64 / 3600.0;
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
                let date_str = format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    history_entry.date.year(),
                    history_entry.date.month(),
                    history_entry.date.day(),
                    history_entry.date.hour(),
                    history_entry.date.minute()
                );

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
            }

            HistoryRow::Day(batch_idx, date) => {
                let history_entry = &history[*batch_idx];
                let (_, day_secs_map, day_counts_map, max_day_secs) =
                    batch_cache.entry(*batch_idx).or_insert_with(|| {
                        let wls: Vec<_> = history_entry
                            .local_worklogs_id
                            .iter()
                            .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
                            .collect();
                        let mut day_secs: HashMap<chrono::NaiveDate, i64> = HashMap::new();
                        let mut day_counts: HashMap<chrono::NaiveDate, usize> = HashMap::new();
                        for w in &wls {
                            let d = w.started.date_naive();
                            *day_secs.entry(d).or_insert(0) += w.time_spent_seconds;
                            *day_counts.entry(d).or_insert(0) += 1;
                        }
                        let max = day_secs.values().copied().max().unwrap_or(1).max(1);
                        (wls, day_secs, day_counts, max)
                    });

                let day_secs = day_secs_map.get(date).copied().unwrap_or(0);
                let day_count = day_counts_map.get(date).copied().unwrap_or(0);
                let day_hours = day_secs as f64 / 3600.0;

                const BAR_WIDTH: usize = 8;
                let filled =
                    ((day_secs as f64 / *max_day_secs as f64) * BAR_WIDTH as f64).round() as usize;
                let filled = filled.min(BAR_WIDTH);
                let bar = format!("{}{}", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled));

                let is_last_day = !matches!(
                    flat_rows.get(flat_idx + 1),
                    Some(HistoryRow::Day(bi, _)) if *bi == *batch_idx
                );
                let tree_char = if is_last_day { "└─" } else { "├─" };

                lines.push(Line::from(vec![
                    Span::raw(selection_icon),
                    Span::raw("   "),
                    Span::styled(tree_char, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        date.format("%a %d-%b").to_string(),
                        Style::default().fg(if is_selected {
                            Color::LightCyan
                        } else {
                            Color::White
                        }),
                    ),
                    Span::raw("  "),
                    Span::styled(bar, Style::default().fg(Color::Green)),
                    Span::raw("  "),
                    Span::styled(
                        format!("{:.1}h", day_hours),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("({} WL)", day_count),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            HistoryRow::JiraOnly(sprint_i) => {
                let Some((sprint, sprint_wls)) = sprint_entries.get(*sprint_i) else {
                    continue;
                };
                let virtual_id = format!("{}{}", JIRA_ONLY_VIRTUAL_PREFIX, sprint.id);
                let is_expanded = expanded_history_ids.contains(&virtual_id);
                let total_seconds: u64 = sprint_wls.iter().map(|w| w.time_spent_seconds).sum();
                let total_hours = total_seconds as f64 / 3600.0;
                let expand_icon = if is_expanded { "🔽" } else { "🔸" };

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
        }
    }

    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let scroll_offset = if total_lines <= visible_height {
        0
    } else if selected_line_idx >= total_lines.saturating_sub(visible_height / 2) {
        total_lines.saturating_sub(visible_height)
    } else {
        selected_line_idx.saturating_sub(visible_height / 2)
    };

    let visible: Vec<Line> = lines
        .into_iter()
        .skip(scroll_offset)
        .take(visible_height)
        .collect();
    let paragraph = Paragraph::new(visible).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_history_details(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    expanded_history_ids: &std::collections::HashSet<String>,
) {
    let sprint_entries = jira_only_by_sprint(data);
    let flat_rows = build_flat_rows(data, expanded_history_ids, sprint_entries.len());

    match flat_rows.get(selected_index) {
        Some(HistoryRow::JiraOnly(sprint_i)) => {
            if let Some((sprint, sprint_wls)) = sprint_entries.into_iter().nth(*sprint_i) {
                render_jira_only_details(frame, area, sprint, &sprint_wls, data);
            }
            return;
        }
        Some(HistoryRow::Day(batch_idx, date)) => {
            if let Some(history_entry) = data.worklog_history.get(*batch_idx) {
                let worklogs: Vec<_> = history_entry
                    .local_worklogs_id
                    .iter()
                    .filter_map(|wid| LocalWorklogService::production().get_worklog(wid))
                    .collect();
                let day_wls: Vec<_> = worklogs
                    .into_iter()
                    .filter(|w| w.started.date_naive() == *date)
                    .collect();
                render_day_details(frame, area, *date, &day_wls);
            }
            return;
        }
        _ => {}
    }

    // Batch header selected (or empty)
    if data.worklog_history.is_empty() {
        let block = Block::default()
            .title("⚠  Revert Preview")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Red).bg(theme().bg_primary));
        let inner = block.inner(*area);
        frame.render_widget(block, *area);
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No history selected",
                Style::default().fg(Color::Gray),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let batch_idx = match flat_rows.get(selected_index) {
        Some(HistoryRow::Batch(i)) => *i,
        _ => 0,
    };

    if let Some(history_entry) = data.worklog_history.get(batch_idx) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(*area);
        render_revert_preview(frame, &chunks[0], data, history_entry);
        render_selected_history_item(frame, &chunks[1], data, history_entry);
    }
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

fn render_day_details(
    frame: &mut Frame,
    area: &Rect,
    date: chrono::NaiveDate,
    worklogs: &[wtf_lib::models::data::LocalWorklog],
) {
    use chrono::Timelike;

    let total_secs: i64 = worklogs.iter().map(|w| w.time_spent_seconds).sum();
    let total_hours = total_secs as f64 / 3600.0;

    let block = Block::default()
        .title(format!(
            "📅 {} — {:.1}h total",
            date.format("%A %d %B %Y"),
            total_hours
        ))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    if worklogs.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No worklogs for this day",
                Style::default().fg(Color::Gray),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let mut sorted = worklogs.to_vec();
    sorted.sort_by_key(|w| w.started);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:<5}", "Time"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<16}", "Issue"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<8}", "Duration"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                "Comment",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "─".repeat(70),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    for worklog in &sorted {
        let hours = worklog.time_spent_seconds as f64 / 3600.0;
        let time_str = format!(
            "{:02}:{:02}",
            worklog.started.hour(),
            worklog.started.minute()
        );
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<5}", time_str),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<16}", truncate_string(&worklog.issue_id, 16)),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<8}", format!("{:.1}h", hours)),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                {
                    let msg = worklog
                        .comment
                        .find("]-")
                        .map_or(worklog.comment.as_str(), |i| &worklog.comment[i + 2..]);
                    truncate_string(msg, 50)
                },
                Style::default().fg(Color::Gray),
            ),
        ]));
    }

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
