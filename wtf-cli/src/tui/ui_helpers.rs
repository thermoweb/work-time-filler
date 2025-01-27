use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    Frame,
};

/// Render a tab with list on left (60%) and details on right (40%)
/// This is a common pattern used across multiple tabs
pub(super) fn render_list_detail_layout<L, D>(
    frame: &mut Frame,
    area: &Rect,
    render_list: L,
    render_details: D,
) where
    L: FnOnce(&mut Frame, &Rect),
    D: FnOnce(&mut Frame, &Rect),
{
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // List
            Constraint::Percentage(40), // Details
        ])
        .split(*area);

    render_list(frame, &chunks[0]);
    render_details(frame, &chunks[1]);
}

/// Helper function to truncate strings with ellipsis
pub(super) fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

/// Helper function to build styled shortcut help text
pub(super) fn build_shortcut_help(shortcuts: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, (key, label)) in shortcuts.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::raw("["));
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw("]"));
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(Color::Gray),
        ));
    }
    spans
}

/// Helper function to wrap text to a maximum width
pub(super) fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Helper function to parse and clean meeting description
/// Handles formats like: JIRA[<a href="url">text</a>] and HTML entities
pub(super) fn parse_description(desc: &str) -> Vec<(String, Option<String>)> {
    // Returns Vec of (text, optional_url) tuples
    let mut result = Vec::new();

    // Parse JIRA[<a href="...">...</a>] format
    let jira_pattern = regex::Regex::new(r#"JIRA\[<a href="([^"]+)">([^<]+)</a>\]"#).unwrap();

    let mut last_end = 0;
    for cap in jira_pattern.captures_iter(desc) {
        // Add text before this match
        if let Some(m) = cap.get(0) {
            if m.start() > last_end {
                let text = &desc[last_end..m.start()];
                if !text.is_empty() {
                    result.push((clean_and_strip_html(text), None));
                }
            }

            // Add the link
            if let (Some(url), Some(text)) = (cap.get(1), cap.get(2)) {
                result.push((text.as_str().to_string(), Some(url.as_str().to_string())));
            }

            last_end = m.end();
        }
    }

    // Add remaining text after last match
    if last_end < desc.len() {
        let text = &desc[last_end..];
        if !text.is_empty() {
            result.push((clean_and_strip_html(text), None));
        }
    }

    // If no JIRA links found, return the whole description
    if result.is_empty() {
        result.push((clean_and_strip_html(desc), None));
    }

    result
}

/// Parse HTML formatted text into segments with styling info
#[derive(Clone)]
pub(super) struct StyledSegment {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Parse HTML and extract styled segments
pub(super) fn parse_html_styled_text(html: &str) -> Vec<StyledSegment> {
    let mut segments = Vec::new();
    let mut current = StyledSegment {
        text: String::new(),
        bold: false,
        italic: false,
        underline: false,
    };

    // Stack to track nested formatting
    let mut bold_depth: u32 = 0;
    let mut italic_depth: u32 = 0;
    let mut underline_depth: u32 = 0;

    // Simple HTML tag regex
    let tag_pattern = regex::Regex::new(r"<(/?)([a-zA-Z]+)[^>]*>").unwrap();

    let mut last_end = 0;
    for cap in tag_pattern.captures_iter(html) {
        if let Some(m) = cap.get(0) {
            // Add text before this tag
            if m.start() > last_end {
                current.text.push_str(&html[last_end..m.start()]);
            }

            let is_closing = cap.get(1).map_or(false, |m| m.as_str() == "/");
            let tag_name = cap.get(2).map_or("", |m| m.as_str()).to_lowercase();

            match tag_name.as_str() {
                "b" | "strong" => {
                    if is_closing {
                        bold_depth = bold_depth.saturating_sub(1);
                    } else {
                        bold_depth += 1;
                    }
                }
                "i" | "em" => {
                    if is_closing {
                        italic_depth = italic_depth.saturating_sub(1);
                    } else {
                        italic_depth += 1;
                    }
                }
                "u" => {
                    if is_closing {
                        underline_depth = underline_depth.saturating_sub(1);
                    } else {
                        underline_depth += 1;
                    }
                }
                _ => {
                    // Ignore other tags (they'll be stripped)
                }
            }

            // If style changed and we have text, push current segment
            let new_bold = bold_depth > 0;
            let new_italic = italic_depth > 0;
            let new_underline = underline_depth > 0;

            if (new_bold != current.bold
                || new_italic != current.italic
                || new_underline != current.underline)
                && !current.text.is_empty()
            {
                segments.push(current.clone());
                current = StyledSegment {
                    text: String::new(),
                    bold: new_bold,
                    italic: new_italic,
                    underline: new_underline,
                };
            } else {
                current.bold = new_bold;
                current.italic = new_italic;
                current.underline = new_underline;
            }

            last_end = m.end();
        }
    }

    // Add remaining text
    if last_end < html.len() {
        current.text.push_str(&html[last_end..]);
    }

    if !current.text.is_empty() {
        segments.push(current);
    }

    segments
}

/// Clean HTML entities and strip unsupported HTML tags, keeping formatting tags
pub(super) fn clean_and_strip_html(text: &str) -> String {
    // First, clean HTML entities
    let cleaned = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");

    // Parse styled segments and flatten back to text (formatting will be applied during rendering)
    let segments = parse_html_styled_text(&cleaned);

    // For now, just extract the text (we'll use segments in rendering later)
    segments
        .into_iter()
        .map(|s| s.text)
        .collect::<Vec<_>>()
        .join("")
}
