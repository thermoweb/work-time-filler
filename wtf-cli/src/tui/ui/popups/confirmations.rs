use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::ui_helpers::*;
use crate::tui::theme::theme;
use crate::tui::{GapFillConfirmation, RevertConfirmationState, WorklogCreationConfirmation, WorklogSource};
use wtf_lib::services::worklogs_service::LocalWorklogService;

/// Render unlink confirmation dialog
pub(in crate::tui) fn render_unlink_confirmation(
    frame: &mut Frame,
    data: &TuiData,
    meeting_id: &str,
) {
    // Find the meeting to get its details
    let meeting = data.all_meetings.iter().find(|m| m.id == meeting_id);

    let meeting_title = meeting
        .and_then(|m| m.title.as_ref())
        .map(|t| t.as_str())
        .unwrap_or("Unknown meeting");

    let jira_link = meeting
        .and_then(|m| m.jira_link.as_ref())
        .map(|l| l.as_str())
        .unwrap_or("Unknown");

    // Calculate popup size - centered, medium size
    let area = frame.area();
    let popup_width = 60.min(area.width - 4);
    let popup_height = 8;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    frame.render_widget(
        Block::default().style(Style::default().bg(theme().bg_primary)),
        popup_area,
    );

    // Create message
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Unlink meeting from Jira issue?",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Meeting: "),
            Span::styled(
                truncate_string(meeting_title, 40),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::raw("Issue:   "),
            Span::styled(jira_link, Style::default().fg(Color::Blue)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Yes  "),
            Span::styled(
                "[N]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" No  "),
            Span::styled(
                "[Esc]",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Cancel"),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Confirm Unlink")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, popup_area);
}

pub(in crate::tui) fn render_revert_confirmation(
    frame: &mut Frame,
    data: &TuiData,
    state: &RevertConfirmationState,
) {
    // Find the history entry
    let history_entry = data
        .worklog_history
        .iter()
        .find(|h| h.id == state.history_id);

    let (worklog_count, total_hours) = if let Some(history) = history_entry {
        let worklogs: Vec<_> = history
            .local_worklogs_id
            .iter()
            .filter_map(|wid| LocalWorklogService::get_worklog(wid))
            .collect();
        let total_time = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>();
        (worklogs.len(), total_time as f64 / 3600.0)
    } else {
        (0, 0.0)
    };

    let area = frame.area();

    // Calculate popup size - centered, larger
    let popup_width = 70.min(area.width - 4);
    let popup_height = 14;

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear ONLY the popup area (this removes the background TUI content from this rectangle)
    frame.render_widget(Clear, popup_area);

    let history_id_short = if state.history_id.len() >= 8 {
        &state.history_id[..8]
    } else {
        &state.history_id
    };

    // Create message based on reverting state
    let lines = if state.reverting {
        // Show spinner while reverting
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame_idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            / 80) as usize
            % spinner_frames.len();

        vec![
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    spinner_frames[frame_idx],
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw("  "),
                Span::styled(
                    "Deleting worklogs from Jira...",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    format!("{} worklogs", worklog_count),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    format!("{:.1} hours", total_hours),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Please wait...",
                Style::default().fg(Color::DarkGray),
            )]),
        ]
    } else {
        // Show confirmation form
        vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "⚠️  DANGER: Revert Worklog Push ⚠️",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::raw("This will "),
                Span::styled(
                    "DELETE",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" from Jira:"),
            ]),
            Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    format!("{} worklogs", worklog_count),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    format!("{:.1} hours", total_hours),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::raw("Type the number of hours ("),
                Span::styled(
                    format!("{:.1}", total_hours),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(") to confirm:"),
            ]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![
                Span::styled(
                    &state.user_input,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("_", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "[Esc]",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Cancel"),
            ]),
        ]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" !! Revert {} ", history_id_short))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(theme().bg_primary).fg(Color::White));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme().bg_primary));

    frame.render_widget(paragraph, popup_area);
}

/// Render logs panel showing recent log messages
pub(in crate::tui) fn render_worklog_creation_confirmation(
    frame: &mut Frame,
    state: &WorklogCreationConfirmation,
) {
    let area = frame.area();

    // Calculate popup size - ensure it fits all content
    // Need: 17 content lines + 2 borders = 19 lines minimum
    let popup_width = 72.min(area.width - 4);
    let popup_height = 20.min(area.height - 4);

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear ONLY the popup area
    frame.render_widget(Clear, popup_area);

    let suggested_hours = state.suggested_hours();
    let total_with_full = state.existing_hours + state.requested_hours;

    // Build source description
    let (source_type, source_desc) = match &state.source {
        WorklogSource::Meeting { title, .. } => ("Meeting", title.clone()),
        WorklogSource::GitHub { description, .. } => ("GitHub", description.clone()),
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "⚠️  Daily Time Limit Check",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Creating worklog for: "),
            Span::styled(
                &state.issue_id,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("Source: "),
            Span::styled(source_type, Style::default().fg(Color::Gray)),
            Span::raw(" - "),
            Span::styled(&source_desc, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::raw("Date: "),
            Span::styled(
                state.date.format("%Y-%m-%d").to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Already logged:  "),
            Span::styled(
                format!("{:.1}h", state.existing_hours),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::raw("Requested:      +"),
            Span::styled(
                format!("{:.1}h", state.requested_hours),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::raw("Total would be:  "),
            Span::styled(
                format!("{:.1}h", total_with_full),
                if total_with_full > state.daily_limit {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
            if total_with_full > state.daily_limit {
                Span::styled(
                    format!(" (exceeds {:.0}h limit!)", state.daily_limit),
                    Style::default().fg(Color::Red),
                )
            } else {
                Span::raw("")
            },
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Options:",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                " [F] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "Full     - Log all {:.1}h (total: {:.1}h)",
                state.requested_hours, total_with_full
            )),
        ]),
        Line::from(vec![
            Span::styled(
                " [P] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "Partial  - Log only {:.1}h (total: {:.1}h)",
                suggested_hours,
                state.existing_hours + suggested_hours
            )),
        ]),
        Line::from(vec![
            Span::styled(
                " [S] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Skip     - Don't create this worklog  "),
            Span::styled("[Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" also skips"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Your choice: "),
            Span::styled(
                &state.user_input,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}

pub(in crate::tui) fn render_gap_fill_confirmation(frame: &mut Frame, state: &GapFillConfirmation) {
    let area = frame.area();

    // Calculate popup size
    // Base content: 14 lines + up to 10 gap previews + 2 for borders = ~26 lines max
    let popup_width = 70.min(area.width - 4);
    let content_lines = 14 + state.gaps.len().min(10);
    let popup_height = (content_lines as u16 + 2).min(area.height - 2);

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear popup area
    frame.render_widget(Clear, popup_area);

    let total_hours: f64 = state.gaps.iter().map(|(_, h)| h).sum();

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Fill Gaps Confirmation",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Sprint: "),
            Span::styled(&state.sprint_name, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Issue: "),
            Span::styled(&state.issue_id, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Days to fill: "),
            Span::styled(
                format!("{}", state.gaps.len()),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("Total hours: "),
            Span::styled(
                format!("{:.1}h", total_hours),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Preview:",
            Style::default().fg(Color::Gray),
        )]),
    ];

    // Add preview of gaps (max 10 lines)
    for (date, hours) in state.gaps.iter().take(10) {
        let existing =
            wtf_lib::services::worklogs_service::LocalWorklogService::calculate_daily_total(*date);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                date.format("%Y-%m-%d").to_string(),
                Style::default().fg(Color::White),
            ),
            Span::raw(": +"),
            Span::styled(format!("{:.1}h", hours), Style::default().fg(Color::Green)),
            Span::raw(format!(" ({:.1} → {:.1})", existing, existing + hours)),
        ]));
    }

    if state.gaps.len() > 10 {
        lines.push(Line::from(vec![Span::styled(
            format!("  ... and {} more days", state.gaps.len() - 10),
            Style::default().fg(Color::DarkGray),
        )]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[Y]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" to confirm, "),
        Span::styled(
            "[N]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" or "),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" to cancel"),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}
