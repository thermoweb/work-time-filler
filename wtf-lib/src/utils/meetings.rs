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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GoogleConfig, GOOGLE_CALENDAR_EVENT_COLORS};
    use crate::models::data::Meeting;
    use chrono::Utc;
    use std::collections::HashMap;

    fn base_meeting() -> Meeting {
        Meeting {
            id: "meet-1".to_string(),
            title: None,
            description: None,
            start: Utc::now(),
            end: Utc::now(),
            attendees: None,
            jira_link: None,
            recurrence: None,
            logs: HashMap::new(),
            my_response_status: None,
            color_id: None,
        }
    }

    #[test]
    fn test_not_untracked_by_default() {
        let meeting = base_meeting();
        let config = Config::default();
        assert!(!is_untracked(&meeting, &config, &HashSet::new()));
    }

    #[test]
    fn test_untracked_by_manual_id() {
        let meeting = base_meeting();
        let config = Config::default();
        let mut ids = HashSet::new();
        ids.insert("meet-1".to_string());
        assert!(is_untracked(&meeting, &config, &ids));
    }

    #[test]
    fn test_untracked_by_keyword_in_title() {
        let mut meeting = base_meeting();
        meeting.title = Some("Daily #untrack standup".to_string());
        assert!(is_untracked(&meeting, &Config::default(), &HashSet::new()));
    }

    #[test]
    fn test_untracked_by_keyword_in_description() {
        let mut meeting = base_meeting();
        meeting.description = Some("This is an #untrack meeting".to_string());
        assert!(is_untracked(&meeting, &Config::default(), &HashSet::new()));
    }

    #[test]
    fn test_untracked_by_notrack_color() {
        let mut meeting = base_meeting();
        // color_id "3" → index 2 → GOOGLE_CALENDAR_EVENT_COLORS[2] = "Grape"
        meeting.color_id = Some("3".to_string());
        let color_name = GOOGLE_CALENDAR_EVENT_COLORS[2]; // "Grape"

        let mut config = Config::default();
        let mut color_labels = HashMap::new();
        color_labels.insert(color_name.to_string(), "notrack".to_string());
        config.google = Some(GoogleConfig {
            credentials_path: String::new(),
            token_cache_path: String::new(),
            color_labels,
        });

        assert!(is_untracked(&meeting, &config, &HashSet::new()));
    }

    #[test]
    fn test_not_untracked_when_color_mapped_to_other_value() {
        let mut meeting = base_meeting();
        meeting.color_id = Some("3".to_string());
        let color_name = GOOGLE_CALENDAR_EVENT_COLORS[2];

        let mut config = Config::default();
        let mut color_labels = HashMap::new();
        color_labels.insert(color_name.to_string(), "PROJ-123".to_string());
        config.google = Some(GoogleConfig {
            credentials_path: String::new(),
            token_cache_path: String::new(),
            color_labels,
        });

        assert!(!is_untracked(&meeting, &config, &HashSet::new()));
    }
}
