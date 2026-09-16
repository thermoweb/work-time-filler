// Gap filling operations: automatically create worklogs for time gaps

use chrono::NaiveDate;
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::LocalWorklogService;

use crate::logger;

use super::super::{
    types::{GapFillConfirmation, GapFillState},
    Tui,
};

/// Days already logging at least this many hours are considered done, not gaps.
const GAP_FILL_MIN_THRESHOLD: f64 = 6.0;

impl Tui {
    /// Workdays of a sprint that are not substantially logged yet, with the hours missing.
    /// Empty when the sprint is unknown, has no date range, or has nothing left to fill.
    pub(in crate::tui) fn sprint_gap_days(&self, sprint_id: usize) -> Vec<(NaiveDate, f64)> {
        let sprint = match self.data.all_sprints.iter().find(|s| s.id == sprint_id) {
            Some(sprint) => sprint,
            None => return Vec::new(),
        };
        let (start, end) = match (sprint.start, sprint.end) {
            (Some(start), Some(end)) => (start, end),
            _ => return Vec::new(),
        };

        let meetings_svc = MeetingsService::production();
        LocalWorklogService::production().find_gap_days(
            start.date_naive(),
            end.date_naive(),
            self.data.daily_hours_limit,
            GAP_FILL_MIN_THRESHOLD,
            &|date| meetings_svc.is_absent(date),
            &self.data.jira_worklogs,
        )
    }

    pub(in crate::tui) fn handle_fill_gaps(&mut self) {
        // Get the selected sprint
        if let Some(sprint) = self
            .data
            .all_sprints
            .get(self.data.ui_state.selected_sprint_index)
        {
            // Check if sprint has date range
            if sprint.start.is_none() || sprint.end.is_none() {
                logger::log("⚠️  Cannot fill gaps: Sprint has no date range".to_string());
                return;
            }

            let sprint_id = sprint.id;

            // Nothing to fill: don't bother asking which issue to charge it to
            if self.sprint_gap_days(sprint_id).is_empty() {
                logger::log(
                    "✓ No gaps to fill - all workdays are substantially logged".to_string(),
                );
                return;
            }

            // Get all issues
            let mut all_issues: Vec<_> = self.data.issues_by_key.values().cloned().collect();

            // Sort by key for now (simple alphabetical)
            all_issues.sort_by(|a, b| a.key.cmp(&b.key));

            // Show gap fill issue selection popup
            self.gap_fill_state = Some(GapFillState {
                sprint_id,
                all_issues,
                selected_issue_index: 0,
                search_query: String::new(),
            });
        }
    }

    /// Recompute the gaps for the chosen issue and show the confirmation popup.
    /// If the gaps vanished in the meantime, the wizard advances instead of stalling.
    pub(in crate::tui) fn open_gap_fill_confirmation(
        &mut self,
        sprint_id: usize,
        issue_id: String,
    ) {
        let gaps = self.sprint_gap_days(sprint_id);

        if gaps.is_empty() {
            logger::log("✓ No gaps to fill - all workdays are substantially logged".to_string());
            if self.wizard_state.is_some() {
                self.wizard_skip_gap_fill("no gaps to fill");
            }
            return;
        }

        let sprint_name = self
            .data
            .all_sprints
            .iter()
            .find(|s| s.id == sprint_id)
            .map(|s| s.name.clone())
            .unwrap_or_default();

        self.gap_fill_confirmation = Some(GapFillConfirmation {
            _sprint_id: sprint_id,
            sprint_name,
            issue_id,
            gaps,
        });
    }
}
