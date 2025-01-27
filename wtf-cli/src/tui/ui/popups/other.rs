use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::SprintFollowState;
use crate::tui::theme::theme;

pub(in crate::tui) fn render_sprint_follow_popup(frame: &mut Frame, state: &SprintFollowState) {
    let area = frame.area();
    let popup_width = 80.min(area.width.saturating_sub(4));
    let popup_height = 25.min(area.height.saturating_sub(4));
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    // Filter sprints based on search query
    let filtered_sprints: Vec<&wtf_lib::models::data::Sprint> = state
        .all_sprints
        .iter()
        .filter(|sprint| {
            if state.search_query.is_empty() {
                true
            } else {
                let query_lower = state.search_query.to_lowercase();
                sprint.name.to_lowercase().contains(&query_lower)
                    || format!("{}", sprint.id).contains(&query_lower)
            }
        })
        .collect();

    let mut lines = vec![];

    // Title
    lines.push(Line::from(vec![Span::styled(
        format!(
            "📌 Follow/Unfollow Sprints ({} total)",
            state.all_sprints.len()
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Search box
    lines.push(Line::from(vec![
        Span::styled("Search: ", Style::default().fg(Color::Cyan)),
        Span::styled(&state.search_query, Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::White)), // Cursor
    ]));
    lines.push(Line::from(""));

    // Sprint list (show up to 12 sprints)
    let max_visible = 12;
    let start_index = if state.selected_index >= max_visible {
        state.selected_index - max_visible + 1
    } else {
        0
    };

    if filtered_sprints.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  No sprints found",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for (idx, sprint) in filtered_sprints
            .iter()
            .enumerate()
            .skip(start_index)
            .take(max_visible)
        {
            let is_selected = idx == state.selected_index;
            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            let checkbox = if sprint.followed { "✓" } else { " " };
            let state_icon = match sprint.state {
                wtf_lib::models::data::SprintState::Active => "●",
                wtf_lib::models::data::SprintState::Future => "◯",
                wtf_lib::models::data::SprintState::Closed => "✓",
            };

            // Format dates
            let date_str = if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                format!("{} → {}", start.format("%d/%m"), end.format("%d/%m"))
            } else {
                "No dates".to_string()
            };

            let name_max = 35;
            let name = if sprint.name.len() > name_max {
                format!("{}...", &sprint.name[..name_max - 3])
            } else {
                sprint.name.clone()
            };

            lines.push(Line::from(vec![Span::styled(
                format!("  [{}] {} {:<37} {}", checkbox, state_icon, name, date_str),
                style,
            )]));
        }

        // Show scroll indicators
        if start_index > 0 {
            lines.insert(
                5,
                Line::from(vec![Span::styled(
                    "  ▲ More above",
                    Style::default().fg(Color::DarkGray),
                )]),
            );
        }
        if start_index + max_visible < filtered_sprints.len() {
            lines.push(Line::from(vec![Span::styled(
                "  ▼ More below",
                Style::default().fg(Color::DarkGray),
            )]));
        }
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
            "[A/Enter]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Toggle follow  "),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Close"),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(theme().bg_primary));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}

pub(in crate::tui) fn render_about_popup(
    frame: &mut Frame,
    about_image: &Option<image::DynamicImage>,
) {
    let area = frame.area();
    let popup_width = 120.min(area.width.saturating_sub(4));
    let popup_height = 28.min(area.height.saturating_sub(4));
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    // First render the border on the full popup area
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme().border))
            .style(Style::default().bg(Color::Rgb(18, 22, 23)))
            .title(" About "),
        popup_area,
    );

    // Create side-by-side layout: logo on left, text on right
    let inner_area = Rect {
        x: popup_area.x + 2,
        y: popup_area.y + 2,
        width: popup_area.width.saturating_sub(4),
        height: popup_area.height.saturating_sub(3),
    };

    // Render the logo image if available on the left side
    if let Some(img) = about_image {
        // Logo area on the left with minimal padding
        let logo_width = 44;
        let left_padding = 1;
        let logo_area = Rect {
            x: inner_area.x + left_padding,
            y: inner_area.y,
            width: logo_width,
            height: 20.min(inner_area.height),
        };

        // Use the best available protocol for highest resolution
        use ratatui_image::picker::Picker;
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let mut dyn_img = picker.new_resize_protocol(img.clone());
        let image_widget = ratatui_image::StatefulImage::new();
        frame.render_stateful_widget(image_widget, logo_area, &mut dyn_img);
    }

    // Text area on the right side - optimized spacing
    // Layout: 1 (left pad) + 44 (logo) + 2 (gap) = 47
    let text_x_offset = if about_image.is_some() { 47 } else { 0 };
    let text_area = Rect {
        x: inner_area.x + text_x_offset,
        y: inner_area.y,
        width: inner_area.width.saturating_sub(text_x_offset),
        height: inner_area.height,
    };

    let mut lines = vec![];

    lines.extend(vec![
        Line::from(vec![Span::styled(
            "🧙 WTF - Worklog Time Filler",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "Version 0.1.0",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Powered by Chronie, the Chronurgist",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::ITALIC),
        )]),
        Line::from(vec![Span::styled(
            "Master of time, slayer of temporal anomalies",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(""),
        Line::from("A powerful TUI for managing Jira worklogs, integrating"),
        Line::from("meetings, and tracking time with magical ease."),
        Line::from(""),
        Line::from(vec![Span::styled(
            "✨ Features:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  • Sprint Management with visual progress"),
        Line::from("  • Google Calendar meeting integration"),
        Line::from("  • Worklog staging and batch operations"),
        Line::from("  • GitHub session tracking"),
        Line::from("  • Smart time gap filling"),
        Line::from("  • Chronie's Wizard for guided workflows"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "🎯 Quick Help:",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  • 1-5: Switch tabs"),
        Line::from("  • W: Launch Chronie wizard (Sprints tab)"),
        Line::from("  • R: Refresh data"),
        Line::from("  • H: Show this about screen"),
        Line::from("  • Q: Quit"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[H]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" or "),
            Span::styled(
                "[Esc]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" to close"),
        ]),
    ]);

    // Then render text content in the text area (left-aligned for side-by-side layout)
    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, text_area);
}
