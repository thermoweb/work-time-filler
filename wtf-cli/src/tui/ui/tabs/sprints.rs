use chrono::Local;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::ui_helpers::*;
use crate::tui::theme::theme;
use wtf_lib::models::data::{Sprint, SprintState};

fn render_worklog_wall(frame: &mut Frame, area: &Rect, data: &TuiData) {
    use chrono::Datelike;

    let block = Block::default()
        .title("📈 Worklog Activity (Last Year)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    if data.worklog_wall.is_empty() {
        return;
    }

    // Build grid with year boundaries respected
    // When year changes, we pad current column, add separator, then continue from same weekday
    let mut grid: Vec<Vec<(f64, bool, bool)>> = vec![vec![]; 5]; // (hours, is_absence, is_separator)
    let mut prev_year: Option<i32> = None;

    for (day_index, activity) in data.worklog_wall.iter().enumerate() {
        let weekday = day_index % 7;

        // Only process Mon-Fri (weekdays 0-4)
        if weekday < 5 {
            let current_year = activity.date.year();

            // Check for year change
            if let Some(prev) = prev_year {
                if current_year != prev {
                    // Year changed! Pad all rows to same length
                    let max_len = grid.iter().map(|row| row.len()).max().unwrap_or(0);
                    for row in &mut grid {
                        while row.len() < max_len {
                            row.push((0.0, false, false));
                        }
                    }

                    // Add separator column to all rows
                    for row in &mut grid {
                        row.push((0.0, false, true)); // Special marker for separator
                    }

                    // Pad rows BEFORE the current weekday to align properly
                    // If year changes on Thursday, pad Mon-Wed with empty cells
                    let max_len_after_sep = grid.iter().map(|row| row.len()).max().unwrap_or(0);
                    for row_idx in 0..weekday {
                        while grid[row_idx].len() < max_len_after_sep + 1 {
                            grid[row_idx].push((0.0, false, false));
                        }
                    }
                }
            }
            prev_year = Some(current_year);

            // Add data to grid at correct weekday row
            grid[weekday].push((activity.hours, activity.is_absence, false));
        }
    }

    // Pad all rows to same length
    let max_len = grid.iter().map(|row| row.len()).max().unwrap_or(0);
    for row in &mut grid {
        while row.len() < max_len {
            row.push((0.0, false, false));
        }
    }

    // Render the grid with weekday labels (Mon-Fri only)
    let mut lines = Vec::new();
    let weekday_labels = ["Mon", "Tue", "Wed", "Thu", "Fri"];

    for (weekday_idx, weekday_row) in grid.iter().enumerate() {
        let mut line_spans = vec![Span::styled(
            format!("{} ", weekday_labels[weekday_idx]),
            Style::default().fg(Color::DarkGray),
        )];

        for &(hours, is_absence, is_separator) in weekday_row {
            if is_separator {
                // Draw vertical separator
                line_spans.push(Span::styled("│", Style::default().fg(Color::Blue)));
            } else {
                let braille = hours_to_braille(hours, data.daily_hours_limit);
                let color = if is_absence {
                    Color::DarkGray // Absences in dark gray
                } else {
                    Color::Green // Imputations in green
                };
                line_spans.push(Span::styled(braille, Style::default().fg(color)));
            }
        }

        lines.push(Line::from(line_spans));
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

/// Convert hours to braille character based on density
fn hours_to_braille(hours: f64, daily_limit: f64) -> &'static str {
    if hours == 0.0 {
        "⠀" // Empty
    } else if hours >= daily_limit {
        "⣿" // Full square for >= daily limit (100%)
    } else {
        let percentage = (hours / daily_limit * 100.0).min(100.0);
        match percentage as u32 {
            0..=12 => "⢀",  // 1 dot (minimum for any activity)
            13..=25 => "⢠", // 2 dots
            26..=37 => "⢰", // 3 dots
            38..=50 => "⢸", // 4 dots left
            51..=62 => "⣀", // 4 dots bottom
            63..=75 => "⣠", // 5 dots
            76..=87 => "⣰", // 6 dots
            _ => "⣸",       // 7 dots
        }
    }
}

/// Convert hours to color

/// Sprints tab - Split view with details and activity
pub(in crate::tui) fn render_sprints_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let selected_index = data.ui_state.selected_sprint_index;
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // Top: List + Details (more space now)
            Constraint::Length(8), // Worklog wall (5 rows + borders)
        ])
        .split(*area);

    // Split top area into list and details (60/40)
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Sprint list
            Constraint::Percentage(40), // Sprint details
        ])
        .split(chunks[0]);

    render_sprint_list_expanded(frame, &top_chunks[0], data, selected_index);

    if let Some(sprint) = data.all_sprints.get(selected_index) {
        render_sprint_details_with_activity(frame, &top_chunks[1], sprint, data);
    }

    // Worklog wall
    render_worklog_wall(frame, &chunks[1], data);
}

fn render_sprint_list_expanded(
    frame: &mut Frame,
    area: &Rect,
    data: &TuiData,
    selected_index: usize,
) {
    let shortcuts = build_shortcut_help(&[("W", "izard"), ("A", "dd/follow"), ("F", "ill")]);
    let mut title_spans = vec![
        Span::raw("📊 Followed Sprints ("),
        Span::raw(data.all_sprints.len().to_string()),
        Span::raw(") | "),
    ];
    title_spans.extend(shortcuts);

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    if data.all_sprints.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No sprints followed",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from("Tip: Run 'wtf fetch all' to fetch sprints"),
        ];
        let paragraph = Paragraph::new(content).alignment(Alignment::Left);
        frame.render_widget(paragraph, inner);
        return;
    }

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    for (i, sprint) in data.all_sprints.iter().enumerate() {
        let is_selected = i == selected_index;

        let icon = match sprint.state {
            SprintState::Active => "🟢",
            SprintState::Future => "🔵",
            SprintState::Closed => "⚫",
        };

        let capacity_hours = sprint.workdays as f64 * data.daily_hours_limit;
        let logged_hours = calculate_sprint_logged_hours(sprint.id, data);
        let percentage = if capacity_hours > 0.0 {
            (logged_hours / capacity_hours * 100.0).min(100.0) as u16
        } else {
            0
        };

        let status_text = if sprint.state == SprintState::Active {
            if let Some(end) = sprint.end {
                let today = Local::now().date_naive();
                let end_date = end.date_naive();
                let days_left = (end_date - today).num_days();
                if days_left > 0 {
                    format!("{} days left", days_left)
                } else if days_left == 0 {
                    "Last day!".to_string()
                } else {
                    "Ended".to_string()
                }
            } else {
                "Active".to_string()
            }
        } else if sprint.state == SprintState::Future {
            "Future".to_string()
        } else {
            "Closed".to_string()
        };

        let indicator = if is_selected { theme().selector } else { theme().unselected_selector };
        let base_style = if is_selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        // Create progress bar (10 blocks)
        let filled_blocks = ((percentage as f64 / 10.0).round() as usize).min(10);
        let progress_bar = format!(
            "{}{}",
            "█".repeat(filled_blocks),
            "░".repeat(10 - filled_blocks)
        );

        // Color based on percentage
        let percentage_color = if percentage >= 80 {
            Color::Green
        } else if percentage >= 50 {
            Color::Yellow
        } else {
            Color::Red
        };

        let line = Line::from(vec![
            Span::styled(indicator, base_style.fg(Color::Yellow)),
            Span::raw(" "),
            Span::raw(format!("{} ", icon)),
            Span::styled(
                format!("{:<24}", truncate_string(&sprint.name, 24)),
                base_style.fg(Color::White),
            ),
            Span::raw(" "),
            Span::styled(progress_bar, base_style.fg(percentage_color)),
            Span::raw(" "),
            Span::styled(
                format!("{:>3}%", percentage),
                base_style.fg(percentage_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:>5.1}h/{:<4.0}h", logged_hours, capacity_hours),
                base_style.fg(Color::DarkGray),
            ),
            Span::raw(" "),
            Span::styled(
                status_text,
                base_style.fg(match sprint.state {
                    SprintState::Active => Color::Green,
                    SprintState::Future => Color::Blue,
                    SprintState::Closed => Color::Gray,
                }),
            ),
        ]);

        lines.push(line);
    }

    lines.push(Line::from(""));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn render_sprint_details_with_activity(
    frame: &mut Frame,
    area: &Rect,
    sprint: &Sprint,
    data: &TuiData,
) {
    // Split vertically: details on top, activity on bottom
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(14), // Sprint details
            Constraint::Min(5),     // Activity graph
        ])
        .split(*area);

    render_sprint_details(frame, &chunks[0], sprint, data);
    render_sprint_activity_compact(frame, &chunks[1], sprint, data);
}

fn render_sprint_details(frame: &mut Frame, area: &Rect, sprint: &Sprint, data: &TuiData) {
    let block = Block::default()
        .title("🏃 Sprint Details")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    let capacity_hours = sprint.workdays as f64 * data.daily_hours_limit;
    let logged_hours = calculate_sprint_logged_hours(sprint.id, data);
    let remaining_hours = (capacity_hours - logged_hours).max(0.0);
    let percentage = if capacity_hours > 0.0 {
        ((logged_hours / capacity_hours) * 100.0).min(100.0)
    } else {
        0.0
    };

    let start_str = sprint
        .start
        .map(|d| d.format("%d %b").to_string())
        .unwrap_or_else(|| "?".to_string());
    let end_str = sprint
        .end
        .map(|d| d.format("%d %b %Y").to_string())
        .unwrap_or_else(|| "?".to_string());

    // Calculate burn rate
    let (avg_per_day, need_per_day, status_text, status_color) =
        if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
            let today = Local::now().date_naive();
            let start_date = start.date_naive();
            let end_date = end.date_naive();
            let total_days = (end_date - start_date).num_days() + 1;
            let elapsed_days = (today - start_date).num_days().max(0).min(total_days);
            let remaining_days = (end_date - today).num_days().max(0);

            let avg = if elapsed_days > 0 {
                logged_hours / elapsed_days as f64
            } else {
                0.0
            };

            let need = if remaining_days > 0 {
                remaining_hours / remaining_days as f64
            } else {
                0.0
            };

            let diff = need - avg;
            let status = if remaining_days == 0 {
                ("Sprint ended".to_string(), Color::Gray)
            } else if diff > 1.0 {
                (format!("⚠ Behind schedule (-{:.1}h/day)", diff), Color::Red)
            } else if diff > 0.3 {
                (
                    format!("⚡ Slightly behind (-{:.1}h/day)", diff),
                    Color::Yellow,
                )
            } else if diff < -0.5 {
                ("✨ Ahead of schedule".to_string(), Color::Green)
            } else {
                ("✓ On track".to_string(), Color::Green)
            };

            (avg, need, status.0, status.1)
        } else {
            (0.0, 0.0, "No dates".to_string(), Color::Gray)
        };

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                &sprint.name,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ("),
            Span::styled(
                format!("{:?}", sprint.state),
                Style::default().fg(match sprint.state {
                    SprintState::Active => Color::Green,
                    SprintState::Future => Color::Blue,
                    SprintState::Closed => Color::Gray,
                }),
            ),
            Span::raw(")"),
        ]),
        Line::from(""),
        Line::from(format!("Duration: {} - {}", start_str, end_str)),
        Line::from(format!(
            "Workdays: {} days ({:.0}h capacity)",
            sprint.workdays, capacity_hours
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("Progress: "),
            Span::styled(
                format!("{:.1}h", logged_hours),
                Style::default().fg(Color::Green),
            ),
            Span::raw(" / "),
            Span::styled(
                format!("{:.0}h", capacity_hours),
                Style::default().fg(Color::White),
            ),
            Span::raw(format!("  ({:.0}%)", percentage)),
        ]),
    ];

    // Add progress bar
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .ratio(percentage / 100.0);

    // Render progress bar in a small area
    let progress_area = Rect::new(
        inner.x + 1,
        inner.y + lines.len() as u16,
        inner.width.saturating_sub(2),
        1,
    );
    if progress_area.y < inner.bottom() {
        frame.render_widget(gauge, progress_area);
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("Remaining: "),
        Span::styled(
            format!("{:.1}h", remaining_hours),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(format!("  ({:.0}%)", 100.0 - percentage)),
    ]));
    lines.push(Line::from(format!(
        "Avg/day: {:.1}h    Need: {:.1}h/day",
        avg_per_day, need_per_day
    )));
    lines.push(Line::from(vec![
        Span::raw("Status: "),
        Span::styled(
            status_text,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

fn create_activity_bar(hours: f64, daily_limit: f64) -> String {
    let blocks = (hours / daily_limit * 8.0).round() as usize;
    let blocks = blocks.min(8);

    let filled = "█".repeat(blocks);
    let empty = "░".repeat(8 - blocks);

    format!("{}{}", filled, empty)
}

fn render_sprint_activity_compact(frame: &mut Frame, area: &Rect, sprint: &Sprint, data: &TuiData) {
    let block = Block::default()
        .title("📈 Activity")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    let activities = data
        .sprint_activities
        .get(&sprint.id)
        .cloned()
        .unwrap_or_default();

    if activities.is_empty() {
        let content = Paragraph::new("No activity").alignment(Alignment::Center);
        frame.render_widget(content, inner);
        return;
    }

    // Split activities into two columns for better space utilization
    let midpoint = (activities.len() + 1) / 2;
    let left_activities = &activities[..midpoint];
    let right_activities = if activities.len() > midpoint {
        &activities[midpoint..]
    } else {
        &[]
    };

    // Split the inner area into two columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Render left column
    let mut left_lines = Vec::new();
    for activity in left_activities.iter() {
        let bar = create_activity_bar(activity.hours, data.daily_hours_limit);
        let date_str = activity.date.format("%b %d").to_string();

        let color = if activity.is_absence {
            Style::default().fg(Color::DarkGray)
        } else {
            get_activity_color(activity.hours)
        };

        let line = Line::from(vec![
            Span::styled(date_str, Style::default().fg(Color::Gray)),
            Span::raw(" "),
            Span::styled(bar, color),
            Span::raw(" "),
            Span::styled(
                format!("{:.1}h", activity.hours),
                Style::default().fg(Color::White),
            ),
        ]);
        left_lines.push(line);
    }

    let left_paragraph = Paragraph::new(left_lines).alignment(Alignment::Left);
    frame.render_widget(left_paragraph, columns[0]);

    // Render right column
    if !right_activities.is_empty() {
        let mut right_lines = Vec::new();
        for activity in right_activities.iter() {
            let bar = create_activity_bar(activity.hours, data.daily_hours_limit);
            let date_str = activity.date.format("%b %d").to_string();

            let color = if activity.is_absence {
                Style::default().fg(Color::DarkGray)
            } else {
                get_activity_color(activity.hours)
            };

            let line = Line::from(vec![
                Span::styled(date_str, Style::default().fg(Color::Gray)),
                Span::raw(" "),
                Span::styled(bar, color),
                Span::raw(" "),
                Span::styled(
                    format!("{:.1}h", activity.hours),
                    Style::default().fg(Color::White),
                ),
            ]);
            right_lines.push(line);
        }

        let right_paragraph = Paragraph::new(right_lines).alignment(Alignment::Left);
        frame.render_widget(right_paragraph, columns[1]);
    }
}

fn get_activity_color(hours: f64) -> Style {
    if hours == 0.0 {
        Style::default().fg(Color::DarkGray)
    } else if hours < 3.0 {
        Style::default().fg(Color::Gray)
    } else if hours < 6.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

// Old footer function - replaced by global status bar
// fn render_footer(frame: &mut Frame, area: &Rect, data: &TuiData) {
//     ...
// }

// Helper functions

fn calculate_sprint_logged_hours(sprint_id: usize, data: &TuiData) -> f64 {
    data.sprint_activities
        .get(&sprint_id)
        .map(|activities| {
            activities
                .iter()
                .filter(|a| !a.is_absence) // Exclude absences from logged hours
                .map(|a| a.hours)
                .sum()
        })
        .unwrap_or(0.0)
}
