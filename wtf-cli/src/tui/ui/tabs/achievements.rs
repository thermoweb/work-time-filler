use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::theme::theme;
use wtf_lib::Achievement;
use wtf_lib::services::AchievementService;

/// Wrap text to exactly 2 lines, fitting within given width
fn wrap_text_two_lines(text: &str, width: usize) -> (String, String) {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let mut line1 = String::new();
    let mut line2 = String::new();
    
    // Fill first line
    while !words.is_empty() {
        let word = words[0];
        let test_line = if line1.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", line1, word)
        };
        
        if test_line.len() <= width {
            line1 = test_line;
            words.remove(0);
        } else {
            break;
        }
    }
    
    // Fill second line with remaining words
    while !words.is_empty() {
        let word = words[0];
        let test_line = if line2.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", line2, word)
        };
        
        if test_line.len() <= width {
            line2 = test_line;
            words.remove(0);
        } else {
            // Word doesn't fit, truncate with ...
            if line2.is_empty() {
                line2 = format!("{}...", &word[..width.saturating_sub(3)]);
            } else {
                line2 = format!("{}...", line2);
            }
            break;
        }
    }
    
    // Add dot at the end if text fits completely
    if words.is_empty() && !line2.is_empty() && line2.len() < width {
        if !line2.ends_with('.') {
            line2.push('.');
        }
    } else if words.is_empty() && !line1.is_empty() && line2.is_empty() && line1.len() < width {
        if !line1.ends_with('.') {
            line1.push('.');
        }
    }
    
    (line1, line2)
}

pub fn render(frame: &mut Frame, area: Rect, data: &TuiData) {
    let scroll_offset = data.ui_state.achievements_scroll_offset;
    
    // Main frame
    let block = Block::default()
        .title(" Achievements ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split into header, content, and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Content
            Constraint::Length(1),  // Footer for page indicator
        ])
        .split(inner);

    // Header with stats
    let unlocked_count = AchievementService::unlock_count();
    let total_count = Achievement::all().len();
    
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Completed: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} / {}", unlocked_count, total_count),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ])
    .alignment(Alignment::Center);
    
    frame.render_widget(header, chunks[0]);

    // Calculate how many columns can fit based on terminal width
    let content_area = chunks[1];
    let achievement_width = 37; // Width per achievement (35 + 2 for spacing)
    let max_columns = (content_area.width / achievement_width).max(1) as usize;
    
    // Achievement list with proper Block widgets
    let all_achievements = Achievement::all();
    let unlocked = AchievementService::get_all_unlocked();
    let unlocked_map: std::collections::HashMap<_, _> =
        unlocked.iter().map(|u| (u.achievement, u.unlocked_at)).collect();

    // Sort achievements: unlocked first, then locked
    let mut sorted_achievements = all_achievements.clone();
    sorted_achievements.sort_by_key(|achievement| {
        if unlocked_map.contains_key(achievement) {
            0 // Unlocked achievements first
        } else {
            1 // Locked achievements second
        }
    });

    // Calculate layout for achievements (6 lines per achievement: 1 title + 2 desc + 1 date + 2 borders)
    let achievement_height = 6;
    let achievements_per_column = (content_area.height / achievement_height).max(1) as usize;
    let total_visible = (achievements_per_column * max_columns).max(1);
    
    // Clamp scroll offset to valid range
    let max_scroll = sorted_achievements.len().saturating_sub(total_visible);
    let clamped_scroll_offset = scroll_offset.min(max_scroll);
    
    // Apply scroll offset
    let start_index = clamped_scroll_offset;
    let end_index = (start_index + total_visible).min(sorted_achievements.len());
    let visible_achievements = &sorted_achievements[start_index..end_index];
    
    // Render achievements in columns (newspaper style: fill column, then next column)
    let text_width = 33; // 35 - 2 for borders
    for (display_index, achievement) in visible_achievements.iter().enumerate() {
        // Calculate column and row
        let col = display_index / achievements_per_column;
        let row = display_index % achievements_per_column;
        
        // Calculate position
        let x_offset = col as u16 * achievement_width;
        let y_offset = row as u16 * achievement_height;
        
        let achievement_area = Rect {
            x: content_area.x + x_offset,
            y: content_area.y + y_offset,
            width: 35,
            height: achievement_height,
        };
        
        let meta = achievement.meta();
        let is_unlocked = unlocked_map.contains_key(achievement);

        if is_unlocked {
            // UNLOCKED - Golden block
            let unlocked_at = unlocked_map.get(achievement).unwrap();
            let date_str = unlocked_at.format("%d/%m/%y").to_string();

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme().achievement_border));

            let inner = block.inner(achievement_area);
            frame.render_widget(block, achievement_area);

            // Split inner into 4 rows
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1); 4])
                .split(inner);

            // Row 0: icon (fixed 3 cells) | name (rest) — isolates ZWJ emoji width mismatch
            let title_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(rows[0]);

            frame.render_widget(
                Paragraph::new(Span::styled(meta.icon, Style::default().fg(Color::White))),
                title_cols[0],
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    meta.name,
                    Style::default()
                        .fg(Color::Rgb(255, 215, 0))
                        .add_modifier(Modifier::BOLD),
                )),
                title_cols[1],
            );

            // Rows 1-2: description
            let (desc_line1, desc_line2) = wrap_text_two_lines(meta.description, text_width);
            frame.render_widget(
                Paragraph::new(Span::styled(desc_line1, Style::default().fg(Color::Gray))),
                rows[1],
            );
            frame.render_widget(
                Paragraph::new(Span::styled(desc_line2, Style::default().fg(Color::Gray))),
                rows[2],
            );

            // Row 3: check mark (fixed 2 cells) | date
            let date_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(2), Constraint::Min(0)])
                .split(rows[3]);

            frame.render_widget(
                Paragraph::new(Span::styled("✓", Style::default().fg(Color::Green))),
                date_cols[0],
            );
            frame.render_widget(
                Paragraph::new(Span::styled(date_str, Style::default().fg(Color::Green))),
                date_cols[1],
            );
        } else {
            // LOCKED - Gray block
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray));

            let inner = block.inner(achievement_area);
            frame.render_widget(block, achievement_area);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
                .split(inner);

            let lock_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(rows[0]);

            frame.render_widget(
                Paragraph::new(Span::styled("🔒", Style::default().fg(Color::DarkGray))),
                lock_cols[0],
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "???",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )),
                lock_cols[1],
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "Hidden",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
                rows[1],
            );
        }
    }
    
    // Footer with pagination info
    let footer_area = chunks[2];
    if sorted_achievements.len() > total_visible && total_visible > 0 {
        let current_page = (clamped_scroll_offset / total_visible) + 1;
        let total_pages = (sorted_achievements.len() + total_visible - 1) / total_visible;
        
        let footer_text = format!(
            "Page {}/{} | Showing {}-{} of {} | ←→ to scroll",
            current_page,
            total_pages,
            start_index + 1,
            end_index,
            sorted_achievements.len()
        );
        
        let footer = Paragraph::new(footer_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        
        frame.render_widget(footer, footer_area);
    }
}
