use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::tab_controller::TabController;
use crate::tui::theme::theme;
use crate::tui::Tui;
use wtf_lib::models::achievement::AchievementCategory;
use wtf_lib::models::achievement::AchievementUnlock;
use wtf_lib::models::tiered_achievement::TieredAchievementDef;
use wtf_lib::Achievement;

/// Count of achievements shown in the list: all non-secret + unlocked secrets + tiered defs.
fn shown_achievements_count(
    unlocked: &[AchievementUnlock],
    tiered_progress: &std::collections::HashMap<String, u64>,
) -> usize {
    let _ = tiered_progress; // tiered defs are always visible
    let unlocked_set: std::collections::HashSet<Achievement> =
        unlocked.iter().map(|u| u.achievement).collect();
    let flat_count = Achievement::all()
        .into_iter()
        .filter(|a| {
            let is_secret = a.meta().category == AchievementCategory::Secret;
            !is_secret || unlocked_set.contains(a)
        })
        .count();
    flat_count + TieredAchievementDef::all().len()
}

fn progress_bar(current: u64, max: u64, bar_width: usize) -> String {
    if max == 0 {
        return "░".repeat(bar_width);
    }
    let filled = ((current.min(max) as f64 / max as f64) * bar_width as f64) as usize;
    format!("{}{}", "▓".repeat(filled), "░".repeat(bar_width - filled))
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::tui) struct AchievementsTab;

impl TabController for AchievementsTab {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData) {
        render(frame, area, data);
    }

    fn handle_key(&self, tui: &mut Tui, key: KeyEvent) {
        let shown_count =
            shown_achievements_count(&tui.data.unlocked_achievements, &tui.data.tiered_progress);
        match key.code {
            KeyCode::Left | KeyCode::PageUp => {
                tui.data.ui_state.achievements_scroll_offset = tui
                    .data
                    .ui_state
                    .achievements_scroll_offset
                    .saturating_sub(1);
            }
            KeyCode::Right | KeyCode::PageDown => {
                if tui.data.ui_state.achievements_scroll_offset + 1 < shown_count {
                    tui.data.ui_state.achievements_scroll_offset += 1;
                }
            }
            KeyCode::Home => {
                tui.data.ui_state.achievements_scroll_offset = 0;
            }
            KeyCode::End => {
                tui.data.ui_state.achievements_scroll_offset = shown_count.saturating_sub(1);
            }
            _ => {}
        }
    }
}

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
    } else if words.is_empty()
        && !line1.is_empty()
        && line2.is_empty()
        && line1.len() < width
        && !line1.ends_with('.')
    {
        line1.push('.');
    }

    (line1, line2)
}

pub(in crate::tui) fn render(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let scroll_offset = data.ui_state.achievements_scroll_offset;

    // Main frame
    let block = Block::default()
        .title("🏆 Achievements")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Split into header, content, and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Footer for page indicator
        ])
        .split(inner);

    let unlocked = data.unlocked_achievements.clone();
    let unlocked_map: std::collections::HashMap<_, _> = unlocked
        .iter()
        .map(|u| (u.achievement, u.unlocked_at))
        .collect();

    // Secret locked achievements are excluded from the list entirely.
    // Unlocked secrets are shown and count toward the total (surprise bonus).
    let shown_achievements: Vec<Achievement> = Achievement::all()
        .into_iter()
        .filter(|a| {
            let is_secret = a.meta().category == AchievementCategory::Secret;
            !is_secret || unlocked_map.contains_key(a)
        })
        .collect();

    let tiered_defs = TieredAchievementDef::all();
    let tiered_progress = &data.tiered_progress;

    // Score: flat unlocked points + tiered earned points per tier crossed
    let flat_score: u32 = unlocked_map.keys().map(|a| a.meta().points).sum();
    let tiered_score: u32 = tiered_defs
        .iter()
        .map(|def| {
            let count = tiered_progress.get(def.id).copied().unwrap_or(0);
            def.tiers
                .iter()
                .filter(|t| count >= t.threshold)
                .map(|t| t.points)
                .sum::<u32>()
        })
        .sum();
    let current_score = flat_score + tiered_score;

    let flat_visible_max: u32 = shown_achievements.iter().map(|a| a.meta().points).sum();
    let tiered_visible_max: u32 = tiered_defs
        .iter()
        .map(|def| def.tiers.iter().map(|t| t.points).sum::<u32>())
        .sum();
    let visible_max = flat_visible_max + tiered_visible_max;

    // Header: completion count + score on two lines
    let unlocked_count = unlocked_map.len();
    let total_count = shown_achievements.len() + tiered_defs.len();

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
        Line::from(vec![
            Span::styled("Score: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} / {} pts", current_score, visible_max),
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
    let achievement_width = 37u16; // Width per achievement (35 + 2 for spacing)
    let max_columns = (content_area.width / achievement_width).max(1) as usize;

    // Sort flat achievements: unlocked first, then locked
    let mut sorted_achievements = shown_achievements.clone();
    sorted_achievements.sort_by_key(|achievement| {
        if unlocked_map.contains_key(achievement) {
            0 // Unlocked first
        } else {
            1 // Locked second
        }
    });

    // Calculate layout for achievements (6 lines per achievement: 1 title + 2 desc + 1 date + 2 borders)
    let achievement_height = 6u16;
    let achievements_per_column = (content_area.height / achievement_height).max(1) as usize;

    // Build the unified list: tiered cards first, then flat achievements
    // Each "slot" is either a tiered def index or a flat achievement index
    enum CardSlot {
        Tiered(usize),
        Flat(usize),
    }
    let mut all_slots: Vec<CardSlot> = Vec::new();
    for i in 0..tiered_defs.len() {
        all_slots.push(CardSlot::Tiered(i));
    }
    for i in 0..sorted_achievements.len() {
        all_slots.push(CardSlot::Flat(i));
    }

    let total_visible = (achievements_per_column * max_columns).max(1);

    // Clamp scroll offset to valid range
    let max_scroll = all_slots.len().saturating_sub(total_visible);
    let clamped_scroll_offset = scroll_offset.min(max_scroll);

    // Apply scroll offset
    let start_index = clamped_scroll_offset;
    let end_index = (start_index + total_visible).min(all_slots.len());
    let visible_slots = &all_slots[start_index..end_index];

    // Render achievements in columns (newspaper style: fill column, then next column)
    let text_width = 33usize; // 35 - 2 for borders
    for (display_index, slot) in visible_slots.iter().enumerate() {
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

        match slot {
            CardSlot::Tiered(def_idx) => {
                let def = &tiered_defs[*def_idx];
                let count = tiered_progress.get(def.id).copied().unwrap_or(0);
                let tier_idx = def.current_tier_index(count);
                let next_tier = def.next_tier(count);
                let is_maxed = next_tier.is_none();

                let accent = if is_maxed {
                    Color::Rgb(255, 215, 0)
                } else if tier_idx.is_some() {
                    Color::Rgb(80, 160, 220)
                } else {
                    Color::DarkGray
                };

                let border_color = if is_maxed {
                    Color::Rgb(200, 160, 0)
                } else if tier_idx.is_some() {
                    Color::Rgb(60, 120, 180)
                } else {
                    Color::DarkGray
                };

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color));

                let inner_area = block.inner(achievement_area);
                frame.render_widget(block, achievement_area);

                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1); 4])
                    .split(inner_area);

                // Row 0: icon | tier name | pts right-aligned
                let current_tier = tier_idx.map(|i| &def.tiers[i]);
                let icon = current_tier.map(|t| t.icon).unwrap_or("⬜");
                let tier_name = current_tier.map(|t| t.name).unwrap_or("Not started");
                let pts_earned: u32 = def
                    .tiers
                    .iter()
                    .filter(|t| count >= t.threshold)
                    .map(|t| t.points)
                    .sum();
                let pts_max: u32 = def.tiers.iter().map(|t| t.points).sum();

                let title_cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(7),
                    ])
                    .split(rows[0]);

                frame.render_widget(
                    Paragraph::new(Span::styled(icon, Style::default().fg(Color::White))),
                    title_cols[0],
                );
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        tier_name,
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    )),
                    title_cols[1],
                );
                if pts_max > 0 {
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!("{}/{}", pts_earned, pts_max),
                            Style::default().fg(accent),
                        ))
                        .alignment(Alignment::Right),
                        title_cols[2],
                    );
                }

                // Row 1: progress bar | count/next_threshold
                let bar_width = 20usize;
                let (bar_current, bar_max) = if let Some(nt) = next_tier {
                    let prev_threshold = tier_idx
                        .and_then(|i| {
                            if i > 0 {
                                Some(def.tiers[i].threshold)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    (
                        count.saturating_sub(prev_threshold),
                        nt.threshold - prev_threshold,
                    )
                } else {
                    // Maxed out
                    let last = def.tiers.last().unwrap();
                    (last.threshold, last.threshold)
                };
                let bar_str = progress_bar(bar_current, bar_max, bar_width);
                let count_label = if let Some(nt) = next_tier {
                    format!("{}/{}", count, nt.threshold)
                } else {
                    format!("{}", count)
                };

                let progress_cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(bar_width as u16), Constraint::Min(0)])
                    .split(rows[1]);

                frame.render_widget(
                    Paragraph::new(Span::styled(bar_str, Style::default().fg(accent))),
                    progress_cols[0],
                );
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        format!(" {}", count_label),
                        Style::default().fg(Color::Gray),
                    )),
                    progress_cols[1],
                );

                // Row 2: unit label
                frame.render_widget(
                    Paragraph::new(Span::styled(def.unit, Style::default().fg(Color::DarkGray))),
                    rows[2],
                );

                // Row 3: next tier info or maxed
                let next_label = if is_maxed {
                    "✓ MAXED".to_string()
                } else if let Some(nt) = next_tier {
                    let label = format!("→ {} at {}", nt.name, nt.threshold);
                    if label.len() > text_width {
                        format!("{}...", &label[..text_width.saturating_sub(3)])
                    } else {
                        label
                    }
                } else {
                    String::new()
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        next_label,
                        Style::default().fg(if is_maxed { accent } else { Color::DarkGray }),
                    )),
                    rows[3],
                );
            }
            CardSlot::Flat(ach_idx) => {
                let achievement = &sorted_achievements[*ach_idx];
                let meta = achievement.meta();
                let is_unlocked = unlocked_map.contains_key(achievement);
                let is_secret = meta.category == AchievementCategory::Secret;

                if is_unlocked {
                    // Secrets render orange; regular achievements render gold
                    let accent = if is_secret {
                        Color::Rgb(255, 140, 0)
                    } else {
                        Color::Rgb(255, 215, 0)
                    };
                    let border_color = if is_secret {
                        Color::Rgb(200, 100, 0)
                    } else {
                        theme().achievement_border
                    };

                    let unlocked_at = unlocked_map.get(achievement).unwrap();
                    let date_str = unlocked_at.format("%d/%m/%y").to_string();

                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(border_color));

                    let inner_area = block.inner(achievement_area);
                    frame.render_widget(block, achievement_area);

                    let rows = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(1); 4])
                        .split(inner_area);

                    // Row 0: icon (fixed 3 cells) | name — isolates ZWJ emoji width mismatch
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
                            Style::default().fg(accent).add_modifier(Modifier::BOLD),
                        )),
                        title_cols[1],
                    );

                    // Rows 1-2: description
                    let (desc_line1, desc_line2) =
                        wrap_text_two_lines(&meta.description, text_width);
                    frame.render_widget(
                        Paragraph::new(Span::styled(desc_line1, Style::default().fg(Color::Gray))),
                        rows[1],
                    );
                    frame.render_widget(
                        Paragraph::new(Span::styled(desc_line2, Style::default().fg(Color::Gray))),
                        rows[2],
                    );

                    // Row 3: ✓ | date | pts (right-aligned)
                    let bottom_cols = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Min(0),
                            Constraint::Length(7),
                        ])
                        .split(rows[3]);

                    frame.render_widget(
                        Paragraph::new(Span::styled("✓", Style::default().fg(accent))),
                        bottom_cols[0],
                    );
                    frame.render_widget(
                        Paragraph::new(Span::styled(date_str, Style::default().fg(accent))),
                        bottom_cols[1],
                    );
                    if meta.points > 0 {
                        frame.render_widget(
                            Paragraph::new(Span::styled(
                                format!("{} pts", meta.points),
                                Style::default().fg(accent),
                            ))
                            .alignment(Alignment::Right),
                            bottom_cols[2],
                        );
                    }
                } else {
                    // LOCKED - show real name/description dimmed, with lock + pts badge
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray));

                    let inner_area = block.inner(achievement_area);
                    frame.render_widget(block, achievement_area);

                    let rows = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(1); 4])
                        .split(inner_area);

                    // Row 0: icon | name (dimmed)
                    let title_cols = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Length(3), Constraint::Min(0)])
                        .split(rows[0]);

                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            meta.icon.as_str(),
                            Style::default().fg(Color::DarkGray),
                        )),
                        title_cols[0],
                    );
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            meta.name.as_str(),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::BOLD),
                        )),
                        title_cols[1],
                    );

                    // Rows 1-2: description (dimmed)
                    let (desc_line1, desc_line2) =
                        wrap_text_two_lines(&meta.description, text_width);
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            desc_line1,
                            Style::default().fg(Color::DarkGray),
                        )),
                        rows[1],
                    );
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            desc_line2,
                            Style::default().fg(Color::DarkGray),
                        )),
                        rows[2],
                    );

                    // Row 3: 🔒 | pts badge (right-aligned)
                    let bottom_cols = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Min(0),
                            Constraint::Length(7),
                        ])
                        .split(rows[3]);

                    frame.render_widget(
                        Paragraph::new(Span::styled("🔒", Style::default().fg(Color::DarkGray))),
                        bottom_cols[0],
                    );
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!("{} pts", meta.points),
                            Style::default().fg(Color::DarkGray),
                        ))
                        .alignment(Alignment::Right),
                        bottom_cols[2],
                    );
                }
            }
        }
    }

    // Footer with pagination info
    let footer_area = chunks[2];
    if all_slots.len() > total_visible && total_visible > 0 {
        let current_page = (clamped_scroll_offset / total_visible) + 1;
        let total_pages = all_slots.len().div_ceil(total_visible);

        let footer_text = format!(
            "Page {}/{} | Showing {}-{} of {} | ←→ to scroll",
            current_page,
            total_pages,
            start_index + 1,
            end_index,
            all_slots.len()
        );

        let footer = Paragraph::new(footer_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);

        frame.render_widget(footer, footer_area);
    }
}
