use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::ui_helpers::*;
use crate::tui::theme::theme;
use crate::tui::{GapFillState, IssueSelectionState};

/// Render issue selection popup
pub(in crate::tui) fn render_issue_selection_popup(frame: &mut Frame, state: &IssueSelectionState) {
    use wtf_lib::services::meetings_service::MeetingsService;

    // Get the meeting being linked
    let meeting = MeetingsService::get_meeting_by_id(state.meeting_id.clone());

    // Calculate popup size - 80% width, 80% height
    let area = frame.area();
    let popup_width = (area.width as f32 * 0.8) as u16;
    let popup_height = (area.height as f32 * 0.8) as u16;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear the popup area to make it opaque
    frame.render_widget(Clear, popup_area);

    // Render solid background
    frame.render_widget(
        Block::default().style(Style::default().bg(theme().bg_primary)),
        popup_area,
    );

    // Filter issues based on search query
    let filtered_issues: Vec<&wtf_lib::models::data::Issue> = state
        .all_issues
        .iter()
        .filter(|issue| {
            if state.search_query.is_empty() {
                true
            } else {
                let query_lower = state.search_query.to_lowercase();
                issue.key.to_lowercase().contains(&query_lower)
                    || issue.summary.to_lowercase().contains(&query_lower)
            }
        })
        .collect();

    // Render the issue list
    let mut lines = vec![];

    // Add search bar
    let search_display = if state.search_query.is_empty() {
        "Type to search...".to_string()
    } else {
        state.search_query.clone()
    };

    lines.push(Line::from(vec![
        Span::styled("Search: ", Style::default().fg(Color::Yellow)),
        Span::styled(
            search_display,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("_", Style::default().fg(Color::DarkGray)),
    ]));
    lines.push(Line::from(""));

    if filtered_issues.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No matching Jira issues found",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        // Calculate visible window (account for search bar + borders)
        let visible_height = popup_height.saturating_sub(6) as usize;
        let total_issues = filtered_issues.len();

        // Calculate scroll position
        let scroll_offset = if state.selected_issue_index < visible_height / 2 {
            0
        } else if state.selected_issue_index >= total_issues.saturating_sub(visible_height / 2) {
            total_issues.saturating_sub(visible_height)
        } else {
            state
                .selected_issue_index
                .saturating_sub(visible_height / 2)
        };

        // Render visible issues
        let visible_issues = filtered_issues
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height);

        for (idx, issue) in visible_issues {
            let is_selected = idx == state.selected_issue_index;

            let cursor = if is_selected { "❯ " } else { "  " };
            let base_style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            // Truncate summary to fit
            let max_summary_len = popup_width.saturating_sub(20) as usize;
            let summary = &issue.summary;
            let truncated_summary = if summary.len() > max_summary_len {
                format!("{}…", &summary[..max_summary_len.saturating_sub(1)])
            } else {
                summary.to_string()
            };

            lines.push(Line::from(vec![
                Span::styled(cursor, base_style.fg(Color::Yellow)),
                Span::styled(
                    &issue.key,
                    base_style.fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" - ", base_style.fg(Color::DarkGray)),
                Span::styled(truncated_summary, base_style.fg(Color::White)),
            ]));
        }
    }

    let showing = if !state.search_query.is_empty() {
        format!("{}/{}", filtered_issues.len(), state.all_issues.len())
    } else {
        format!("{}", state.all_issues.len())
    };

    let meeting_info = if let Some(ref m) = meeting {
        let meeting_title = m.title.as_deref().unwrap_or("Untitled meeting");
        format!("Linking: {} | ", truncate_string(meeting_title, 40))
    } else {
        String::new()
    };

    let title = format!(
        "{}Select Jira Issue ({}) | Type to search | [Enter] Select | [Esc] Cancel",
        meeting_info, showing
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}

pub(in crate::tui) fn render_gap_fill_issue_selection(frame: &mut Frame, state: &GapFillState) {
    let area = frame.area();

    // Calculate popup size - ensure enough space for content
    // Need: borders (2) + list (min 10) + search (3) + help (1) = 16 minimum
    let popup_width = 80.min(area.width - 4);
    let popup_height = 35.min(area.height - 4).max(16);

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear popup area
    frame.render_widget(Clear, popup_area);

    // Filter issues based on search
    let filtered_issues: Vec<_> = state
        .all_issues
        .iter()
        .filter(|issue| {
            if state.search_query.is_empty() {
                true
            } else {
                let query_lower = state.search_query.to_lowercase();
                issue.key.to_lowercase().contains(&query_lower)
                    || issue.summary.to_lowercase().contains(&query_lower)
            }
        })
        .collect();

    let block = Block::default()
        .title("Select Issue to Fill Gaps")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Create list items
    let items: Vec<ListItem> = filtered_issues
        .iter()
        .enumerate()
        .map(|(idx, issue)| {
            let style = if idx == state.selected_issue_index {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            let content = format!(
                "{:<15} {}",
                issue.key,
                issue.summary.chars().take(50).collect::<String>(),
            );

            ListItem::new(content).style(style)
        })
        .collect();

    // Split into list, search bar, and help text
    // Ensure we always have room for search (3) + help (1) = 4 lines minimum
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // List needs at least 5 lines to be useful
            Constraint::Length(3), // Search bar (with borders)
            Constraint::Length(1), // Help text
        ])
        .split(inner);

    // Render list
    let list =
        List::new(items).highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(list, chunks[0]);

    // Render search bar
    let search_text = if state.search_query.is_empty() {
        "Type to search...".to_string()
    } else {
        state.search_query.clone()
    };

    let search_paragraph = Paragraph::new(search_text)
        .block(Block::default().title("Search").borders(Borders::ALL))
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(search_paragraph, chunks[1]);

    // Render help text
    let help = Paragraph::new(vec![Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(" Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Green)),
        Span::raw(" Select  "),
        Span::styled("Esc", Style::default().fg(Color::Red)),
        Span::raw(" Cancel"),
    ])])
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[2]);
}
