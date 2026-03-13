use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::{data::TuiData, Tui};

pub(in crate::tui) trait TabController {
    fn render(&self, frame: &mut Frame, area: &Rect, data: &TuiData);
    fn handle_key(&self, tui: &mut Tui, key: KeyEvent);
}
