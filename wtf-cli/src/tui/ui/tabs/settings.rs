use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::data::TuiData;
use crate::tui::theme::theme;
use crate::tui::ui_helpers::build_shortcut_help;
use wtf_lib::config::Config;

/// Number of editable fields in the settings tab (indices 0..FIELD_COUNT-1)
pub(in crate::tui) const FIELD_COUNT: usize = 8;

/// Get the display value for a field from the config
pub(in crate::tui) fn get_field_value(field_idx: usize, config: &Config) -> String {
    match field_idx {
        0 => config.jira.base_url.clone(),
        1 => config.jira.username.clone(),
        2 => config.jira.api_token.reveal().to_string(),
        3 => config
            .jira
            .auto_follow_sprint_pattern
            .clone()
            .unwrap_or_default(),
        4 => config
            .github
            .organisation
            .clone()
            .unwrap_or_default(),
        5 => config
            .google
            .as_ref()
            .map(|g| g.credentials_path.clone())
            .unwrap_or_default(),
        6 => config
            .google
            .as_ref()
            .map(|g| g.token_cache_path.clone())
            .unwrap_or_default(),
        7 => config.worklog.daily_hours_limit.to_string(),
        _ => String::new(),
    }
}

pub(in crate::tui) fn render_settings_tab(frame: &mut Frame, area: &Rect, data: &TuiData) {
    let state = &data.ui_state;
    let config = &data.config;

    // (field_idx, section_label_if_new_section, field_label, is_sensitive)
    let fields: &[(usize, Option<&str>, &str, bool)] = &[
        (0, Some("Jira"), "Base URL", false),
        (1, None, "Username", false),
        (2, None, "API Token", true),
        (3, None, "Sprint Pattern", false),
        (4, Some("GitHub"), "Organisation", false),
        (5, Some("Google"), "Credentials Path", false),
        (6, None, "Token Cache Path", false),
        (7, Some("Worklog"), "Daily Hours Limit", false),
    ];

    let mut lines: Vec<Line> = vec![Line::from("")];

    for (field_idx, section, label, is_sensitive) in fields {
        if let Some(section_name) = section {
            lines.push(Line::from(vec![Span::styled(
                format!(" ── {} ", section_name),
                Style::default()
                    .fg(theme().highlight)
                    .add_modifier(Modifier::BOLD),
            )]));
        }

        let is_selected = state.settings_selected_field == *field_idx;
        let indicator = if is_selected { theme().selector } else { theme().unselected_selector };

        let value_str: String = if state.settings_editing && is_selected {
            format!("{}_", state.settings_input_buffer)
        } else {
            let raw = get_field_value(*field_idx, config);
            if *is_sensitive && !state.settings_show_sensitive.contains(field_idx) {
                if raw.is_empty() {
                    "(not set)".to_string()
                } else {
                    "●●●●●●●●●●●●".to_string()
                }
            } else if raw.is_empty() {
                "(not set)".to_string()
            } else {
                raw
            }
        };

        let value_color = if state.settings_editing && is_selected {
            theme().info
        } else if is_selected {
            theme().fg_primary
        } else {
            theme().fg_secondary
        };

        let label_style = if is_selected {
            Style::default()
                .fg(theme().fg_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme().fg_secondary)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {}", indicator),
                Style::default().fg(theme().highlight),
            ),
            Span::styled(format!("{:<28}", label), label_style),
            Span::styled(value_str, Style::default().fg(value_color)),
        ]));
    }

    // Status / unsaved-changes indicator
    lines.push(Line::from(""));
    if let Some(msg) = &state.settings_status {
        let color = if msg.starts_with('✓') {
            theme().success
        } else {
            theme().error
        };
        lines.push(Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(color),
        )));
        lines.push(Line::from(""));
    }
    if state.settings_dirty {
        lines.push(Line::from(Span::styled(
            " ● Unsaved changes — press [s] to save",
            Style::default().fg(theme().warning),
        )));
    }

    // Build title with shortcuts
    let shortcuts = if state.settings_editing {
        build_shortcut_help(&[("Enter", " Confirm"), ("Esc", " Cancel")])
    } else {
        build_shortcut_help(&[
            ("Enter", " Edit"),
            ("v", " Reveal"),
            ("s", " Save"),
            ("↑↓", " Navigate"),
        ])
    };

    let mut title_spans = vec![Span::raw("Settings | ")];
    title_spans.extend(shortcuts);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(title_spans))
        .title_alignment(Alignment::Left)
        .border_style(Style::default().fg(theme().border))
        .style(Style::default().bg(theme().bg_primary));

    let inner = block.inner(*area);
    frame.render_widget(block, *area);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, inner);
}
