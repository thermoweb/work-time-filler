pub mod data;
mod helpers;
mod operations;
mod types;
pub mod ui;
mod ui_helpers;
mod wizard;
mod achievement_tracker;
pub mod theme;

// Re-export types for public API
pub use types::*;

use achievement_tracker::AchievementTracker;

// Import custom logger macros
use crate::{info, error};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use indicatif::MultiProgress;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::VecDeque;
use std::io;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use crate::commands::fetch::fetch_google_meetings;
use crate::logger::{self, collecting_logger};
use crate::tasks::jira_tasks::{
    FetchJiraBoard, FetchJiraIssues, FetchJiraSprint, FetchJiraWorklogs,
};
use crate::tasks::Task;
use data::TuiData;
use once_cell::sync::Lazy;
use wtf_lib::models::data::{LocalWorklog, LocalWorklogState, Meeting};
use wtf_lib::services::jira_service::JiraService;
use wtf_lib::services::meetings_service::MeetingsService;
use wtf_lib::services::worklogs_service::LocalWorklogService;
use wtf_lib::utils::branding::AppBranding;

// Load application branding from embedded logo
static APP_BRANDING: Lazy<Option<AppBranding>> = Lazy::new(|| AppBranding::load().ok());

impl Tui {
    pub fn new() -> Self {
        // Initialize collecting logger for TUI mode and bridge to log crate
        let log_collector = collecting_logger();
        logger::init_logger_with_log_bridge(log_collector.clone() as std::sync::Arc<dyn crate::logger::Logger>);

        // Chronie's startup greeting (only if ChroniesApprentice is unlocked)
        log_chronie_message("startup", "🧙 Chronie:");

        // Load logo image for About popup
        let about_image = Self::load_logo_image();

        // Initialize EventBus and register subscribers
        let mut event_bus = EventBus::new();
        event_bus.subscribe(Box::new(wizard::WizardEventHandler));
        event_bus.subscribe(Box::new(AchievementTracker));


        Self {
            data: TuiData::collect(),
            current_tab: Tab::Sprints,
            revert_confirmation_state: None,
            worklog_creation_confirmation: None,
            gap_fill_state: None,
            gap_fill_confirmation: None,
            wizard_state: None,
            wizard_cancel_confirmation: None,
            sprint_follow_state: None,
            issue_selection_state: None,
            unlink_confirmation_meeting_id: None,
            show_about_popup: false,
            about_image,
            fetch_status: FetchStatus::Idle,
            event_bus,
            key_sequence_buffer: VecDeque::with_capacity(20),
            fetch_receiver: None,
            revert_receiver: None,
            push_receiver: None,
            push_progress_receiver: None,
            data_refresh_receiver: None,
            status_clear_time: None,
            should_quit: false,
            log_collector,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Run the main loop
        let res = self.main_loop(&mut terminal);

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(err) = res {
            eprintln!("Error: {:?}", err);
        }

        Ok(())
    }

    /// Handle all async operation updates (fetch, push, revert, data refresh)
    fn handle_async_operations(&mut self) {
        self.handle_fetch_status();
        self.handle_revert_completion();
        self.handle_push_operations();
        self.handle_data_refresh();
        self.check_and_clear_status_timer();
        self.wizard_update_animation();
        
        // Process EventBus events (temporarily take ownership to avoid borrow issues)
        let mut event_bus = std::mem::replace(&mut self.event_bus, EventBus::new());
        event_bus.process_events(self);
        self.event_bus = event_bus;
    }

    /// Handle fetch status updates - bridge channel to EventBus
    fn handle_fetch_status(&mut self) {
        if let Some(receiver) = &self.fetch_receiver {
            if let Ok(status) = receiver.try_recv() {
                self.fetch_status = status.clone();
                match status {
                    FetchStatus::Complete => {
                        self.refresh_data();
                        self.fetch_receiver = None;
                        self.status_clear_time = Some(std::time::Instant::now());
                        self.event_bus.publish(AppEvent::FetchComplete(self.data.clone()));
                    }
                    FetchStatus::Error(err) => {
                        self.fetch_receiver = None;
                        self.status_clear_time = Some(std::time::Instant::now());
                        self.event_bus.publish(AppEvent::FetchError(err));
                    }
                    _ => {}
                }
            }
        }
    }

    /// Handle revert completion - bridge channel to EventBus
    fn handle_revert_completion(&mut self) {
        if let Some(receiver) = &self.revert_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.revert_receiver = None;
                self.revert_confirmation_state = None;
                match result {
                    Ok(()) => {
                        info!("Revert completed successfully");
                        self.refresh_data();
                        log_chronie_message("rewriting_history", "🧙 Chronie:");
                        self.event_bus.publish(AppEvent::RevertComplete);
                    }
                    Err(e) => {
                        error!("Revert failed: {}", e);
                        self.event_bus.publish(AppEvent::RevertError(e));
                    }
                }
            }
        }
    }

    /// Handle push operations - bridge channels to EventBus
    fn handle_push_operations(&mut self) {
        // Check for completion
        if let Some(receiver) = &self.push_receiver {
            if let Ok((msg, history_id)) = receiver.try_recv() {
                self.push_receiver = None;
                self.push_progress_receiver = None;
                self.fetch_status = FetchStatus::Complete;
                info!("{}", msg);
                self.refresh_data();
                self.status_clear_time = Some(std::time::Instant::now());
                self.event_bus.publish(AppEvent::PushComplete { history_id });
            }
        }

        // Check for progress updates
        if let Some(receiver) = &self.push_progress_receiver {
            let mut progress_logs = Vec::new();
            while let Ok(log_msg) = receiver.try_recv() {
                progress_logs.push(log_msg);
            }
            for log_msg in progress_logs {
                // Parse and publish event
                if let Some(start) = log_msg.find('[') {
                    if let Some(end) = log_msg.find(']') {
                        let progress_str = &log_msg[start + 1..end];
                        if let Some(slash_pos) = progress_str.find('/') {
                            if let (Ok(current), Ok(total)) = (
                                progress_str[..slash_pos].trim().parse::<usize>(),
                                progress_str[slash_pos + 1..].trim().parse::<usize>(),
                            ) {
                                self.event_bus.publish(AppEvent::PushProgress {
                                    current,
                                    total,
                                    message: log_msg,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Handle data refresh - bridge channel to EventBus
    fn handle_data_refresh(&mut self) {
        if let Some(receiver) = &self.data_refresh_receiver {
            if let Ok(new_data) = receiver.try_recv() {
                self.data = new_data.clone();
                self.data_refresh_receiver = None;
                self.event_bus.publish(AppEvent::DataRefreshed(new_data));
            }
        }
    }

    /// Auto-clear status messages after timeout
    fn check_and_clear_status_timer(&mut self) {
        if let Some(clear_time) = self.status_clear_time {
            let timeout = if matches!(self.fetch_status, FetchStatus::Error(_)) {
                Duration::from_secs(5)
            } else {
                Duration::from_secs(3)
            };

            if clear_time.elapsed() > timeout {
                self.fetch_status = FetchStatus::Idle;
                self.status_clear_time = None;
                
                // New way: Publish event
                self.event_bus.publish(AppEvent::StatusMessageTimeout);
            }
        }
    }

    fn main_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<()> {
        loop {
            let logs = self.log_collector.get_messages();
            terminal.draw(|f| ui::render(f, self, &logs))?;

            // Handle all async operations
            self.handle_async_operations();

            // Poll for keyboard events
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key);
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Export current logs to clipboard
    fn export_logs(&self) {
        use arboard::Clipboard;
        use std::thread;
        use std::time::Duration;
        
        // Get logs from collector
        let logs = self.log_collector.get_messages();
        let content = logs.join("\n");
        
        // Copy to clipboard
        match Clipboard::new() {
            Ok(mut clipboard) => {
                match clipboard.set_text(&content) {
                    Ok(_) => {
                        logger::log(format!("📋 Logs copied to clipboard! ({} lines)", logs.len()));
                        // Keep clipboard alive for a bit so clipboard manager can grab it
                        // This is necessary on X11/Wayland systems
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        logger::log(format!("❌ Failed to copy to clipboard: {}", e));
                    }
                }
            }
            Err(e) => {
                logger::log(format!("❌ Failed to access clipboard: {}", e));
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Track key sequences globally (for secret achievements)
        // Skip tracking if we're in input mode (wizard, gap fill, issue selection)
        let in_input_mode = self.wizard_state.is_some() 
            || self.gap_fill_state.is_some() 
            || self.issue_selection_state.is_some()
            || self.sprint_follow_state.is_some()
            || (self.current_tab == Tab::Settings && self.data.ui_state.settings_editing);
        
        if !in_input_mode {
            self.track_key_sequence(&key);
        }
        
        // Revert confirmation popup takes highest priority
        if self.revert_confirmation_state.is_some() {
            self.handle_history_key(key);
            return;
        }

        // Wizard cancel confirmation takes highest priority
        if self.wizard_cancel_confirmation.is_some() {
            self.handle_wizard_cancel_confirmation_key(key);
            return;
        }

        // If wizard is active, handle wizard keys (except for manual linking which uses existing popup)
        if let Some(ref wizard) = self.wizard_state {
            match wizard.current_step {
                WizardStep::ManualLinking { .. } => {
                    // If in manual linking and issue selection is shown, let it handle keys
                    if self.issue_selection_state.is_some() {
                        self.handle_issue_selection_key(key);
                        return;
                    }
                    // Allow Esc to cancel wizard
                    if key.code == KeyCode::Esc {
                        self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
                        return;
                    }
                    // Otherwise handle manual linking navigation
                    self.handle_wizard_manual_linking_key(key);
                    return;
                }
                WizardStep::CreatingGitHubWorklogs { .. } => {
                    // If worklog creation confirmation is shown, handle it
                    if self.worklog_creation_confirmation.is_some() {
                        self.handle_worklog_creation_confirmation_key(key);
                        return;
                    }
                    // Allow Esc to cancel wizard when not in a confirmation
                    if key.code == KeyCode::Esc {
                        self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
                        return;
                    }
                }
                WizardStep::FillingGaps { .. } => {
                    // Gap fill popups handle their own keys
                    if self.gap_fill_confirmation.is_some() {
                        self.handle_gap_fill_confirmation_key(key);
                        return;
                    }
                    if self.gap_fill_state.is_some() {
                        self.handle_gap_fill_issue_selection_key(key);
                        return;
                    }
                    // Allow Esc to cancel wizard when not in a popup
                    if key.code == KeyCode::Esc {
                        self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
                        return;
                    }
                }
                WizardStep::ReviewingWorklogs { .. } => {
                    // Review step: P to push, Esc to cancel
                    match key.code {
                        KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Enter => {
                            self.wizard_step_push();
                            return;
                        }
                        KeyCode::Esc => {
                            self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
                            return;
                        }
                        _ => {}
                    }
                }
                WizardStep::Pushing => {
                    // During push, only allow Esc (which shows warning)
                    if key.code == KeyCode::Esc {
                        logger::log(
                            "⚠️  Cannot cancel wizard during push - please wait".to_string(),
                        );
                    }
                    return;
                }
                WizardStep::Complete => {
                    // Any key closes the wizard
                    logger::log("✅ Wizard closed".to_string());
                    self.wizard_state = None;
                    return;
                }
                _ => {
                    // Other wizard steps: allow Esc to cancel
                    if key.code == KeyCode::Esc {
                        self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
                        return;
                    }
                }
            }
        }

        // If we're in gap fill confirmation mode, handle that first
        if self.gap_fill_confirmation.is_some() {
            self.handle_gap_fill_confirmation_key(key);
            return;
        }

        // If we're in gap fill issue selection mode, handle that
        if self.gap_fill_state.is_some() {
            self.handle_gap_fill_issue_selection_key(key);
            return;
        }

        // If we're in worklog creation confirmation mode, handle that
        if self.worklog_creation_confirmation.is_some() {
            self.handle_worklog_creation_confirmation_key(key);
            return;
        }

        // If we're in issue selection mode, handle that
        if self.issue_selection_state.is_some() {
            self.handle_issue_selection_key(key);
            return;
        }

        // If we're in sprint follow mode, handle that
        if self.sprint_follow_state.is_some() {
            self.handle_sprint_follow_key(key);
            return;
        }

        // If we're in settings edit mode, capture all keys before global shortcuts
        if self.current_tab == Tab::Settings && self.data.ui_state.settings_editing {
            self.handle_settings_key(key);
            return;
        }

        // If we're in unlink confirmation mode, handle that
        if self.unlink_confirmation_meeting_id.is_some() {
            self.handle_unlink_confirmation_key(key);
            return;
        }

        // About popup - global key that works anywhere
        if key.code == KeyCode::Char('h') || key.code == KeyCode::Char('H') {
            self.show_about_popup = !self.show_about_popup;
            
            // Publish event when About popup is opened
            if self.show_about_popup {
                self.event_bus.publish(AppEvent::AboutPopupOpened);
                
                // Process events immediately to trigger achievement
                let mut event_bus = std::mem::take(&mut self.event_bus);
                event_bus.process_events(self);
                self.event_bus = event_bus;
            }
            
            return;
        }

        // Secret Chronie trigger - ² key (friend-tier messages for ChroniesFriend!)
        if key.code == KeyCode::Char('²') {
            log_chronie_message("friend", "🧙 Chronie:");
            return;
        }

        // Export logs to clipboard - Ctrl+L key
        if key.code == KeyCode::Char('l') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
            self.export_logs();
            return;
        }

        // If about popup is shown, Esc closes it
        if self.show_about_popup {
            if key.code == KeyCode::Esc {
                self.show_about_popup = false;
            }
            return;
        }

        match key.code {
            // Global keys
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Esc | KeyCode::Char('c')
                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
            {
                self.should_quit = true;
            }
            // Tab switching
            KeyCode::Tab => {
                let has_achievements = wtf_lib::services::AchievementService::has_any_unlocked();
                self.current_tab = self.current_tab.next(has_achievements);
            }
            KeyCode::BackTab => {
                let has_achievements = wtf_lib::services::AchievementService::has_any_unlocked();
                self.current_tab = self.current_tab.previous(has_achievements);
            }
            KeyCode::Char('1') => {
                self.current_tab = Tab::Sprints;
            }
            KeyCode::Char('2') => {
                self.current_tab = Tab::Meetings;
            }
            KeyCode::Char('3') => {
                self.current_tab = Tab::Worklogs;
            }
            KeyCode::Char('4') => {
                self.current_tab = Tab::GitHub;
            }
            KeyCode::Char('5') => {
                self.current_tab = Tab::History;
            }
            KeyCode::Char('6') => {
                self.current_tab = Tab::Settings;
            }
            KeyCode::Char('7') => {
                // Only allow switching to Achievements if user has unlocked at least one
                if wtf_lib::services::AchievementService::has_any_unlocked() {
                    self.current_tab = Tab::Achievements;
                }
            }
            // Tab-specific navigation and actions
            _ => {
                self.handle_tab_specific_key(key);
            }
        }
    }

    fn handle_tab_specific_key(&mut self, key: KeyEvent) {
        match self.current_tab {
            Tab::Sprints => {
                self.handle_sprints_key(key);
            }
            Tab::Meetings => {
                self.handle_meetings_key(key);
            }
            Tab::Worklogs => {
                self.handle_worklogs_key(key);
            }
            Tab::GitHub => {
                self.handle_github_key(key);
            }
            Tab::History => {
                self.handle_history_key(key);
            }
            Tab::Achievements => {
                // Handle scrolling for achievements with left/right arrow keys
                // The render function will clamp the offset to valid range
                match key.code {
                    KeyCode::Left | KeyCode::PageUp => {
                        self.data.ui_state.achievements_scroll_offset = 
                            self.data.ui_state.achievements_scroll_offset.saturating_sub(1);
                    }
                    KeyCode::Right | KeyCode::PageDown => {
                        let total_achievements = wtf_lib::Achievement::all().len();
                        // Allow scrolling up to total-1 (render will clamp to actual max)
                        if self.data.ui_state.achievements_scroll_offset < total_achievements {
                            self.data.ui_state.achievements_scroll_offset += 1;
                        }
                    }
                    KeyCode::Home => {
                        self.data.ui_state.achievements_scroll_offset = 0;
                    }
                    KeyCode::End => {
                        // Set to high value, render will clamp
                        self.data.ui_state.achievements_scroll_offset = wtf_lib::Achievement::all().len();
                    }
                    _ => {}
                }
            }
            Tab::Settings => {
                self.handle_settings_key(key);
            }
        }
    }

    fn handle_update(&mut self) {
        // Don't start a new fetch if one is already in progress
        if matches!(self.fetch_status, FetchStatus::Fetching(_)) {
            return;
        }

        let (sender, receiver) = channel();
        self.fetch_receiver = Some(receiver);

        let tab = self.current_tab;

        // Spawn background thread to run async fetch
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();

            runtime.block_on(async {
                // Create a hidden MultiProgress to suppress terminal output
                let mp = MultiProgress::new();
                mp.set_draw_target(indicatif::ProgressDrawTarget::hidden());

                match tab {
                    Tab::Sprints => {
                        // Fetch all for sprints tab (main tab now)
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching all data...".to_string()));

                        // Fetch boards
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching boards...".to_string()));
                        let _ = FetchJiraBoard::new()
                            .with_progress(mp.clone())
                            .execute()
                            .await;

                        // Fetch sprints
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching sprints...".to_string()));
                        let _ = FetchJiraSprint::new()
                            .with_progress(mp.clone())
                            .execute()
                            .await;

                        // Fetch issues
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching issues...".to_string()));
                        let sprints = JiraService::get_followed_sprint();
                        let _ = FetchJiraIssues::new(sprints.clone())
                            .with_progress(mp.clone())
                            .execute()
                            .await;

                        // Fetch worklogs
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching worklogs...".to_string()));
                        let _ = FetchJiraWorklogs::new(sprints)
                            .with_progress(mp.clone())
                            .execute()
                            .await;

                        // Fetch Google meetings
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching meetings...".to_string()));
                        match fetch_google_meetings(Some(mp.clone())).await {
                            Ok(_) => {
                                let _ = sender.send(FetchStatus::Complete);
                            }
                            Err(e) => {
                                let _ = sender.send(FetchStatus::Error(e));
                            }
                        }
                    }
                    Tab::Meetings => {
                        // Fetch google meetings only
                        let _ = sender.send(FetchStatus::Fetching(
                            "Fetching Google Calendar events...".to_string(),
                        ));
                        match fetch_google_meetings(Some(mp.clone())).await {
                            Ok(_) => {
                                let _ = sender.send(FetchStatus::Complete);
                            }
                            Err(e) => {
                                let _ = sender.send(FetchStatus::Error(e));
                            }
                        }
                    }
                    Tab::Worklogs => {
                        // Fetch worklogs only
                        let _ =
                            sender.send(FetchStatus::Fetching("Fetching worklogs...".to_string()));
                        let sprints = JiraService::get_followed_sprint();
                        let _ = FetchJiraWorklogs::new(sprints)
                            .with_progress(mp.clone())
                            .execute()
                            .await;
                        let _ = sender.send(FetchStatus::Complete);
                    }
                    Tab::GitHub => {
                        // For now, no fetch action on GitHub tab
                        let _ = sender.send(FetchStatus::Complete);
                    }
                    Tab::History => {
                        // No fetch action on History tab, it's local data
                        let _ = sender.send(FetchStatus::Complete);
                    }
                    Tab::Achievements => {
                        // No fetch action on Achievements tab, it's local data
                        let _ = sender.send(FetchStatus::Complete);
                    }
                    Tab::Settings => {
                        // No fetch action on Settings tab
                        let _ = sender.send(FetchStatus::Complete);
                    }
                }
            });
        });
    }

    fn handle_sprints_key(&mut self, key: KeyEvent) {
        let max_index = self.data.all_sprints.len().saturating_sub(1);

        // Handle standard navigation keys first
        if helpers::handle_list_navigation(key, &mut self.data.ui_state.selected_sprint_index, max_index) {
            return;
        }

        // Handle sprint-specific keys
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_data();
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.handle_update();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                self.handle_fill_gaps();
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Open sprint follow/unfollow popup - get ALL available sprints
                let mut all_sprints =
                    wtf_lib::services::jira_service::JiraService::get_available_sprints();

                // Sort by start date (newest first), putting sprints without dates at the end
                all_sprints.sort_by(|a, b| {
                    match (a.start, b.start) {
                        (Some(a_start), Some(b_start)) => b_start.cmp(&a_start), // Reverse order (newest first)
                        (Some(_), None) => std::cmp::Ordering::Less, // With date before without
                        (None, Some(_)) => std::cmp::Ordering::Greater, // Without date after with
                        (None, None) => a.name.cmp(&b.name), // Both without date, sort by name
                    }
                });

                self.sprint_follow_state = Some(SprintFollowState {
                    all_sprints,
                    selected_index: 0,
                    search_query: String::new(),
                });
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                self.launch_wizard();
            }
            _ => {}
        }
    }

    fn handle_meetings_key(&mut self, key: KeyEvent) {
        // Sort meetings by date (most recent first) - same as UI
        let mut sorted_meetings = self.data.all_meetings.clone();
        sorted_meetings.sort_by(|a, b| b.start.cmp(&a.start));

        // Apply filter if needed
        let meetings: Vec<Meeting> = if self.data.ui_state.filter_unlinked_only {
            sorted_meetings
                .into_iter()
                .filter(|m| {
                    // Filter out linked meetings
                    let is_unlinked = m.jira_link.is_none();
                    // Filter out declined meetings
                    let is_not_declined = m
                        .my_response_status
                        .as_ref()
                        .map(|s| s != "declined")
                        .unwrap_or(true);
                    is_unlinked && is_not_declined
                })
                .collect()
        } else {
            sorted_meetings
        };

        let max_index = meetings.len().saturating_sub(1);

        // Handle standard navigation keys first
        if helpers::handle_list_navigation(key, &mut self.data.ui_state.selected_meeting_index, max_index) {
            return;
        }

        // Handle meeting-specific keys
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_data();
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.handle_update();
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Auto-link meetings
                self.auto_link_meetings();
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                self.handle_meeting_log();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Toggle filter
                self.data.ui_state.filter_unlinked_only = !self.data.ui_state.filter_unlinked_only;
                self.data.ui_state.selected_meeting_index = 0;
            }
            KeyCode::Delete | KeyCode::Backspace => {
                // Show unlink confirmation
                if let Some(meeting) = meetings.get(self.data.ui_state.selected_meeting_index) {
                    // Only show confirmation if the meeting is actually linked
                    if meeting.jira_link.is_some() {
                        self.unlink_confirmation_meeting_id = Some(meeting.id.clone());
                    }
                }
            }
            KeyCode::Enter => {
                // Link selected meeting
                if let Some(meeting) = meetings.get(self.data.ui_state.selected_meeting_index) {
                    self.link_meeting(meeting.id.clone());
                }
            }
            KeyCode::PageUp => {
                self.data.ui_state.selected_meeting_index = self.data.ui_state.selected_meeting_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.data.ui_state.selected_meeting_index = (self.data.ui_state.selected_meeting_index + 10).min(max_index);
            }
            _ => {}
        }
    }

    fn handle_worklogs_key(&mut self, key: KeyEvent) {
        // Sort worklogs by date (most recent first)
        let mut sorted_worklogs = self.data.all_worklogs.clone();
        sorted_worklogs.sort_by(|a, b| b.started.cmp(&a.started));

        // Apply filter if needed
        let worklogs: Vec<LocalWorklog> = if self.data.ui_state.filter_staged_only {
            sorted_worklogs
                .into_iter()
                .filter(|w| {
                    w.status == LocalWorklogState::Staged || w.status == LocalWorklogState::Created
                })
                .collect()
        } else {
            sorted_worklogs
        };

        let max_index = worklogs.len().saturating_sub(1);

        // Clamp the selected index to valid range
        if self.data.ui_state.selected_worklog_index > max_index {
            self.data.ui_state.selected_worklog_index = max_index;
        }

        // Handle standard navigation keys first
        if helpers::handle_list_navigation(key, &mut self.data.ui_state.selected_worklog_index, max_index) {
            return;
        }

        // Handle worklog-specific keys
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_data();
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.handle_update();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Toggle filter
                self.data.ui_state.filter_staged_only = !self.data.ui_state.filter_staged_only;
                self.data.ui_state.selected_worklog_index = 0;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                    // Ctrl+A: Stage all Created worklogs
                    self.handle_stage_all_worklogs();
                } else {
                    // 'a': Toggle worklog status (Created <-> Staged)
                    if let Some(worklog) = worklogs.get(self.data.ui_state.selected_worklog_index) {
                        self.handle_toggle_worklog_stage(worklog.id.clone());
                    }
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Push all staged worklogs
                self.handle_push_worklogs();
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                // Reset - delete all staged worklogs
                self.handle_reset_worklogs();
            }
            KeyCode::Delete | KeyCode::Backspace => {
                // Delete selected worklog
                if let Some(worklog) = worklogs.get(self.data.ui_state.selected_worklog_index) {
                    self.handle_delete_worklog(worklog.id.clone());
                }
            }
            KeyCode::PageUp => {
                self.data.ui_state.selected_worklog_index = self.data.ui_state.selected_worklog_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.data.ui_state.selected_worklog_index = (self.data.ui_state.selected_worklog_index + 10).min(max_index);
            }
            _ => {}
        }
    }

    fn handle_github_key(&mut self, key: KeyEvent) {
        let filtered_sessions = &self.data.github_sessions;

        // Allow 'u' to work even if list is empty
        match key.code {
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.handle_github_sync();
                return;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if !filtered_sessions.is_empty() {
                    self.handle_create_worklog_from_session();
                }
                return;
            }
            _ => {}
        }

        if filtered_sessions.is_empty() {
            return;
        }

        let max_index = filtered_sessions.len().saturating_sub(1);

        // Ensure index is within bounds before navigation
        if self.data.ui_state.selected_github_session_index > max_index {
            self.data.ui_state.selected_github_session_index = max_index;
        }

        match key.code {
            KeyCode::Up => {
                self.data.ui_state.selected_github_session_index =
                    self.data.ui_state.selected_github_session_index.saturating_sub(1);
            }
            KeyCode::Down => {
                if self.data.ui_state.selected_github_session_index < max_index {
                    self.data.ui_state.selected_github_session_index += 1;
                }
            }
            KeyCode::Home => {
                self.data.ui_state.selected_github_session_index = 0;
            }
            KeyCode::End => {
                self.data.ui_state.selected_github_session_index = max_index;
            }
            KeyCode::PageUp => {
                self.data.ui_state.selected_github_session_index =
                    self.data.ui_state.selected_github_session_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.data.ui_state.selected_github_session_index =
                    (self.data.ui_state.selected_github_session_index + 10).min(max_index);
            }
            _ => {}
        }
    }

    fn handle_issue_selection_key(&mut self, key: KeyEvent) {
        let state = match &mut self.issue_selection_state {
            Some(s) => s,
            None => return,
        };

        // Filter issues based on search query
        let filtered_issues: Vec<&wtf_lib::models::data::Issue> = state
            .all_issues
            .iter()
            .filter(|issue| {
                if state.search_query.is_empty() {
                    true
                } else {
                    let query_lower = state.search_query.to_lowercase();
                    issue.key.to_lowercase().contains(&query_lower)
                        || issue.summary.to_lowercase().contains(&query_lower)
                }
            })
            .collect();

        let max_index = filtered_issues.len().saturating_sub(1);

        match key.code {
            KeyCode::Esc => {
                // If search is active, clear it; otherwise cancel
                if !state.search_query.is_empty() {
                    state.search_query.clear();
                    state.selected_issue_index = 0;
                } else {
                    self.issue_selection_state = None;
                }
            }
            KeyCode::Enter => {
                if let Some(issue) = filtered_issues.get(state.selected_issue_index) {
                    // Use selected issue from list
                    let issue_key = issue.key.clone();
                    let meeting_id = state.meeting_id.clone();
                    if let Some(mut meeting) =
                        MeetingsService::get_meeting_by_id(meeting_id.clone())
                    {
                        meeting.jira_link = Some(issue_key.clone());
                        MeetingsService::save(&meeting);
                        logger::log(format!("✅ Linked meeting to {}", issue_key));
                        self.refresh_data();

                        // If in wizard manual linking, track and advance
                        if let Some(wizard) = &mut self.wizard_state {
                            if let WizardStep::ManualLinking {
                                ref mut unlinked_meetings,
                                ref mut selected_index,
                            } = wizard.current_step
                            {
                                wizard.summary.meetings_manually_linked += 1;
                                wizard
                                    .rollback_log
                                    .original_meeting_links
                                    .insert(meeting_id.clone(), None);
                                wizard
                                    .rollback_log
                                    .linked_meeting_ids
                                    .push(meeting_id.clone());

                                // Remove this meeting from unlinked list and auto-advance
                                let mid_clone = meeting_id.clone();
                                unlinked_meetings.retain(|m| m.id != mid_clone);

                                if unlinked_meetings.is_empty() {
                                    // All done, advance to next step
                                    logger::log("✅ All meetings linked!".to_string());
                                    wizard.completed_steps.insert(2);
                                    wizard.current_step = WizardStep::CreatingMeetingWorklogs;
                                    self.wizard_step_create_meeting_worklogs();
                                } else {
                                    // More to link, keep index in bounds
                                    if *selected_index >= unlinked_meetings.len() {
                                        *selected_index = unlinked_meetings.len() - 1;
                                    }
                                }
                            }
                        }
                    }
                    self.issue_selection_state = None;
                } else if !state.search_query.is_empty() {
                    // No match found in database, try fetching from Jira
                    let issue_key = state.search_query.to_uppercase();
                    let meeting_id = state.meeting_id.clone();
                    self.issue_selection_state = None;

                    // Spawn background task to fetch issue
                    logger::log(format!("🔍 Fetching {} from Jira...", issue_key));
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let key_clone = issue_key.clone();
                    std::thread::spawn(move || {
                        let runtime = tokio::runtime::Runtime::new().unwrap();
                        let result = runtime.block_on(async {
                            let jira_client = wtf_lib::client::jira_client::JiraClient::create();
                            jira_client.get_issue(&key_clone).await
                        });
                        sender.send(result).ok();
                    });

                    // Poll for result
                    match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
                        Ok(Ok(jira_issue)) => {
                            // Save issue to database
                            let issue = wtf_lib::models::data::Issue {
                                key: jira_issue.key.clone(),
                                id: jira_issue.id,
                                created: jira_issue.fields.created,
                                status: jira_issue.fields.status.name,
                                summary: jira_issue.fields.summary,
                            };
                            wtf_lib::services::jira_service::IssueService::save_issue(&issue);

                            // Link meeting
                            if let Some(mut meeting) =
                                MeetingsService::get_meeting_by_id(meeting_id.clone())
                            {
                                meeting.jira_link = Some(issue_key.clone());
                                MeetingsService::save(&meeting);
                                logger::log(format!(
                                    "✅ Fetched and linked meeting to {}",
                                    issue_key
                                ));
                                self.refresh_data();

                                // If in wizard manual linking, track and advance
                                if let Some(wizard) = &mut self.wizard_state {
                                    if let WizardStep::ManualLinking {
                                        ref mut unlinked_meetings,
                                        ref mut selected_index,
                                    } = wizard.current_step
                                    {
                                        wizard.summary.meetings_manually_linked += 1;
                                        wizard
                                            .rollback_log
                                            .original_meeting_links
                                            .insert(meeting_id.clone(), None);
                                        wizard
                                            .rollback_log
                                            .linked_meeting_ids
                                            .push(meeting_id.clone());

                                        // Remove this meeting from unlinked list
                                        let mid_clone = meeting_id.clone();
                                        unlinked_meetings.retain(|m| m.id != mid_clone);

                                        if unlinked_meetings.is_empty() {
                                            logger::log("✅ All meetings linked!".to_string());
                                            wizard.completed_steps.insert(2);
                                            wizard.current_step =
                                                WizardStep::CreatingMeetingWorklogs;
                                            self.wizard_step_create_meeting_worklogs();
                                        } else {
                                            if *selected_index >= unlinked_meetings.len() {
                                                *selected_index = unlinked_meetings.len() - 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            logger::log(format!(
                                "❌ Issue {} not found in Jira: {:?}",
                                issue_key, e
                            ));
                        }
                        Err(_) => {
                            logger::log(format!("⏱️ Timeout fetching {}", issue_key));
                        }
                    }
                } else {
                    self.issue_selection_state = None;
                }
            }
            KeyCode::Backspace => {
                // Remove last character from search
                state.search_query.pop();
                state.selected_issue_index = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                // Add character to search (skip if k/j for navigation)
                if c != 'k' && c != 'j' {
                    state.search_query.push(c);
                    state.selected_issue_index = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected_issue_index > 0 {
                    state.selected_issue_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected_issue_index < max_index {
                    state.selected_issue_index += 1;
                }
            }
            KeyCode::Home => {
                state.selected_issue_index = 0;
            }
            KeyCode::End => {
                state.selected_issue_index = max_index;
            }
            KeyCode::PageUp => {
                state.selected_issue_index = state.selected_issue_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                state.selected_issue_index = (state.selected_issue_index + 10).min(max_index);
            }
            _ => {}
        }
    }

    fn handle_unlink_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                // Confirm unlink
                if let Some(meeting_id) = self.unlink_confirmation_meeting_id.take() {
                    self.unlink_meeting(meeting_id);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // Cancel unlink
                self.unlink_confirmation_meeting_id = None;
            }
            _ => {}
        }
    }

    fn handle_sprint_follow_key(&mut self, key: KeyEvent) {
        let state = match &mut self.sprint_follow_state {
            Some(s) => s,
            None => return,
        };

        // Filter sprints based on search query
        let filtered_sprints: Vec<&wtf_lib::models::data::Sprint> = state
            .all_sprints
            .iter()
            .filter(|sprint| {
                if state.search_query.is_empty() {
                    true
                } else {
                    let query_lower = state.search_query.to_lowercase();
                    sprint.name.to_lowercase().contains(&query_lower)
                        || format!("{}", sprint.id).contains(&query_lower)
                }
            })
            .collect();

        let max_index = filtered_sprints.len().saturating_sub(1);

        match key.code {
            KeyCode::Esc => {
                // If search is active, clear it; otherwise close popup
                if !state.search_query.is_empty() {
                    state.search_query.clear();
                    state.selected_index = 0;
                } else {
                    self.sprint_follow_state = None;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected_index > 0 {
                    state.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected_index < max_index {
                    state.selected_index += 1;
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Enter => {
                // Toggle follow/unfollow
                if let Some(sprint) = filtered_sprints.get(state.selected_index) {
                    let sprint_id_num = sprint.id;
                    let sprint_id = sprint_id_num.to_string();
                    let was_followed = sprint.followed;
                    let sprint_name = sprint.name.clone();

                    // Drop the borrow before we mutate
                    drop(filtered_sprints);

                    let result = if was_followed {
                        wtf_lib::services::jira_service::JiraService::unfollow_sprint(&sprint_id)
                    } else {
                        wtf_lib::services::jira_service::JiraService::follow_sprint(&sprint_id)
                    };

                    match result {
                        Ok(_) => {
                            let action = if was_followed {
                                "Unfollowed"
                            } else {
                                "Followed"
                            };
                            logger::log(format!("✅ {} sprint: {}", action, sprint_name));

                            // Update local state
                            if let Some(s) =
                                state.all_sprints.iter_mut().find(|s| s.id == sprint_id_num)
                            {
                                s.followed = !was_followed;
                            }
                            self.refresh_data();
                        }
                        Err(e) => {
                            logger::log(format!("❌ Failed to toggle sprint: {}", e));
                        }
                    }
                } else {
                    drop(filtered_sprints);
                }
            }
            KeyCode::Char(c) => {
                // Don't add 'a' or 'A' to search (they toggle follow)
                if c != 'a' && c != 'A' && c != 'k' && c != 'K' && c != 'j' && c != 'J' {
                    state.search_query.push(c);
                    state.selected_index = 0; // Reset selection on search
                }
            }
            KeyCode::Backspace => {
                state.search_query.pop();
                state.selected_index = 0;
            }
            _ => {}
        }
    }

    fn handle_worklog_creation_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Full - create worklog with full duration
                if let Some(state) = self.worklog_creation_confirmation.take() {
                    match state.source {
                        WorklogSource::GitHub { session_id, .. } => {
                            // Find the session and create worklogs
                            if let Some(session) = self
                                .data
                                .github_sessions
                                .iter()
                                .find(|s| s.id == session_id)
                            {
                                let session_clone = session.clone();
                                let jira_issues = session_clone.get_jira_issues();
                                let duration_seconds = session_clone.duration_seconds;
                                let time_per_issue = if jira_issues.len() > 1 {
                                    duration_seconds / jira_issues.len() as i64
                                } else {
                                    duration_seconds
                                };
                                let created = self.create_worklogs_from_session(
                                    &session_clone,
                                    &jira_issues,
                                    time_per_issue,
                                );

                                // If in wizard mode, track and advance
                                if let Some(wizard) = &mut self.wizard_state {
                                    wizard.summary.worklogs_from_github += created;
                                    wizard.summary.total_hours +=
                                        (time_per_issue * created as i64) as f64 / 3600.0;
                                }

                                self.wizard_advance_github_session();
                            }
                        }
                        WorklogSource::Meeting { .. } => {
                            // TODO: Handle meeting worklog creation
                            logger::log(
                                "Meeting worklog creation not yet implemented in confirmation"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Partial - create worklog with only remaining time to daily limit
                if let Some(state) = self.worklog_creation_confirmation.take() {
                    let suggested_hours = state.suggested_hours();
                    if suggested_hours <= 0.0 {
                        logger::log(
                            "⚠️  Already at or over daily limit - cannot create partial worklog"
                                .to_string(),
                        );
                        return;
                    }

                    let suggested_seconds = (suggested_hours * 3600.0) as i64;

                    match state.source {
                        WorklogSource::GitHub { session_id, .. } => {
                            if let Some(session) = self
                                .data
                                .github_sessions
                                .iter()
                                .find(|s| s.id == session_id)
                            {
                                let session_clone = session.clone();
                                let jira_issues = session_clone.get_jira_issues();
                                // For partial, use the suggested time
                                let created = self.create_worklogs_from_session(
                                    &session_clone,
                                    &jira_issues,
                                    suggested_seconds,
                                );

                                // If in wizard mode, track and advance
                                if let Some(wizard) = &mut self.wizard_state {
                                    wizard.summary.worklogs_from_github += created;
                                    wizard.summary.total_hours +=
                                        (suggested_seconds * created as i64) as f64 / 3600.0;
                                }

                                self.wizard_advance_github_session();
                            }
                        }
                        WorklogSource::Meeting { .. } => {
                            logger::log(
                                "Meeting worklog creation not yet implemented in confirmation"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Esc => {
                // Skip - cancel worklog creation
                self.worklog_creation_confirmation = None;
                logger::log("⏭️  Worklog creation skipped".to_string());

                // If in wizard mode, advance to next session
                self.wizard_advance_github_session();
            }
            _ => {}
        }
    }

    fn handle_gap_fill_issue_selection_key(&mut self, key: KeyEvent) {
        if let Some(state) = &mut self.gap_fill_state {
            // Apply search filter
            let filtered_issues: Vec<_> = state
                .all_issues
                .iter()
                .filter(|issue| {
                    if state.search_query.is_empty() {
                        true
                    } else {
                        let query_lower = state.search_query.to_lowercase();
                        issue.key.to_lowercase().contains(&query_lower)
                            || issue.summary.to_lowercase().contains(&query_lower)
                    }
                })
                .collect();

            let max_index = filtered_issues.len().saturating_sub(1);

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if state.selected_issue_index > 0 {
                        state.selected_issue_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if state.selected_issue_index < max_index {
                        state.selected_issue_index += 1;
                    }
                }
                KeyCode::Enter => {
                    // Select the issue
                    if let Some(&issue) = filtered_issues.get(state.selected_issue_index) {
                        let sprint_id = state.sprint_id;
                        let issue_id = issue.key.clone();

                        // Remove gap fill state
                        self.gap_fill_state = None;

                        // Calculate gaps for the sprint
                        if let Some(sprint) =
                            self.data.all_sprints.iter().find(|s| s.id == sprint_id)
                        {
                            if let (Some(start), Some(end)) = (sprint.start, sprint.end) {
                                let gaps = LocalWorklogService::find_gap_days(
                                    start.date_naive(),
                                    end.date_naive(),
                                    self.data.daily_hours_limit,
                                    6.0, // Skip days already over 6h
                                );

                                if gaps.is_empty() {
                                    logger::log(
                                        "✓ No gaps to fill - all workdays are substantially logged"
                                            .to_string(),
                                    );
                                    return;
                                }

                                // Show confirmation popup
                                self.gap_fill_confirmation = Some(GapFillConfirmation {
                                    _sprint_id: sprint_id,
                                    sprint_name: sprint.name.clone(),
                                    issue_id,
                                    gaps,
                                });
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    self.gap_fill_state = None;

                    // If in wizard mode, skip gap filling and advance to review
                    if let Some(wizard) = &mut self.wizard_state {
                        wizard.completed_steps.insert(5); // Step 5 complete (skipped)
                        logger::log(
                            "⏭️  Wizard: Skipping gap filling, advancing to review...".to_string(),
                        );
                        self.wizard_step_review();
                    } else {
                        logger::log("⏭️  Gap filling cancelled".to_string());
                    }
                }
                KeyCode::Char(c) => {
                    state.search_query.push(c);
                    state.selected_issue_index = 0; // Reset selection on search
                }
                KeyCode::Backspace => {
                    state.search_query.pop();
                    state.selected_issue_index = 0;
                }
                _ => {}
            }
        }
    }

    fn handle_gap_fill_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                // Confirm and create worklogs
                if let Some(confirmation) = self.gap_fill_confirmation.take() {
                    let mut created_count = 0;
                    let total_hours: f64 = confirmation.gaps.iter().map(|(_, h)| h).sum();

                    for (date, hours_to_add) in &confirmation.gaps {
                        // Create worklog at noon (12:00) for that day
                        use chrono::{NaiveTime, TimeZone, Utc};
                        let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
                        let datetime = date.and_time(noon);
                        let datetime_utc = Utc.from_utc_datetime(&datetime);

                        let seconds = (hours_to_add * 3600.0) as i64;

                        let worklog = LocalWorklogService::create_new_local_worklogs(
                            datetime_utc,
                            seconds,
                            &confirmation.issue_id,
                            Some("Tech activities"),
                            None,
                        );

                        // Track for wizard rollback
                        if let Some(wizard) = &mut self.wizard_state {
                            wizard.rollback_log.created_worklog_ids.push(worklog.id);
                        }

                        created_count += 1;
                    }

                    logger::log(format!(
                        "✅ Created {} worklogs ({:.1}h) for {} in {}",
                        created_count, total_hours, confirmation.issue_id, confirmation.sprint_name
                    ));
                    log_chronie_message("gap_filling", "🧙 Chronie:");
                    self.refresh_data();

                    // If in wizard mode, update summary and advance
                    if let Some(wizard) = &mut self.wizard_state {
                        wizard.summary.worklogs_from_gaps = created_count;
                        wizard.summary.total_hours += total_hours;
                        wizard.completed_steps.insert(5); // Step 5 complete
                        logger::log(
                            "⏭️  Wizard: Gap filling complete, advancing to review...".to_string(),
                        );
                        self.wizard_step_review();
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // Cancel
                self.gap_fill_confirmation = None;

                // If in wizard, still advance but without filling gaps
                if let Some(wizard) = &mut self.wizard_state {
                    wizard.completed_steps.insert(5); // Step 5 complete (skipped)
                    logger::log(
                        "⏭️  Wizard: Skipping gap filling, advancing to review...".to_string(),
                    );
                    self.wizard_step_review();
                } else {
                    logger::log("⏭️  Gap filling cancelled".to_string());
                }
            }
            _ => {}
        }
    }

    fn handle_wizard_cancel_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // User confirmed cancellation - perform rollback
                logger::log("⚠️  Wizard cancelled, rolling back...".to_string());
                self.wizard_rollback();
                self.wizard_cancel_confirmation = None;
                self.wizard_state = None;
                self.refresh_data();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // User declined, continue wizard
                self.wizard_cancel_confirmation = None;
            }
            _ => {}
        }
    }

    fn handle_wizard_manual_linking_key(&mut self, key: KeyEvent) {
        // Extract meeting ID if we need to link
        let meeting_id_to_link = if let Some(WizardState {
            current_step:
                WizardStep::ManualLinking {
                    ref unlinked_meetings,
                    ref selected_index,
                },
            ..
        }) = self.wizard_state
        {
            match key.code {
                KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Enter => {
                    unlinked_meetings.get(*selected_index).map(|m| m.id.clone())
                }
                _ => None,
            }
        } else {
            None
        };

        // Handle navigation within the borrow scope
        if let Some(WizardState {
            current_step:
                WizardStep::ManualLinking {
                    ref mut unlinked_meetings,
                    ref mut selected_index,
                },
            ..
        }) = self.wizard_state
        {
            let max_index = unlinked_meetings.len().saturating_sub(1);

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_index > 0 {
                        *selected_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected_index < max_index {
                        *selected_index += 1;
                    }
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    // Skip remaining - will be handled after borrow scope
                }
                _ => {}
            }
        }

        // Now handle actions that need mutable self
        if let Some(meeting_id) = meeting_id_to_link {
            self.link_meeting(meeting_id);
        } else if matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S')) {
            // Skip remaining and advance to next step
            logger::log("ℹ️  Skipping remaining unlinked meetings".to_string());
            if let Some(wizard) = &mut self.wizard_state {
                wizard.completed_steps.insert(2); // Step 2 complete
                wizard.current_step = WizardStep::CreatingMeetingWorklogs;
            }
            self.wizard_step_create_meeting_worklogs();
        }
    }

    fn handle_history_key(&mut self, key: KeyEvent) {
        // If showing revert confirmation popup, handle that first
        if let Some(ref mut state) = self.revert_confirmation_state {
            // Don't handle input if reverting is in progress
            if state.reverting {
                // Only allow Escape to potentially cancel (though we won't actually cancel the operation)
                if matches!(key.code, KeyCode::Esc) {
                    // Don't cancel while reverting
                }
                return;
            }

            match key.code {
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                    state.user_input.push(c);
                }
                KeyCode::Backspace => {
                    state.user_input.pop();
                }
                KeyCode::Enter => {
                    // Get the expected hours
                    if let Some(history) = self
                        .data
                        .worklog_history
                        .iter()
                        .find(|h| h.id == state.history_id)
                    {
                        let worklogs: Vec<_> = history
                            .local_worklogs_id
                            .iter()
                            .filter_map(|wid| LocalWorklogService::get_worklog(wid))
                            .collect();
                        let total_hours = worklogs.iter().map(|w| w.time_spent_seconds).sum::<i64>()
                            as f64
                            / 3600.0;

                        // Parse user input and check if it matches
                        if let Ok(user_hours) = state.user_input.parse::<f64>() {
                            // Allow some tolerance for rounding (0.05 hours = 3 minutes)
                            if (user_hours - total_hours).abs() < 0.05 {
                                // Confirm revert
                                let history_id = state.history_id.clone();
                                self.revert_history(history_id);
                                // Don't close the popup - it will show spinner now
                            } else {
                                logger::log(format!(
                                    "❌ Incorrect hours entered. Expected {:.1}, got {:.1}",
                                    total_hours, user_hours
                                ));
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    // Cancel revert
                    self.revert_confirmation_state = None;
                }
                _ => {}
            }
            return;
        }

        // Normal navigation
        let max_index = if self.data.worklog_history.is_empty() {
            0
        } else {
            self.data.worklog_history.len() - 1
        };

        // Handle standard navigation keys first
        if helpers::handle_list_navigation(key, &mut self.data.ui_state.selected_history_index, max_index) {
            return;
        }

        // Handle history-specific keys
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                // Collapse current item if expanded
                if let Some(history) = self.data.worklog_history.get(self.data.ui_state.selected_history_index) {
                    self.data.ui_state.expanded_history_ids.remove(&history.id);
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                // Expand current item
                if let Some(history) = self.data.worklog_history.get(self.data.ui_state.selected_history_index) {
                    self.data.ui_state.expanded_history_ids.insert(history.id.clone());
                }
            }
            KeyCode::Enter => {
                // Toggle expand/collapse
                if let Some(history) = self.data.worklog_history.get(self.data.ui_state.selected_history_index) {
                    if self.data.ui_state.expanded_history_ids.contains(&history.id) {
                        self.data.ui_state.expanded_history_ids.remove(&history.id);
                    } else {
                        self.data.ui_state.expanded_history_ids.insert(history.id.clone());
                    }
                }
            }
            KeyCode::Delete => {
                // Show revert confirmation (reverts in Jira)
                if let Some(history) = self.data.worklog_history.get(self.data.ui_state.selected_history_index) {
                    self.revert_confirmation_state = Some(RevertConfirmationState {
                        history_id: history.id.clone(),
                        user_input: String::new(),
                        reverting: false,
                    });
                }
            }
            KeyCode::Char('D') => {
                // Delete history from DB without Jira revert (Shift+D)
                if let Some(history) = self.data.worklog_history.get(self.data.ui_state.selected_history_index) {
                    match LocalWorklogService::delete_history_from_db(&history.id) {
                        Ok(()) => {
                            logger::log(format!("🗑️ Deleted history entry from database (worklogs remain in Jira)"));
                            self.refresh_data();
                        }
                        Err(e) => {
                            logger::log(format!("❌ Failed to delete history: {}", e));
                        }
                    }
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // Create history for pushed worklogs without history (recovery function)
                LocalWorklogService::create_history_for_pushed_worklogs();
                logger::log("📝 Created recovery history for unhistorized pushed worklogs".to_string());
                self.refresh_data();
            }
            KeyCode::PageUp => {
                self.data.ui_state.selected_history_index = self.data.ui_state.selected_history_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.data.ui_state.selected_history_index = (self.data.ui_state.selected_history_index + 10).min(max_index);
            }
            _ => {}
        }
    }

    fn revert_history(&mut self, history_id: String) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.revert_receiver = Some(receiver);

        // Spawn thread to do the async work
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                // Get the history entry first
                if let Some(history) = LocalWorklogService::get_worklog_history(&history_id) {
                    info!("Reverting worklog history: {}", history_id);
                    LocalWorklogService::revert_worklog_history(&history).await;
                    info!("Successfully reverted worklog history: {}", history_id);
                    Ok(())
                } else {
                    error!("Failed to find worklog history: {}", history_id);
                    Err("History not found".to_string())
                }
            });

            let _ = sender.send(result);
        });

        // Set reverting flag to show spinner
        if let Some(state) = &mut self.revert_confirmation_state {
            state.reverting = true;
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        use crate::tui::ui::tabs::settings::{FIELD_COUNT, get_field_value};

        if self.data.ui_state.settings_editing {
            match key.code {
                KeyCode::Esc => {
                    self.data.ui_state.settings_editing = false;
                    self.data.ui_state.settings_input_buffer.clear();
                }
                KeyCode::Enter => {
                    self.apply_settings_field_edit();
                }
                KeyCode::Backspace => {
                    self.data.ui_state.settings_input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.data.ui_state.settings_input_buffer.push(c);
                }
                _ => {}
            }
        } else {
            // Clear status on navigation
            match key.code {
                KeyCode::Up | KeyCode::Down => {
                    self.data.ui_state.settings_status = None;
                }
                _ => {}
            }

            match key.code {
                KeyCode::Up => {
                    if self.data.ui_state.settings_selected_field > 0 {
                        self.data.ui_state.settings_selected_field -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.data.ui_state.settings_selected_field < FIELD_COUNT - 1 {
                        self.data.ui_state.settings_selected_field += 1;
                    }
                }
                KeyCode::Enter => {
                    // Populate input buffer with current value and enter edit mode
                    let field_idx = self.data.ui_state.settings_selected_field;
                    let current = get_field_value(field_idx, &self.data.config.clone());
                    self.data.ui_state.settings_input_buffer = current;
                    self.data.ui_state.settings_editing = true;
                    self.data.ui_state.settings_status = None;
                }
                KeyCode::Char('v') | KeyCode::Char('V') => {
                    let field_idx = self.data.ui_state.settings_selected_field;
                    let revealed = &mut self.data.ui_state.settings_show_sensitive;
                    if revealed.contains(&field_idx) {
                        revealed.remove(&field_idx);
                    } else {
                        revealed.insert(field_idx);
                    }
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.save_settings();
                }
                _ => {}
            }
        }
    }

    fn apply_settings_field_edit(&mut self) {
        use wtf_lib::config::{GoogleConfig, SensitiveString};

        let field_idx = self.data.ui_state.settings_selected_field;
        let value = self.data.ui_state.settings_input_buffer.clone();
        let config = &mut self.data.config;

        match field_idx {
            0 => config.jira.base_url = value,
            1 => config.jira.username = value,
            2 => config.jira.api_token = SensitiveString::new(value),
            3 => {
                config.jira.auto_follow_sprint_pattern =
                    if value.is_empty() { None } else { Some(value) }
            }
            4 => {
                config.github.organisation =
                    if value.is_empty() { None } else { Some(value) }
            }
            5 => {
                if let Some(ref mut g) = config.google {
                    g.credentials_path = value;
                } else if !value.is_empty() {
                    config.google = Some(GoogleConfig {
                        credentials_path: value,
                        token_cache_path: String::new(),
                    });
                }
            }
            6 => {
                if let Some(ref mut g) = config.google {
                    g.token_cache_path = value;
                } else if !value.is_empty() {
                    config.google = Some(GoogleConfig {
                        credentials_path: String::new(),
                        token_cache_path: value,
                    });
                }
            }
            7 => {
                if let Ok(hours) = value.parse::<f64>() {
                    config.worklog.daily_hours_limit = hours;
                }
            }
            _ => {}
        }

        self.data.ui_state.settings_editing = false;
        self.data.ui_state.settings_input_buffer.clear();
        self.data.ui_state.settings_dirty = true;
    }

    fn save_settings(&mut self) {
        match self.data.config.save() {
            Ok(()) => {
                self.data.ui_state.settings_dirty = false;
                self.data.ui_state.settings_status = Some("✓ Settings saved successfully".to_string());
                logger::log("⚙️  Settings saved".to_string());
            }
            Err(e) => {
                self.data.ui_state.settings_status = Some(format!("✗ Save failed: {}", e));
                logger::log(format!("❌ Failed to save settings: {}", e));
            }
        }
    }

    fn refresh_data(&mut self) {
        // Avoid spawning multiple refresh threads
        if self.data_refresh_receiver.is_some() {
            return;
        }

        let (tx, rx) = channel();
        self.data_refresh_receiver = Some(rx);

        // Preserve current UI state when refreshing
        let ui_state = self.data.ui_state.clone();

        // Spawn background thread for data collection to avoid blocking UI
        thread::spawn(move || {
            let data = TuiData::collect_with_ui_state(ui_state);
            let _ = tx.send(data);
        });
    }

    // ============================================================================
    // Key Sequence Tracking (for secret achievements)
    // ============================================================================
    
    fn track_key_sequence(&mut self, key: &KeyEvent) {
        // Convert key to string representation
        let key_str = match key.code {
            KeyCode::Up => "up",
            KeyCode::Down => "down",
            KeyCode::Left => "left",
            KeyCode::Right => "right",
            KeyCode::Char(c) => {
                // Use lowercase for consistency
                return self.track_key_str(&c.to_lowercase().to_string());
            }
            _ => return, // Ignore other keys
        };
        
        self.track_key_str(key_str);
    }
    
    fn track_key_str(&mut self, key_str: &str) {
        // Add key to buffer
        self.key_sequence_buffer.push_back(key_str.to_string());
        
        // Keep buffer size limited (max 20 keys for longest sequences)
        if self.key_sequence_buffer.len() > 20 {
            self.key_sequence_buffer.pop_front();
        }
        
        // Check for known sequences
        self.check_key_sequences();
    }
    
    fn check_key_sequences(&mut self) {
        // Load sequences from PNG metadata (use local APP_BRANDING)
        let Some(branding) = APP_BRANDING.as_ref() else {
            return;
        };
        
        let Some(secrets) = &branding.secrets else {
            return;
        };
        
        // Check each sequence from PNG
        for (name, sequence_def) in &secrets.sequences {
            let keys: Vec<&str> = sequence_def.keys.iter().map(|s| s.as_str()).collect();
            
            if self.matches_sequence(&keys) {
                // Publish event
                self.event_bus.publish(AppEvent::SecretSequenceTriggered { 
                    sequence_name: name.clone() 
                });
                
                // Log for debugging
                logger::log(format!("🔓 Secret sequence detected: {}", name));
                
                // Clear buffer to avoid re-triggering
                self.key_sequence_buffer.clear();
                
                // Process events immediately
                let mut event_bus = std::mem::take(&mut self.event_bus);
                event_bus.process_events(self);
                self.event_bus = event_bus;
                
                break;
            }
        }
    }
    
    fn matches_sequence(&self, sequence: &[&str]) -> bool {
        if self.key_sequence_buffer.len() < sequence.len() {
            return false;
        }
        
        // Check if last N keys match the sequence
        let start = self.key_sequence_buffer.len() - sequence.len();
        let recent_keys: Vec<String> = self.key_sequence_buffer
            .iter()
            .skip(start)
            .cloned()
            .collect();
        
        recent_keys.iter()
            .zip(sequence.iter())
            .all(|(a, b)| a == b)
    }

    fn load_logo_image() -> Option<image::DynamicImage> {
        // Load logo from embedded bytes
        const LOGO_BYTES: &[u8] = include_bytes!("../../../doc/assets/logo.png");

        // Decode image from bytes
        image::load_from_memory(LOGO_BYTES).ok()
    }
}

// Helper function to get application branding text
pub fn get_branding_text(category: &str) -> Option<String> {
    if let Some(branding) = APP_BRANDING.as_ref() {
        // Debug: show what categories we have
        let categories = branding.get_category_names();
        crate::logger::debug(format!("Available branding categories: {:?}", categories));
        crate::logger::debug(format!("Looking for category: '{}'", category));
        
        branding.get_text(category).map(|s| s.to_string())
    } else {
        crate::logger::debug("APP_BRANDING is None!".to_string());
        None
    }
}

/// Get Chronie message if user has unlocked the ability to hear from Chronie
/// Returns None if required achievement is not unlocked
pub fn get_chronie_message(category: &str) -> Option<String> {
    use wtf_lib::models::achievement::Achievement;
    use wtf_lib::services::achievement_service::AchievementService;
    
    // Map message categories to required achievements
    let required_achievement = match category {
        // Friend-tier messages (require secret achievement)
        "secret" | "friend" => Achievement::ChroniesFriend,
        
        // Apprentice-tier messages (require wizard completion)
        "startup" | "random" | "overwork" | "wizard_complete" => Achievement::ChroniesApprentice,
        
        // Default: require apprentice level
        _ => Achievement::ChroniesApprentice,
    };
    
    // Check if user has unlocked the required achievement
    if !AchievementService::is_unlocked(required_achievement) {
        return None;
    }
    
    // User has unlocked Chronie! Get the message
    get_branding_text(category)
}

/// Log a Chronie message if user has unlocked the ability to hear from Chronie
/// Handles achievement check and formatting automatically
pub fn log_chronie_message(category: &str, prefix: &str) {
    use wtf_lib::models::achievement::Achievement;
    use wtf_lib::services::achievement_service::AchievementService;
    
    // Debug: Check what's happening
    let required = match category {
        "secret" | "friend" => Achievement::ChroniesFriend,
        _ => Achievement::ChroniesApprentice,
    };
    
    let is_unlocked = AchievementService::is_unlocked(required);
    crate::logger::debug(format!("Chronie check - category: {}, required: {:?}, unlocked: {}", 
        category, required, is_unlocked));
    
    if let Some(msg) = get_chronie_message(category) {
        crate::logger::log(format!("{} {}", prefix, msg));
    } else {
        crate::logger::debug(format!("No message returned for category: {}", category));
    }
}
