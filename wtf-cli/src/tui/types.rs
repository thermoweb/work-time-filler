// Type definitions for dashboard state and configuration

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::Receiver;

use wtf_lib::models::data::Meeting;

use super::data::TuiData;

// ============================================================================
// EventBus System - Centralized event handling
// ============================================================================

/// All events that can occur in the application
#[derive(Debug, Clone)]
#[allow(dead_code)] // Event data fields are used by subscribers, not always directly read
pub enum AppEvent {
    // Fetch events
    FetchComplete(TuiData),
    FetchError(String),
    
    // Push events
    PushComplete { history_id: String },
    PushProgress { current: usize, total: usize, message: String },
    PushError(String),
    
    // Revert events
    RevertComplete,
    RevertError(String),
    
    // Data events
    DataRefreshed(TuiData),
    
    // Status events
    StatusMessageTimeout,
    
    // UI events
    AboutPopupOpened,
    
    // Achievement events
    AchievementUnlocked { achievement: wtf_lib::Achievement },
    
    // Secret sequence events
    SecretSequenceTriggered { sequence_name: String },
}

/// Trait for components that react to events
pub trait EventSubscriber {
    fn on_event(&mut self, event: &AppEvent, tui: &mut Tui);
}

/// Central event bus for decoupled communication
pub struct EventBus {
    pending_events: VecDeque<AppEvent>,
    subscribers: Vec<Box<dyn EventSubscriber>>,
    #[allow(dead_code)]
    history: Vec<AppEvent>, // For debugging/replay
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            pending_events: VecDeque::new(),
            subscribers: Vec::new(),
            history: Vec::new(),
        }
    }
    
    /// Publish an event (can be called from anywhere, including background threads)
    pub fn publish(&mut self, event: AppEvent) {
        self.history.push(event.clone());
        self.pending_events.push_back(event);
    }
    
    /// Process all pending events
    pub fn process_events(&mut self, tui: &mut Tui) {
        while let Some(event) = self.pending_events.pop_front() {
            for subscriber in &mut self.subscribers {
                subscriber.on_event(&event, tui);
            }
        }
    }
    
    /// Register a subscriber
    pub fn subscribe(&mut self, subscriber: Box<dyn EventSubscriber>) {
        self.subscribers.push(subscriber);
    }
    
    /// Debug helpers
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending_events.len()
    }
    
    #[allow(dead_code)]
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

// ============================================================================
// Existing Types
// ============================================================================

#[derive(Debug, Clone)]
pub enum FetchStatus {
    Idle,
    Fetching(String), // Message describing what's being fetched
    Complete,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Sprints,
    Meetings,
    Worklogs,
    GitHub,
    History,
    Achievements,
    Settings,
}

impl Tab {
    /// Get all available tabs (conditional based on state)
    pub fn available_tabs(has_achievements: bool) -> Vec<Tab> {
        let mut tabs = vec![
            Tab::Sprints,
            Tab::Meetings,
            Tab::Worklogs,
            Tab::GitHub,
            Tab::History,
            Tab::Settings,
        ];

        if has_achievements {
            tabs.push(Tab::Achievements);
        }

        tabs
    }
    
    pub fn next(self, has_achievements: bool) -> Self {
        let tabs = Self::available_tabs(has_achievements);
        let current_index = tabs.iter().position(|&t| t == self).unwrap_or(0);
        let next_index = (current_index + 1) % tabs.len();
        tabs[next_index]
    }

    pub fn previous(self, has_achievements: bool) -> Self {
        let tabs = Self::available_tabs(has_achievements);
        let current_index = tabs.iter().position(|&t| t == self).unwrap_or(0);
        let prev_index = if current_index == 0 {
            tabs.len() - 1
        } else {
            current_index - 1
        };
        tabs[prev_index]
    }

    /// Render this tab's content
    pub fn render(
        &self,
        frame: &mut ratatui::Frame,
        area: &ratatui::layout::Rect,
        data: &super::data::TuiData,
    ) {
        use super::ui::tabs;
        
        match self {
            Tab::Sprints => tabs::sprints::render_sprints_tab(frame, area, data),
            Tab::Meetings => tabs::meetings::render_meetings_tab(frame, area, data),
            Tab::Worklogs => tabs::worklogs::render_worklogs_tab(frame, area, data),
            Tab::GitHub => tabs::github::render_github_tab(frame, area, data),
            Tab::History => tabs::history::render_history_tab(frame, area, data),
            Tab::Achievements => tabs::achievements::render(frame, *area, data),
            Tab::Settings => tabs::settings::render_settings_tab(frame, area, data),
        }
    }
}

pub struct Tui {
    pub(crate) data: super::data::TuiData,
    pub(crate) current_tab: Tab,
    pub(crate) revert_confirmation_state: Option<RevertConfirmationState>,
    pub(crate) worklog_creation_confirmation: Option<WorklogCreationConfirmation>,
    pub(crate) gap_fill_state: Option<GapFillState>,
    pub(crate) gap_fill_confirmation: Option<GapFillConfirmation>,
    pub(crate) wizard_state: Option<WizardState>,
    pub(crate) wizard_cancel_confirmation: Option<WizardCancelConfirmation>,
    pub(crate) sprint_follow_state: Option<SprintFollowState>,
    pub(crate) issue_selection_state: Option<IssueSelectionState>,
    pub(crate) unlink_confirmation_meeting_id: Option<String>,
    pub(crate) show_about_popup: bool,
    pub(crate) about_image: Option<image::DynamicImage>,
    pub(crate) fetch_status: FetchStatus,
    
    // EventBus - Centralized event system  
    pub(crate) event_bus: EventBus,
    
    // Key sequence tracking for secret achievements
    pub(crate) key_sequence_buffer: VecDeque<String>,
    
    // Channel receivers for async operations (bridge to EventBus)
    pub(super) fetch_receiver: Option<Receiver<FetchStatus>>,
    pub(super) revert_receiver: Option<Receiver<Result<(), String>>>,
    pub(super) push_receiver: Option<Receiver<(String, String)>>,
    pub(super) push_progress_receiver: Option<Receiver<String>>,
    pub(super) data_refresh_receiver: Option<Receiver<super::data::TuiData>>,
    pub(super) update_receiver: Option<Receiver<Option<String>>>,
    
    pub(super) status_clear_time: Option<std::time::Instant>,
    pub(super) needs_full_clear: bool,
    pub(super) should_quit: bool,
    pub(super) log_collector: std::sync::Arc<crate::logger::CollectingLogger>,
}

pub struct RevertConfirmationState {
    pub(crate) history_id: String,
    pub(crate) user_input: String,
    pub(crate) reverting: bool, // Track if revert is in progress
}

pub struct IssueSelectionState {
    pub(crate) meeting_id: String,
    pub(crate) all_issues: Vec<wtf_lib::models::data::Issue>, // Keep all issues
    pub(crate) selected_issue_index: usize,
    pub(crate) search_query: String, // Search filter
}

pub struct GapFillState {
    pub(crate) sprint_id: usize,
    pub(crate) all_issues: Vec<wtf_lib::models::data::Issue>,
    pub(crate) selected_issue_index: usize,
    pub(crate) search_query: String,
}

pub struct GapFillConfirmation {
    pub(crate) _sprint_id: usize,
    pub(crate) sprint_name: String,
    pub(crate) issue_id: String,
    pub(crate) gaps: Vec<(chrono::NaiveDate, f64)>, // (date, hours_to_add)
}

pub struct SprintFollowState {
    pub(crate) all_sprints: Vec<wtf_lib::models::data::Sprint>,
    pub(crate) selected_index: usize,
    pub(crate) search_query: String,
}

#[derive(Clone)]
pub struct WorklogCreationConfirmation {
    pub source: WorklogSource,
    pub issue_id: String,
    pub date: chrono::NaiveDate,
    pub requested_hours: f64,
    pub existing_hours: f64,
    pub daily_limit: f64,
    pub user_input: String,
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum WorklogSource {
    Meeting {
        meeting_id: String,
        title: String,
    },
    GitHub {
        session_id: String,
        description: String,
    },
}

impl WorklogCreationConfirmation {
    pub fn suggested_hours(&self) -> f64 {
        (self.daily_limit - self.existing_hours).max(0.0)
    }

    #[allow(dead_code)]
    pub fn would_exceed(&self) -> bool {
        self.existing_hours + self.requested_hours > self.daily_limit
    }
}

// Wizard state structures
#[allow(dead_code)]
pub enum WizardStep {
    Syncing,
    AutoLinking,
    ManualLinking {
        unlinked_meetings: Vec<Meeting>,
        selected_index: usize,
    },
    CreatingMeetingWorklogs,
    CreatingGitHubWorklogs {
        sessions: Vec<wtf_lib::models::data::GitHubSession>,
        current_session_index: usize,
    },
    FillingGaps {
        selected_issue: Option<String>,
    },
    ReviewingWorklogs {
        excluded_days: std::collections::HashSet<chrono::NaiveDate>,
    },
    Pushing,
    Complete,
}

pub struct WizardState {
    pub sprint_id: usize,
    pub sprint_name: String,
    pub current_step: WizardStep,
    pub completed_steps: std::collections::HashSet<usize>, // 1-8
    pub summary: WizardSummary,
    pub rollback_log: WizardRollbackLog,
    pub skip_reasons: HashMap<usize, String>, // Track why steps were skipped
    pub push_logs: Vec<String>,               // Recent push logs to show in wizard UI
    pub spinner_frame: usize,                 // For animating current step indicator
    pub push_current: usize,                  // Current worklog being pushed (for progress bar)
    pub push_total: usize,                    // Total worklogs to push (for progress bar)
    pub startup_message: Option<String>,      // Chronie's startup quote (set once)
}

#[derive(Clone)]
pub struct WizardRollbackLog {
    pub linked_meeting_ids: Vec<String>, // Both auto and manual
    pub created_worklog_ids: Vec<String>,
    pub original_meeting_links: HashMap<String, Option<String>>,
}

#[derive(Clone)]
pub struct WizardSummary {
    pub meetings_auto_linked: usize,
    pub meetings_manually_linked: usize,
    pub worklogs_from_meetings: usize,
    pub worklogs_from_github: usize,
    pub worklogs_from_gaps: usize,
    pub total_hours: f64,
    pub pushed_count: usize,
}

impl Default for WizardSummary {
    fn default() -> Self {
        Self {
            meetings_auto_linked: 0,
            meetings_manually_linked: 0,
            worklogs_from_meetings: 0,
            worklogs_from_github: 0,
            worklogs_from_gaps: 0,
            total_hours: 0.0,
            pushed_count: 0,
        }
    }
}

impl Default for WizardRollbackLog {
    fn default() -> Self {
        Self {
            linked_meeting_ids: Vec::new(),
            created_worklog_ids: Vec::new(),
            original_meeting_links: HashMap::new(),
        }
    }
}

pub struct WizardCancelConfirmation;
