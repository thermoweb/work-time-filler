use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::tab_controller::TabAction;
use crate::tui::theme::theme;
use crate::tui::ui_helpers::wrap_text;
use crate::tui::Tui;

const KEY_WIDTH: usize = 12;
const NAME_WIDTH: usize = 26;

const GLOBAL_ACTIONS: &[TabAction] = &[
    TabAction::help_only(
        "Tab / 1-7",
        "Switch tab",
        "Go to the next tab, or jump to one.",
    ),
    TabAction::help_only("PgUp / PgDn", "Scroll logs", "Scroll the logs panel."),
    TabAction::help_only("Ctrl+L", "Copy logs", "Copy the logs to the clipboard."),
    TabAction::help_only("H", "About", "Show the about screen."),
    TabAction::help_only("Q", "Quit", "Leave wtf."),
];

/// Help popup: lists what each action of the current tab does
pub(in crate::tui) fn render_help_popup(frame: &mut Frame, tui: &Tui, scroll: u16) {
    let (lines, popup_area) = help_layout(frame.area(), tui);
    let max_scroll = max_scroll(lines.len(), popup_area);

    let mut bottom_hint = vec![
        Span::raw(" "),
        key_span("[?]"),
        Span::raw("/"),
        key_span("[Esc]"),
        Span::raw(" close "),
    ];
    if max_scroll > 0 {
        bottom_hint.extend([key_span(" [↑↓]"), Span::raw(" scroll ")]);
    }

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" ❓ What does each action do? ")
        .title_bottom(Line::from(bottom_hint).alignment(Alignment::Center))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme().highlight))
        .style(Style::default().bg(theme().bg_primary));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((scroll.min(max_scroll), 0)),
        popup_area,
    );
}

/// Keys while the help popup is open: it captures everything so nothing reaches the tab behind
pub(in crate::tui) fn handle_help_key(tui: &mut Tui, key: KeyEvent) {
    let Some(scroll) = tui.help_popup_scroll else {
        return;
    };
    let max = terminal_max_scroll(tui);
    // Clamp first: the terminal may have been resized since the last scroll
    let scroll = scroll.min(max);

    tui.help_popup_scroll = match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Char('Q') => None,
        KeyCode::Up | KeyCode::Char('k') => Some(scroll.saturating_sub(1)),
        KeyCode::Down | KeyCode::Char('j') => Some((scroll + 1).min(max)),
        KeyCode::Home => Some(0),
        KeyCode::End => Some(max),
        _ => Some(scroll),
    };
}

/// Mouse wheel while the help popup is open
pub(in crate::tui) fn scroll_help(tui: &mut Tui, scroll_up: bool) {
    let key = if scroll_up {
        KeyCode::Up
    } else {
        KeyCode::Down
    };
    handle_help_key(tui, KeyEvent::new(key, KeyModifiers::empty()));
}

fn terminal_max_scroll(tui: &Tui) -> u16 {
    let Ok((width, height)) = crossterm::terminal::size() else {
        return 0;
    };
    let (lines, popup_area) = help_layout(Rect::new(0, 0, width, height), tui);
    max_scroll(lines.len(), popup_area)
}

fn max_scroll(line_count: usize, popup_area: Rect) -> u16 {
    let visible = popup_area.height.saturating_sub(2) as usize; // minus borders
    line_count.saturating_sub(visible) as u16
}

/// Content and position of the popup for a given screen area
/// (shared by rendering and scrolling so both agree on the scroll limit)
fn help_layout(area: Rect, tui: &Tui) -> (Vec<Line<'static>>, Rect) {
    let popup_width = 110.min(area.width.saturating_sub(4));
    let description_width = (popup_width as usize)
        .saturating_sub(KEY_WIDTH + NAME_WIDTH + 4)
        .max(20);

    let mut lines = vec![Line::from("")];
    lines.push(section_title(&format!("{} tab", tui.current_tab.label())));
    // Tab actions have multi-line descriptions: a blank line between each keeps them readable
    for action in tui.current_tab.actions(tui) {
        lines.push(Line::from(""));
        lines.extend(action_lines(action, description_width));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(section_title("Everywhere"));
    for action in GLOBAL_ACTIONS {
        lines.extend(action_lines(action, description_width));
    }
    lines.push(Line::from(""));

    let popup_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };
    (lines, popup_area)
}

fn section_title(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {}", title),
        Style::default()
            .fg(theme().info)
            .add_modifier(Modifier::BOLD),
    ))
}

fn key_span(key: &str) -> Span<'static> {
    Span::styled(
        key.to_string(),
        Style::default()
            .fg(theme().highlight)
            .add_modifier(Modifier::BOLD),
    )
}

fn action_lines(action: &TabAction, description_width: usize) -> Vec<Line<'static>> {
    let description = wrap_text(action.description, description_width);
    let indent = " ".repeat(KEY_WIDTH + NAME_WIDTH + 2);

    description
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let text_span = Span::styled(text, Style::default().fg(theme().fg_secondary));
            if i == 0 {
                Line::from(vec![
                    Span::raw("  "),
                    key_span(&format!("{:<width$}", action.key, width = KEY_WIDTH)),
                    Span::styled(
                        format!("{:<width$}", action.name, width = NAME_WIDTH),
                        Style::default().fg(theme().fg_primary),
                    ),
                    text_span,
                ])
            } else {
                Line::from(vec![Span::raw(indent.clone()), text_span])
            }
        })
        .collect()
}
