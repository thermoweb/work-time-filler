use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::{GapFillState, WizardState, WizardStep};
use crate::tui::theme::theme;

pub(in crate::tui) fn render_wizard(
    frame: &mut Frame,
    wizard: &WizardState,
    _data: &TuiData,
    gap_fill_state: &Option<GapFillState>,
) {
    use WizardStep;

    // Create centered popup area - larger to show all content
    let area = frame.area();
    let popup_width = 90.min(area.width.saturating_sub(4));
    let popup_height = 35.min(area.height.saturating_sub(4));
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear area for opacity
    frame.render_widget(Clear, popup_area);

    // Build wizard content
    let mut lines = vec![];

    // Title with Chronie!
    lines.push(Line::from(vec![Span::styled(
        "🧙 Chronie, Chronurgist & Master of Worklogs",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "   Chief Imputation Officer",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]));

    // Add random startup quote if available (use stored message from wizard state)
    if let Some(ref quote) = wizard.startup_message {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(
                quote,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::raw(format!(
        "Sprint: {}",
        wizard.sprint_name
    ))]));
    lines.push(Line::from(""));

    // Vertical progress indicator with step names
    // Map internal step numbers to displayed steps (auto-link is merged with sync)
    let step_mapping = [
        (1, "1. Sync & auto-link"),     // Steps 1 & 2a merged
        (2, "2. Manual-link meetings"), // Step 2b
        (3, "3. Create meeting worklogs"),
        (4, "4. Create GitHub worklogs"),
        (5, "5. Fill gaps"),
        (6, "6. Review worklogs"),
        (7, "7. Push to Jira"),
    ];

    for (_display_num, (internal_num, name)) in step_mapping.iter().enumerate() {
        let (symbol, color, status_text) = if wizard.completed_steps.contains(internal_num) {
            // Check if this step was skipped (has a skip reason)
            if let Some(reason) = wizard.skip_reasons.get(internal_num) {
                ("↓", Color::DarkGray, format!(" (skipped: {})", reason))
            } else {
                ("✓", Color::Green, String::new())
            }
        } else if matches_current_step(*internal_num, &wizard.current_step) {
            // Spinner frames (braille pattern)
            let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let spinner = spinner_chars[wizard.spinner_frame % spinner_chars.len()];
            (spinner, Color::Yellow, String::new())
        } else {
            (" ", Color::DarkGray, String::new())
        };

        lines.push(Line::from(vec![
            Span::styled(format!("[{}] ", symbol), Style::default().fg(color)),
            Span::styled(*name, Style::default().fg(color)),
            Span::styled(status_text, Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("─".repeat(70)));
    lines.push(Line::from(""));

    // Current step content
    match &wizard.current_step {
        WizardStep::Syncing => {
            lines.push(Line::from("⏳ Syncing data and auto-linking meetings..."));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "Fetching latest meetings, issues, and sprints...",
            ));
        }
        WizardStep::AutoLinking => {
            lines.push(Line::from("🔗 Auto-linking meetings with Jira issues..."));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "Finding obvious issue references (PROJ-123 patterns)...",
            ));
        }
        WizardStep::ManualLinking {
            unlinked_meetings,
            selected_index,
        } => {
            lines.push(Line::from(format!(
                "{} unlinked meetings remaining:",
                unlinked_meetings.len()
            )));
            lines.push(Line::from(""));

            // Show meetings with scrolling (max 10 visible)
            let max_visible = 10;
            let total = unlinked_meetings.len();

            // Calculate scroll window
            let start_idx = if total <= max_visible {
                0
            } else if *selected_index < max_visible / 2 {
                0
            } else if *selected_index >= total - max_visible / 2 {
                total.saturating_sub(max_visible)
            } else {
                selected_index.saturating_sub(max_visible / 2)
            };

            let end_idx = (start_idx + max_visible).min(total);

            // Show scroll indicator if not at top
            if start_idx > 0 {
                lines.push(Line::from(vec![Span::styled(
                    format!("  ▲ {} more above", start_idx),
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            // Show visible meetings
            for (idx, meeting) in unlinked_meetings
                .iter()
                .enumerate()
                .skip(start_idx)
                .take(end_idx - start_idx)
            {
                let style = if idx == *selected_index {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else {
                    Style::default()
                };

                let untitled = "Untitled".to_string();
                let title = meeting.title.as_ref().unwrap_or(&untitled);

                // Truncate long titles to fit
                let display_title = if title.len() > 65 {
                    format!("{}...", &title[..62])
                } else {
                    title.clone()
                };

                lines.push(Line::from(vec![Span::styled(
                    format!("  {}", display_title),
                    style,
                )]));
            }

            // Show scroll indicator if not at bottom
            if end_idx < total {
                lines.push(Line::from(vec![Span::styled(
                    format!("  ▼ {} more below", total - end_idx),
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "[↑/↓]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Navigate  "),
                Span::styled(
                    "[L]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Link  "),
                Span::styled(
                    "[S]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Skip  "),
                Span::styled(
                    "[Esc]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Cancel"),
            ]));
        }
        WizardStep::CreatingMeetingWorklogs => {
            lines.push(Line::from("📅 Processing linked meetings..."));
        }
        WizardStep::CreatingGitHubWorklogs {
            ref sessions,
            current_session_index,
        } => {
            if sessions.is_empty() {
                lines.push(Line::from("💻 Looking for GitHub sessions..."));
            } else {
                lines.push(Line::from(format!(
                    "💻 Processing GitHub session {}/{}",
                    current_session_index + 1,
                    sessions.len()
                )));

                if let Some(session) = sessions.get(*current_session_index) {
                    let desc = session
                        .description
                        .split(';')
                        .next()
                        .unwrap_or("Development work");
                    let desc_short = if desc.len() > 60 {
                        format!("{}...", &desc[..57])
                    } else {
                        desc.to_string()
                    };
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("Session: ", Style::default().fg(Color::Cyan)),
                        Span::raw(desc_short),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("Duration: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{:.1}h", session.duration_seconds as f64 / 3600.0)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("Issues: ", Style::default().fg(Color::Cyan)),
                        Span::raw(session.get_jira_issues().join(", ")),
                    ]));
                }

                // If worklog_creation_confirmation exists, it will overlay with F/P/S options
            }
        }
        WizardStep::FillingGaps { .. } => {
            if let Some(state) = gap_fill_state {
                lines.push(Line::from("🔧 Select issue to fill remaining time:"));
                lines.push(Line::from(""));

                // Filter issues based on search query
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

                // Search box
                lines.push(Line::from(vec![
                    Span::styled("Search: ", Style::default().fg(Color::Cyan)),
                    Span::styled(&state.search_query, Style::default().fg(Color::White)),
                    Span::styled("█", Style::default().fg(Color::White)), // Cursor
                ]));
                lines.push(Line::from(""));

                // Show up to 8 issues
                let max_visible = 8;
                for (idx, issue) in filtered_issues.iter().enumerate().take(max_visible) {
                    let style = if idx == state.selected_issue_index {
                        Style::default().bg(Color::DarkGray).fg(Color::White)
                    } else {
                        Style::default()
                    };

                    let summary_max = 50;
                    let summary = if issue.summary.len() > summary_max {
                        format!("{}...", &issue.summary[..summary_max - 3])
                    } else {
                        issue.summary.clone()
                    };

                    lines.push(Line::from(vec![Span::styled(
                        format!("  {} - {}", issue.key, summary),
                        style,
                    )]));
                }

                if filtered_issues.len() > max_visible {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  ... and {} more", filtered_issues.len() - max_visible),
                        Style::default().fg(Color::DarkGray),
                    )]));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(
                        "[↑/↓]",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" Navigate  "),
                    Span::styled(
                        "[Enter]",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" Select  "),
                    Span::styled(
                        "[S]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" Skip  "),
                    Span::styled(
                        "[Esc]",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" Cancel"),
                ]));
            } else {
                lines.push(Line::from("🔧 Preparing gap fill..."));
            }
        }
        WizardStep::ReviewingWorklogs { .. } => {
            lines.push(Line::from("📋 Review worklogs before pushing:"));
            lines.push(Line::from(""));

            lines.push(Line::from(format!(
                "Worklogs ready to push: {}",
                wizard.summary.pushed_count
            )));
            lines.push(Line::from(""));
            lines.push(Line::from("Summary:"));
            lines.push(Line::from(format!(
                "  • Meetings linked (auto): {}",
                wizard.summary.meetings_auto_linked
            )));
            lines.push(Line::from(format!(
                "  • Meetings linked (manual): {}",
                wizard.summary.meetings_manually_linked
            )));
            lines.push(Line::from(format!(
                "  • Worklogs from meetings: {}",
                wizard.summary.worklogs_from_meetings
            )));
            lines.push(Line::from(format!(
                "  • Worklogs from GitHub: {}",
                wizard.summary.worklogs_from_github
            )));
            lines.push(Line::from(format!(
                "  • Worklogs from gaps: {}",
                wizard.summary.worklogs_from_gaps
            )));
            lines.push(Line::from(format!(
                "  • Total hours: {:.1}h",
                wizard.summary.total_hours
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "[P]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Push to Jira  "),
                Span::styled(
                    "[Esc]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Cancel wizard"),
            ]));
        }
        WizardStep::Pushing => {
            lines.push(Line::from(vec![Span::styled(
                "⏳ Uploading worklogs to Jira...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(""));

            // Show recent push logs
            if wizard.push_logs.is_empty() {
                lines.push(Line::from("Please wait, this may take a moment..."));
            } else {
                lines.push(Line::from(vec![Span::styled(
                    "Recent activity:",
                    Style::default().fg(Color::Cyan),
                )]));
                lines.push(Line::from(""));
                for log in &wizard.push_logs {
                    lines.push(Line::from(format!("  {}", log)));
                }
            }

            // Add space for progress bar at bottom
            lines.push(Line::from(""));
            lines.push(Line::from("")); // Space for gauge widget
        }
        WizardStep::Complete => {
            lines.push(Line::from(vec![Span::styled(
                "✅ 🧙 Chronie's Work is Done!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Your time is tracked! Here's what we did:",
                Style::default().fg(Color::Cyan),
            )]));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "  • Auto-linked meetings: {}",
                wizard.summary.meetings_auto_linked
            )));
            lines.push(Line::from(format!(
                "  • Manually-linked meetings: {}",
                wizard.summary.meetings_manually_linked
            )));
            lines.push(Line::from(format!(
                "  • Worklogs from meetings: {}",
                wizard.summary.worklogs_from_meetings
            )));
            lines.push(Line::from(format!(
                "  • Worklogs from GitHub: {}",
                wizard.summary.worklogs_from_github
            )));
            lines.push(Line::from(format!(
                "  • Worklogs from gaps: {}",
                wizard.summary.worklogs_from_gaps
            )));
            lines.push(Line::from(format!(
                "  • Total hours logged: {:.1}h",
                wizard.summary.total_hours
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "[Any Key]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Close wizard"),
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);

    // Render progress bar for Pushing step
    if matches!(wizard.current_step, WizardStep::Pushing) && wizard.push_total > 0 {
        let percentage =
            (wizard.push_current as f64 / wizard.push_total as f64 * 100.0).min(100.0) as u16;

        // Position gauge at bottom of popup (3 lines from bottom)
        let gauge_area = Rect {
            x: popup_area.x + 3,
            y: popup_area.y + popup_area.height.saturating_sub(3),
            width: popup_area.width.saturating_sub(6),
            height: 1,
        };

        let progress_label = format!(
            "{}/{} worklogs ({}%)",
            wizard.push_current, wizard.push_total, percentage
        );

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
            .percent(percentage)
            .label(progress_label);

        frame.render_widget(gauge, gauge_area);
    }
}

pub(in crate::tui) fn matches_current_step(step_num: usize, current_step: &WizardStep) -> bool {
    use WizardStep;
    match (step_num, current_step) {
        (1, WizardStep::Syncing) => true,
        (2, WizardStep::AutoLinking) | (2, WizardStep::ManualLinking { .. }) => true,
        (3, WizardStep::CreatingMeetingWorklogs) => true,
        (4, WizardStep::CreatingGitHubWorklogs { .. }) => true,
        (5, WizardStep::FillingGaps { .. }) => true,
        (6, WizardStep::ReviewingWorklogs { .. }) => true,
        (7, WizardStep::Pushing) => true,
        (8, WizardStep::Complete) => true,
        _ => false,
    }
}

pub(in crate::tui) fn render_wizard_cancel_confirmation(frame: &mut Frame) {
    let area = frame.area();
    let popup_width = 65.min(area.width.saturating_sub(4));
    let popup_height = 11;
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(vec![Span::styled(
            "⚠️  Cancel Chronie's Wizard?",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from("This will rollback all changes:"),
        Line::from("  • Unlink all linked meetings"),
        Line::from("  • Delete all created worklogs"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[Y]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" to cancel wizard, "),
            Span::styled(
                "[N]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" to continue"),
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

