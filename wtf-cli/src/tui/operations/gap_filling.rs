// Gap filling operations: automatically create worklogs for time gaps

use crate::logger;

use super::super::{types::GapFillState, Tui};

impl Tui {
    pub(in crate::tui) fn handle_fill_gaps(&mut self) {
        // Get the selected sprint
        if let Some(sprint) = self.data.all_sprints.get(self.data.ui_state.selected_sprint_index) {
            // Check if sprint has date range
            if sprint.start.is_none() || sprint.end.is_none() {
                logger::log("⚠️  Cannot fill gaps: Sprint has no date range".to_string());
                return;
            }

            // Get all issues
            let mut all_issues: Vec<_> = self.data.issues_by_key.values().cloned().collect();

            // Sort by key for now (simple alphabetical)
            all_issues.sort_by(|a, b| a.key.cmp(&b.key));

            // Show gap fill issue selection popup
            self.gap_fill_state = Some(GapFillState {
                sprint_id: sprint.id,
                all_issues,
                selected_issue_index: 0,
                search_query: String::new(),
            });
        }
    }
}
