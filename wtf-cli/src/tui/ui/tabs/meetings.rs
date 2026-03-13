use crossterm::event::{KeyCode, KeyEvent};
use super::settings::GC_TERM_COLORS;
use chrono::Local;
use ratatui::{
    layout::{Alignment, Rect},
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
use crate::tui::Tui;
use wtf_lib::config::Config;
use wtf_lib::models::data::Meeting;
use wtf_lib::services::meetings_service::MeetingsService;

#[derive(Debug, Clone, Copy, Default)]
pub struct MeetingsTab;

pub(in crate::tui) fn visible_meetings(data: &TuiData) -> Vec<Meeting> {
    let mut sorted_meetings = data.all_meetings.clone();
    sorted_meetings.sort_by(|a, b| b.start.cmp(&a.start));

    if data.ui_state.filter_unlinked_only {
        sorted_meetings
            .into_iter()
            .filter(|meeting| {
                let is_unlinked = meeting.jira_link.is_none();
                let is_not_declined = meeting
                    .my_response_status
                    .as_ref()
                    .map(|status| status != "declined")
                    .unwrap_or(true);
                let is_not_untracked = !wtf_lib::utils::meetings::is_untracked(
                    meeting,
                    &data.config,
                    &data.untracked_meeting_ids,
                );

                is_unlinked && is_not_declined && is_not_untracked
            })
            .collect()
    } else {
        sorted_meetings
    }
}

impl TabController for MeetingsTab {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData) {
        render_meetings_tab(frame, area, data);
    }

    fn handle_key(&self, tui: &mut Tui, key: KeyEvent) {
        let meetings = visible_meetings(&tui.data);
        let max_index = meetings.len().saturating_sub(1);

        if helpers::handle_list_navigation(
            key,
            &mut tui.data.ui_state.selected_meeting_index,
            max_index,
        ) {
            return;
        }

        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                tui.refresh_data();
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                tui.handle_update();
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                tui.auto_link_meetings();
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                tui.handle_meeting_log();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                tui.data.ui_state.filter_unlinked_only = !tui.data.ui_state.filter_unlinked_only;
                tui.data.ui_state.selected_meeting_index = 0;
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(meeting) = meetings.get(tui.data.ui_state.selected_meeting_index) {
                    if meeting.jira_link.is_some() {
                        tui.unlink_confirmation_meeting_id = Some(meeting.id.clone());
                    }
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                if let Some(meeting) = meetings.get(tui.data.ui_state.selected_meeting_index) {
                    let now_untracked = MeetingsService::production().toggle_untracked(&meeting.id);
                    if now_untracked {
                        logger::log("🚫 Meeting marked as untracked".to_string());
                    } else {
                        logger::log("✅ Meeting unmarked as untracked".to_string());
                    }
                    tui.refresh_data();
                }
            }
            KeyCode::Enter => {
                if let Some(meeting) = meetings.get(tui.data.ui_state.selected_meeting_index) {
                    tui.link_meeting(meeting.id.clone());
                }
            }
            KeyCode::PageUp => {
                tui.data.ui_state.selected_meeting_index =
                    tui.data.ui_state.selected_meeting_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                tui.data.ui_state.selected_meeting_index =
                    (tui.data.ui_state.selected_meeting_index + 10).min(max_index);
            }
            _ => {}
        }
    }
}

/// Render meetings tab with list and details
pub(in crate::tui) fn render_meetings_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let selected_index = data.ui_state.selected_meeting_index;
    let meetings = visible_meetings(data);

    render_list_detail_layout(
        frame,
        area,
        |f, a| render_meetings_list(f, a, data, &meetings, selected_index),
        |f, a| render_meeting_details(f, a, data, &meetings, selected_index),
    );
}

fn render_meetings_list(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    meetings: &[Meeting],
    selected_index: usize,
) {
    let mut lines = vec![];

    if meetings.is_empty() {
        let message = if data.ui_state.filter_unlinked_only {
            "No unlinked meetings found"
        } else {
            "No meetings found"
        };
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(message, Style::default().fg(Color::DarkGray)),
        ]));
    } else {
        // Calculate visible window (consider block borders: area.height - 2)
        let visible_height = area.height.saturating_sub(2) as usize;
        let total_meetings = meetings.len();

        // Calculate scroll position to keep selected item visible
        let scroll_offset = if selected_index < visible_height / 2 {
            0
        } else if selected_index >= total_meetings.saturating_sub(visible_height / 2) {
            total_meetings.saturating_sub(visible_height)
        } else {
            selected_index.saturating_sub(visible_height / 2)
        };

        // Render visible meetings
        let visible_meetings = meetings
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height);

        for (idx, meeting) in visible_meetings {
            let local_start = meeting.start.with_timezone(&Local);
            let local_end = meeting.end.with_timezone(&Local);
            let is_selected = idx == selected_index;

            // Date and time
            let date_str = local_start.format("%d %b").to_string();
            let time_str = format!(
                "{}-{}",
                local_start.format("%H:%M"),
                local_end.format("%H:%M")
            );

            // Title - truncate based on available width
            let title = meeting
                .title
                .as_ref()
                .map(|t| truncate_string(t, 70))
                .unwrap_or_else(|| "No title".to_string());

            // Check if meeting is declined
            let is_declined = meeting
                .my_response_status
                .as_ref()
                .map(|s| s == "declined")
                .unwrap_or(false);

            // Check if meeting is untracked
            let is_untracked = wtf_lib::utils::meetings::is_untracked(
                meeting,
                &data.config,
                &data.untracked_meeting_ids,
            );

            // Jira link or status
            let link_text = if let Some(ref jira_link) = meeting.jira_link {
                truncate_string(jira_link, 15)
            } else {
                "—".to_string()
            };

            let link_color = if meeting.jira_link.is_some() {
                Color::Blue
            } else {
                Color::DarkGray
            };

            // Selection indicator and style
            let (indicator, mut base_style) = if is_selected {
                (
                    theme().selector,
                    Style::default().add_modifier(Modifier::BOLD),
                )
            } else {
                (theme().unselected_selector, Style::default())
            };

            // Apply grey and strikethrough for declined meetings
            if is_declined {
                base_style = base_style
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT);
            } else if is_untracked {
                base_style = base_style.fg(Color::DarkGray);
            }

            // Build the compact line: indicator + date + time + title + link
            lines.push(Line::from(vec![
                Span::styled(
                    indicator,
                    if is_declined || is_untracked {
                        base_style
                    } else {
                        base_style.fg(Color::Yellow)
                    },
                ),
                Span::raw(" "),
                Span::raw(" "),
                Span::styled(
                    date_str,
                    if is_declined || is_untracked {
                        base_style
                    } else {
                        base_style.fg(Color::Cyan)
                    },
                ),
                Span::raw(" "),
                Span::styled(time_str, base_style.fg(Color::DarkGray)),
                Span::raw("  "),
                // Colored circle for meetings with a Google Calendar color
                {
                    let circle_color = meeting
                        .color_id
                        .as_deref()
                        .and_then(|cid| cid.parse::<usize>().ok())
                        .filter(|&idx| idx >= 1 && idx <= 11)
                        .map(|idx| {
                            if is_declined || is_untracked {
                                Color::DarkGray
                            } else {
                                GC_TERM_COLORS[idx - 1]
                            }
                        });
                    if let Some(c) = circle_color {
                        Span::styled("● ", Style::default().fg(c))
                    } else {
                        Span::raw("  ")
                    }
                },
                Span::styled(
                    title,
                    if is_declined || is_untracked {
                        base_style
                    } else {
                        base_style.fg(Color::White)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", link_text),
                    if is_declined || is_untracked {
                        base_style
                    } else {
                        base_style.fg(link_color)
                    },
                ),
            ]));
        }
    }

    // Check if selected meeting has a link (to show contextual help)
    let selected_has_link = meetings
        .get(selected_index)
        .and_then(|m| m.jira_link.as_ref())
        .is_some();

    let pending_count = data.meeting_stats.pending;
    let filter_text = if data.ui_state.filter_unlinked_only {
        " [FILTERED: Unlinked Only]"
    } else {
        ""
    };

    // Build contextual help text
    let mut shortcuts_data = vec![("F", "ilter"), ("A", "uto-link"), ("X", " Untrack")];
    if selected_has_link {
        shortcuts_data.push(("Del", " Unlink"));
    }
    shortcuts_data.push(("Enter", " Link"));
    shortcuts_data.push(("L", "og"));
    let shortcuts = build_shortcut_help(&shortcuts_data);

    let mut title_spans = vec![Span::raw("📅 Meetings (")];
    if pending_count > 0 {
        title_spans.push(Span::raw(format!(
            "{} total, {} pending",
            data.all_meetings.len(),
            pending_count
        )));
    } else {
        title_spans.push(Span::raw(format!("{} total", data.all_meetings.len())));
    }
    title_spans.push(Span::raw(filter_text));
    title_spans.push(Span::raw(") | "));
    title_spans.extend(shortcuts);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(title_spans))
        .title_alignment(Alignment::Left)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, *area);
}

fn render_meeting_details(
    frame: &mut Frame,
    area: &Rect,
    _data: &TuiData,
    meetings: &[Meeting],
    selected_index: usize,
) {
    use chrono::Local;

    let block = Block::default()
        .borders(Borders::ALL)
        .title("📋 Meeting Details")
        .title_alignment(Alignment::Left)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    if meetings.is_empty() || selected_index >= meetings.len() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No meeting selected",
                Style::default().fg(Color::Gray),
            )),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let meeting = &meetings[selected_index];
    let local_start = meeting.start.with_timezone(&Local);
    let local_end = meeting.end.with_timezone(&Local);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "Title: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                meeting.title.as_deref().unwrap_or("Untitled"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "Start: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                local_start.format("%Y-%m-%d %H:%M").to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "End:   ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                local_end.format("%Y-%m-%d %H:%M").to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    // Duration
    let duration = meeting.end.signed_duration_since(meeting.start);
    let hours = duration.num_hours();
    let minutes = duration.num_minutes() % 60;
    lines.push(Line::from(vec![
        Span::styled(
            "Duration: ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}h {:02}m", hours, minutes),
            Style::default().fg(Color::Cyan),
        ),
    ]));
    lines.push(Line::from(""));

    // Response Status
    if let Some(status) = &meeting.my_response_status {
        let (status_text, status_color) = match status.as_str() {
            "accepted" => ("✓ Accepted", Color::Green),
            "declined" => ("✗ Declined", Color::Red),
            "tentative" => ("? Tentative", Color::Yellow),
            "needsAction" => ("⏳ Needs Action", Color::Cyan),
            _ => (status.as_str(), Color::Gray),
        };
        lines.push(Line::from(vec![
            Span::styled(
                "Response: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]));
    }

    // Color label
    if let Some(color_id) = &meeting.color_id {
        let (label, color) = match color_id.as_str() {
            "1" => ("Lavender", Color::Rgb(121, 134, 203)),
            "2" => ("Sage", Color::Rgb(51, 182, 121)),
            "3" => ("Grape", Color::Rgb(142, 36, 170)),
            "4" => ("Flamingo", Color::Rgb(229, 57, 53)),
            "5" => ("Banana", Color::Rgb(240, 185, 0)),
            "6" => ("Tangerine", Color::Rgb(246, 109, 13)),
            "7" => ("Peacock", Color::Rgb(3, 155, 229)),
            "8" => ("Graphite", Color::Rgb(97, 97, 97)),
            "9" => ("Blueberry", Color::Rgb(63, 81, 181)),
            "10" => ("Basil", Color::Rgb(11, 128, 67)),
            "11" => ("Tomato", Color::Rgb(213, 0, 0)),
            _ => ("Custom", Color::Gray),
        };
        lines.push(Line::from(vec![
            Span::styled(
                "Color: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("● ({})", label), Style::default().fg(color)),
        ]));
    }

    // Recurrence info
    if let Some(recurrence) = &meeting.recurrence {
        if !recurrence.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(
                    "Recurrence: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("🔁 Recurring", Style::default().fg(Color::Magenta)),
            ]));
        }
    }
    lines.push(Line::from(""));

    // Jira Link - with full URL for clickability
    if let Some(link) = &meeting.jira_link {
        // Build full URL for terminal click support
        let full_url = if let Ok(config) = Config::load() {
            format!(
                "{}/browse/{}",
                config.jira.base_url.trim_end_matches('/'),
                link
            )
        } else {
            link.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(
                "Jira: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(link, Style::default().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled(
                full_url,
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                "Jira: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Not linked", Style::default().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::from(""));

    // Description
    if let Some(desc) = &meeting.description {
        if !desc.trim().is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "Description:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));

            // Parse description to extract links and build styled text
            let parsed = parse_description(desc);

            let mut desc_text_parts = Vec::new();
            let mut link_refs = Vec::new();

            for (text, url) in parsed {
                if let Some(url) = url {
                    // This is a link - add the text
                    desc_text_parts.push(text.clone());
                    link_refs.push((text, url));
                } else {
                    // Regular text - parse HTML formatting
                    desc_text_parts.push(text);
                }
            }

            // Join all text parts
            let full_text = desc_text_parts.join("");

            // Parse HTML styling from the full text
            let styled_segments = parse_html_styled_text(&full_text);

            // Build continuous text from segments for wrapping
            let plain_text: String = styled_segments.iter().map(|s| s.text.as_str()).collect();

            // Wrap text to fit in panel
            let max_width = 50;
            let wrapped_lines = wrap_text(&plain_text, max_width);

            // Render wrapped lines (for now without styling - we'll apply styling per segment later)
            // TODO: Preserve styling across wrapped lines
            for line in wrapped_lines.iter().take(8) {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(line.clone(), Style::default().fg(Color::White)),
                ]));
            }
            if wrapped_lines.len() > 8 {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "... (truncated)".to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            // Show links with full URLs below description
            if !link_refs.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "  Links:",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                for (text, url) in link_refs.iter().take(3) {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(text.clone(), Style::default().fg(Color::Cyan)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::raw("    → "),
                        Span::styled(
                            url.clone(),
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::UNDERLINED),
                        ),
                    ]));
                }
                if link_refs.len() > 3 {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            format!("... ({} more links)", link_refs.len() - 3),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
            }

            lines.push(Line::from(""));
        }
    }

    // Attendees
    if let Some(attendees) = &meeting.attendees {
        if !attendees.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "Attendees:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
            for attendee in attendees.iter().take(10) {
                // Increased from 5 to 10
                let name = attendee
                    .display_name
                    .as_deref()
                    .or(attendee.email.as_deref())
                    .unwrap_or("Unknown");
                lines.push(Line::from(vec![
                    Span::raw("  • "),
                    Span::styled(name, Style::default().fg(Color::Gray)),
                ]));
            }
            if attendees.len() > 10 {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("... ({} more)", attendees.len() - 10),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}
