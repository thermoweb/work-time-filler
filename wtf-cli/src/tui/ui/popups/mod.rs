// Popup rendering modules organized by functionality

mod wizard;
mod issue_selection;
mod confirmations;
mod other;

use ratatui::Frame;

// Re-export individual functions for direct use if needed
pub(in crate::tui) use wizard::{
    render_wizard,
    render_wizard_cancel_confirmation,
};

pub(in crate::tui) use issue_selection::{
    render_issue_selection_popup,
    render_gap_fill_issue_selection,
};

pub(in crate::tui) use confirmations::{
    render_unlink_confirmation,
    render_revert_confirmation,
    render_worklog_creation_confirmation,
    render_gap_fill_confirmation,
};

pub(in crate::tui) use other::{
    render_sprint_follow_popup,
    render_about_popup,
};

/// Render all active popups in the correct priority order
/// 
/// Popup rendering priority (bottom to top):
/// 1. Wizard (if active)
/// 2. Various confirmations and selections
/// 3. Wizard cancel confirmation (highest priority)
/// 4. About popup (on top of everything)
pub(in crate::tui) fn render_all(frame: &mut Frame, tui: &crate::tui::Tui) {
    // Render wizard if active (takes precedence over other popups except cancel confirmation)
    if let Some(wizard) = &tui.wizard_state {
        render_wizard(frame, wizard, &tui.data, &tui.gap_fill_state);
    }

    // Render wizard cancel confirmation (highest priority for wizard)
    if let Some(_cancel) = &tui.wizard_cancel_confirmation {
        render_wizard_cancel_confirmation(frame);
    }

    // Render issue selection popup if active (also used by wizard manual linking)
    if let Some(state) = &tui.issue_selection_state {
        render_issue_selection_popup(frame, state);
    }

    // Render unlink confirmation if active
    if let Some(meeting_id) = &tui.unlink_confirmation_meeting_id {
        render_unlink_confirmation(frame, &tui.data, meeting_id);
    }

    // Render revert confirmation if active
    if let Some(state) = &tui.revert_confirmation_state {
        render_revert_confirmation(frame, &tui.data, state);
    }

    // Render worklog creation confirmation if active
    if let Some(state) = &tui.worklog_creation_confirmation {
        render_worklog_creation_confirmation(frame, state);
    }

    // Render gap fill issue selection if active (only when NOT in wizard)
    if tui.wizard_state.is_none() {
        if let Some(state) = &tui.gap_fill_state {
            render_gap_fill_issue_selection(frame, state);
        }
    }

    // Render gap fill confirmation if active (also used by wizard gap filling)
    if let Some(state) = &tui.gap_fill_confirmation {
        render_gap_fill_confirmation(frame, state);
    }

    // Render sprint follow popup if active
    if let Some(state) = &tui.sprint_follow_state {
        render_sprint_follow_popup(frame, state);
    }

    // Render about popup if active (should be on top of everything)
    if tui.show_about_popup {
        render_about_popup(frame, &tui.about_image);
    }
}
