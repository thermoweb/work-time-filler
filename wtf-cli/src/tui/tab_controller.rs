use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::{data::TuiData, Tui};

/// A keyboard action offered by a tab.
///
/// Single source of truth for both the shortcut hints shown in the panel title
/// and the help popup (`?`), so the two can't drift apart.
#[derive(Debug, Clone, Copy)]
pub(in crate::tui) struct TabAction {
    /// Key as displayed to the user (e.g. "W", "Del", "Ctrl+A")
    pub key: &'static str,
    /// Short action name (e.g. "Wizard")
    pub name: &'static str,
    /// What the action actually does, shown in the help popup
    pub description: &'static str,
    /// Whether the action is advertised in the panel title
    pub in_title: bool,
}

impl TabAction {
    pub(in crate::tui) const fn title(
        key: &'static str,
        name: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            key,
            name,
            description,
            in_title: true,
        }
    }

    pub(in crate::tui) const fn help_only(
        key: &'static str,
        name: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            key,
            name,
            description,
            in_title: false,
        }
    }
}

pub(in crate::tui) trait TabController {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData);
    fn handle_key(&self, tui: &mut Tui, key: KeyEvent);
    /// Actions offered by this tab, listed in the help popup.
    fn actions(&self) -> &'static [TabAction];
}
