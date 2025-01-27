use ratatui::style::Color;

/// Centralized theme configuration for the TUI
pub struct Theme {
    // Background colors
    pub bg_primary: Color,

    // Foreground/text colors
    pub fg_primary: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,

    // Accent colors

    // Status colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    
    // UI element colors
    pub border: Color,
    pub achievement_border: Color,
    pub highlight: Color,

    // Icons
    pub selector: &'static str,
    pub unselected_selector: &'static str,
}

impl Theme {
    /// Get the default theme
    pub fn default() -> Self {
        Self {
            // Background colors
            bg_primary: Color::Rgb(23, 20, 33),

            // Foreground/text colors
            fg_primary: Color::White,
            fg_secondary: Color::Gray,
            fg_muted: Color::DarkGray,

            // Status colors
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Cyan,
            
            // UI element colors
            border: Color::Cyan,
            achievement_border: Color::Yellow,
            highlight: Color::Yellow,
            selector: "► ",
            unselected_selector: "  ",
        }
    }
}

/// Global theme instance
static THEME: once_cell::sync::Lazy<Theme> = once_cell::sync::Lazy::new(Theme::default);

/// Get the current theme
pub fn theme() -> &'static Theme {
    &THEME
}
