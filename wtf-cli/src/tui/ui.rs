use chrono::Local;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::data::TuiData;
use super::{FetchStatus, Tab};
use super::theme::theme;

mod popups;
pub(in crate::tui) mod tabs;

/// Render the main UI based on current tab
pub fn render(frame: &mut Frame, tui: &super::Tui, logs: &[String]) {
    // Set background for entire frame
    frame.render_widget(
        Block::default().style(Style::default().bg(theme().bg_primary)),
        frame.area(),
    );

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)  // Ensure no margin
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Content
            Constraint::Length(8), // Logs panel (7 lines + border)
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Render tab bar
    render_tab_bar(frame, &main_chunks[0], tui.current_tab);

    // Render current tab content
    tui.current_tab.render(frame, &main_chunks[1], &tui.data);

    // Render logs panel
    render_logs_panel(frame, &main_chunks[2], logs);

    // Render status bar at bottom
    render_status_bar(frame, &main_chunks[3], &tui.data, &tui.fetch_status);

    // Render all active popups in priority order
    popups::render_all(frame, tui);
}

fn render_tab_bar(frame: &mut Frame, area: &Rect, current_tab: Tab) {
    let has_achievements = wtf_lib::services::AchievementService::has_any_unlocked();
    
    let mut tabs = vec![
        ("1", "Sprints", Tab::Sprints),
        ("2", "Meetings", Tab::Meetings),
        ("3", "Worklogs", Tab::Worklogs),
        ("4", "GitHub", Tab::GitHub),
        ("5", "History", Tab::History),
        ("6", "Settings", Tab::Settings),
    ];

    if has_achievements {
        tabs.push(("7", "Achievements", Tab::Achievements));
    }

    let tab_titles: Vec<Span> = tabs
        .iter()
        .flat_map(|(num, name, tab)| {
            let is_active = *tab == current_tab;
            vec![
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", num),
                    if is_active {
                        Style::default()
                            .fg(theme().highlight)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme().fg_muted)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    *name,
                    if is_active {
                        Style::default()
                            .fg(theme().fg_primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme().fg_secondary)
                    },
                ),
                Span::raw("  "),
            ]
        })
        .collect();

    let title_line = Line::from(tab_titles);
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .style(Style::default().fg(theme().border).bg(theme().bg_primary));

    let paragraph = Paragraph::new(title_line).block(block);
    frame.render_widget(paragraph, *area);
}

fn render_logs_panel(frame: &mut Frame, area: &Rect, logs: &[String]) {
    let block = Block::default()
        .title("📝 Logs")
        .borders(Borders::ALL)
        .style(Style::default().fg(theme().border).bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    // Show logs that fit in the inner area (account for borders)
    let max_logs = inner.height as usize;
    let recent_logs: Vec<Line> = logs
        .iter()
        .rev()
        .take(max_logs)
        .rev()
        .map(|log| Line::from(Span::styled(log.clone(), Style::default().fg(theme().fg_secondary))))
        .collect();

    let paragraph = Paragraph::new(recent_logs).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}

/// Render status bar at the bottom
fn render_status_bar(frame: &mut Frame, area: &Rect, data: &TuiData, fetch_status: &FetchStatus) {
    // Split status bar into left (app version) and center (status/shortcuts)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20),  // Left: app version (enough for "WTF (0.1.0-beta.0)")
            Constraint::Min(0),      // Center: status/shortcuts
        ])
        .split(*area);
    
    // Render app name and version on the left
    let version = env!("CARGO_PKG_VERSION");
    let app_info = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("WTF ({})", version),
            Style::default().fg(theme().fg_muted),
        ),
    ]))
    .alignment(Alignment::Left)
    .style(Style::default().bg(theme().bg_primary));
    
    frame.render_widget(app_info, chunks[0]);
    
    // Render status/shortcuts in the center
    let content = match fetch_status {
        FetchStatus::Fetching(message) => {
            // Show spinner and message when fetching
            let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                / 80) as usize
                % spinner_frames.len();
            let spinner = spinner_frames[frame_idx];

            Line::from(vec![
                Span::styled(
                    format!("{} ", spinner),
                    Style::default()
                        .fg(theme().warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(message, Style::default().fg(theme().info)),
            ])
        }
        FetchStatus::Complete => Line::from(vec![
            Span::styled(
                "✓ ",
                Style::default()
                    .fg(theme().success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Update complete", Style::default().fg(theme().success)),
        ]),
        FetchStatus::Error(err) => Line::from(vec![
            Span::styled(
                "✗ ",
                Style::default().fg(theme().error).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("Error: {}", err), Style::default().fg(theme().error)),
        ]),
        FetchStatus::Idle => {
            // Show normal footer with last sync time
            let elapsed = Local::now().signed_duration_since(data.last_sync.with_timezone(&Local));
            let time_ago = if elapsed.num_seconds() < 60 {
                format!("{}s ago", elapsed.num_seconds())
            } else {
                format!("{}m ago", elapsed.num_minutes())
            };

            Line::from(vec![
                Span::raw("Last sync: "),
                Span::styled(time_ago, Style::default().fg(theme().fg_muted)),
                Span::raw("   "),
                Span::styled(
                    "[Q]",
                    Style::default()
                        .fg(theme().highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Quit  "),
                Span::styled(
                    "[R]",
                    Style::default()
                        .fg(theme().highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Refresh  "),
                Span::styled(
                    "[U]",
                    Style::default()
                        .fg(theme().highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Update  "),
                Span::styled(
                    "[Tab]",
                    Style::default()
                        .fg(theme().highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Switch"),
            ])
        }
    };

    let paragraph = Paragraph::new(content)
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme().bg_primary));

    frame.render_widget(paragraph, chunks[1]);
}

// GitHub tab extracted to ui/tabs/github.rs
