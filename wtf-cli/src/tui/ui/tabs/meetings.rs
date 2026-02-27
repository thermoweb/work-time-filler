use chrono::Local;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::ui_helpers::*;
use crate::tui::theme::theme;
use wtf_lib::config::Config;

/// Render meetings tab with list and details
pub(in crate::tui) fn render_meetings_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let selected_index = data.ui_state.selected_meeting_index;
    let filter_unlinked_only = data.ui_state.filter_unlinked_only;
    
    render_list_detail_layout(
        frame,
        area,
        |f, a| render_meetings_list(f, a, data, selected_index, filter_unlinked_only),
        |f, a| render_meeting_details(f, a, data, selected_index, filter_unlinked_only),
    );
}

fn render_meetings_list(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
    filter_unlinked_only: bool,
) {
    let mut lines = vec![];

    // Sort meetings by date (most recent first)
    let mut sorted_meetings = data.all_meetings.clone();
    sorted_meetings.sort_by(|a, b| b.start.cmp(&a.start));

    // Apply filter if needed
    let filtered_meetings: Vec<_> = if filter_unlinked_only {
        sorted_meetings
            .into_iter()
            .filter(|m| {
                // Filter out linked meetings
                let is_unlinked = m.jira_link.is_none();
                // Filter out declined meetings
                let is_not_declined = m
                    .my_response_status
                    .as_ref()
                    .map(|s| s != "declined")
                    .unwrap_or(true);
                is_unlinked && is_not_declined
            })
            .collect()
    } else {
        sorted_meetings
    };

    if filtered_meetings.is_empty() {
        let message = if filter_unlinked_only {
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
        let total_meetings = filtered_meetings.len();

        // Calculate scroll position to keep selected item visible
        let scroll_offset = if selected_index < visible_height / 2 {
            0
        } else if selected_index >= total_meetings.saturating_sub(visible_height / 2) {
            total_meetings.saturating_sub(visible_height)
        } else {
            selected_index.saturating_sub(visible_height / 2)
        };

        // Render visible meetings
        let visible_meetings = filtered_meetings
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
                (theme().selector, Style::default().add_modifier(Modifier::BOLD))
            } else {
                (theme().unselected_selector, Style::default())
            };

            // Apply grey and strikethrough for declined meetings
            if is_declined {
                base_style = base_style
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT);
            }

            // Build the compact line: indicator + date + time + title + link
            lines.push(Line::from(vec![
                Span::styled(
                    indicator,
                    if is_declined {
                        base_style
                    } else {
                        base_style.fg(Color::Yellow)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    date_str,
                    if is_declined {
                        base_style
                    } else {
                        base_style.fg(Color::Cyan)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    time_str,
                    if is_declined {
                        base_style
                    } else {
                        base_style.fg(Color::DarkGray)
                    },
                ),
                Span::raw("  "),
                Span::styled(
                    title,
                    if is_declined {
                        base_style
                    } else {
                        base_style.fg(Color::White)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", link_text),
                    if is_declined {
                        base_style
                    } else {
                        base_style.fg(link_color)
                    },
                ),
            ]));
        }
    }

    // Check if selected meeting has a link (to show contextual help)
    let selected_has_link = filtered_meetings
        .get(selected_index)
        .and_then(|m| m.jira_link.as_ref())
        .is_some();

    let pending_count = data.meeting_stats.pending;
    let filter_text = if filter_unlinked_only {
        " [FILTERED: Unlinked Only]"
    } else {
        ""
    };

    // Build contextual help text
    let mut shortcuts_data = vec![("F", "ilter"), ("A", "uto-link")];
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
    data: &TuiData,
    selected_index: usize,
    filter_unlinked_only: bool,
) {
    use chrono::{Datelike, Timelike};

    // Sort and filter meetings the same way as the list
    let mut sorted_meetings = data.all_meetings.clone();
    sorted_meetings.sort_by(|a, b| b.start.cmp(&a.start));

    let filtered_meetings: Vec<_> = if filter_unlinked_only {
        sorted_meetings
            .into_iter()
            .filter(|m| {
                // Filter out linked meetings
                let is_unlinked = m.jira_link.is_none();
                // Filter out declined meetings
                let is_not_declined = m
                    .my_response_status
                    .as_ref()
                    .map(|s| s != "declined")
                    .unwrap_or(true);
                is_unlinked && is_not_declined
            })
            .collect()
    } else {
        sorted_meetings
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("📋 Meeting Details")
        .title_alignment(Alignment::Left)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    if filtered_meetings.is_empty() || selected_index >= filtered_meetings.len() {
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

    let meeting = &filtered_meetings[selected_index];

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
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    meeting.start.year(),
                    meeting.start.month(),
                    meeting.start.day(),
                    meeting.start.hour(),
                    meeting.start.minute()
                ),
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
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    meeting.end.year(),
                    meeting.end.month(),
                    meeting.end.day(),
                    meeting.end.hour(),
                    meeting.end.minute()
                ),
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
