use crate::config::Config;
use crate::models::data::Meeting;
use std::collections::HashSet;

const UNTRACK_KEYWORD: &str = "#untrack";
const NOTRACK_COLOR_VALUE: &str = "notrack";

/// Returns true if a meeting should be treated as untracked, based on:
/// - Manual opt-out (meeting ID in `manual_ids`)
/// - `#untrack` keyword in title or description
/// - Meeting color mapped to "notrack" in config's color_labels
pub fn is_untracked(meeting: &Meeting, config: &Config, manual_ids: &HashSet<String>) -> bool {
    if manual_ids.contains(&meeting.id) {
        return true;
    }

    let has_keyword = |s: &str| s.contains(UNTRACK_KEYWORD);
    if meeting.title.as_deref().map(has_keyword).unwrap_or(false) {
        return true;
    }
    if meeting
        .description
        .as_deref()
        .map(has_keyword)
        .unwrap_or(false)
    {
        return true;
    }

    if let Some(color_id) = &meeting.color_id {
        if let Ok(idx) = color_id.parse::<usize>() {
            if idx >= 1 && idx <= 11 {
                use crate::config::GOOGLE_CALENDAR_EVENT_COLORS;
                let color_name = GOOGLE_CALENDAR_EVENT_COLORS[idx - 1];
                if config
                    .google
                    .as_ref()
                    .and_then(|g| g.color_labels.get(color_name))
                    .map(|v| v == NOTRACK_COLOR_VALUE)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }
    }

    false
}
