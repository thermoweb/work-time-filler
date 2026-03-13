mod achievement_tracker;
pub mod data;
mod helpers;
mod operations;
mod tab_controller;
pub mod theme;
mod types;
pub mod ui;
mod ui_helpers;
mod wizard;

// Re-export types for public API
pub use types::*;

use achievement_tracker::AchievementTracker;

// Import custom logger macros
use crate::{error, info};

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
use tab_controller::TabController;
use wtf_lib::models::data::LocalWorklogState;
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
        logger::init_logger_with_log_bridge(
            log_collector.clone() as std::sync::Arc<dyn crate::logger::Logger>
        );

        // Chronie's startup greeting (only if ChroniesApprentice is unlocked)
        log_chronie_message("startup", "🧙 Chronie:");

        // Initialize achievement service and run revoke schedule
        let achievement_service = wtf_lib::services::achievement_service::AchievementService::production();
        achievement_service.run_revoke_schedule();

        // Load logo image for About popup
        let about_image = Self::load_logo_image();

        // Initialize EventBus and register subscribers
        let mut event_bus = EventBus::new();
        event_bus.subscribe(Box::new(wizard::WizardEventHandler));
        event_bus.subscribe(Box::new(AchievementTracker));

        // Spawn async version check — result arrives via channel
        let (update_sender, update_receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(wtf_lib::utils::version::check_latest_version());
            let _ = update_sender.send(result);
        });

        Self {
            data: TuiData::collect(),
            achievement_service,
            current_tab: Tab::Sprints,
            sprints_tab: ui::tabs::sprints::SprintsTab,
            achievements_tab: ui::tabs::achievements::AchievementsTab,
            meetings_tab: ui::tabs::meetings::MeetingsTab,
            github_tab: ui::tabs::github::GitHubTab,
            settings_tab: ui::tabs::settings::SettingsTab,
            worklogs_tab: ui::tabs::worklogs::WorklogsTab,
            history_tab: ui::tabs::history::HistoryTab,
            revert_confirmation_state: None,
            worklog_creation_confirmation: None,
            gap_fill_state: None,
            gap_fill_confirmation: None,
            wizard_state: None,
            wizard_cancel_confirmation: None,
            wizard_pre_launch_prompt: None,
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
            update_receiver: Some(update_receiver),
            status_clear_time: None,
            needs_full_clear: false,
            should_quit: false,
            pending_auto_link: false,
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
        self.handle_update_check();
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
                        self.pending_auto_link = true;
                        self.refresh_data();
                        self.fetch_receiver = None;
                        self.status_clear_time = Some(std::time::Instant::now());
                        self.event_bus
                            .publish(AppEvent::FetchComplete(self.data.clone()));
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
                self.event_bus
                    .publish(AppEvent::PushComplete { history_id });
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

                // Auto-link is deferred until data is fresh (triggered after fetch completes)
                if self.pending_auto_link {
                    self.pending_auto_link = false;
                    self.auto_link_meetings();
                }
            }
        }
    }

    fn handle_update_check(&mut self) {
        if let Some(receiver) = &self.update_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.update_receiver = None;
                if let Some(tag) = result {
                    logger::log(format!(
                        "🆕 New version {} available! Update: cargo install --git https://github.com/thermoweb/work-time-filler --locked wtf-cli --force",
                        tag
                    ));
                }
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
            if self.needs_full_clear {
                terminal.clear()?;
                self.needs_full_clear = false;
            }
            let logs = self.log_collector.get_messages();
            terminal.draw(|f| ui::render(f, self, &logs))?;

            // Handle all async operations
            self.handle_async_operations();

            // Poll for keyboard events
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle_key(key);
                    }
                    Event::Resize(_, _) => {
                        terminal.clear()?;
                    }
                    _ => {}
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
                        logger::log(format!(
                            "📋 Logs copied to clipboard! ({} lines)",
                            logs.len()
                        ));
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
            let history_tab = self.history_tab;
            history_tab.handle_key(self, key);
            return;
        }

        // Wizard pre-launch prompt (existing unpushed worklogs detected)
        if self.wizard_pre_launch_prompt.is_some() {
            self.handle_wizard_pre_launch_key(key);
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
                    // If the intro prompt is shown, handle skip/continue
                    let has_intro = self
                        .wizard_state
                        .as_ref()
                        .map(|w| w.github_step_intro.is_some())
                        .unwrap_or(false);
                    if has_intro {
                        match key.code {
                            KeyCode::Esc => {
                                if let Some(wizard) = &mut self.wizard_state {
                                    wizard.github_step_intro = None;
                                    wizard.completed_steps.insert(4);
                                    wizard
                                        .skip_reasons
                                        .insert(4, "user skipped GitHub step".to_string());
                                    logger::log(
                                        "⏭️  Wizard: Skipping GitHub step, advancing to gap fill..."
                                            .to_string(),
                                    );
                                    wizard.current_step = WizardStep::FillingGaps {
                                        selected_issue: None,
                                    };
                                }
                                self.wizard_step_fill_gaps();
                            }
                            KeyCode::Enter | KeyCode::Char(' ') => {
                                if let Some(wizard) = &mut self.wizard_state {
                                    wizard.github_step_intro = None;
                                }
                                self.wizard_process_next_github_session();
                            }
                            _ => {}
                        }
                        return;
                    }
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
            let settings_tab = self.settings_tab;
            settings_tab.handle_key(self, key);
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
        if key.code == KeyCode::Char('l')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
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
                let has_achievements = self.achievement_service.has_any_unlocked();
                self.current_tab = self.current_tab.next(has_achievements);
                self.needs_full_clear = true;
            }
            KeyCode::BackTab => {
                let has_achievements = self.achievement_service.has_any_unlocked();
                self.current_tab = self.current_tab.previous(has_achievements);
                self.needs_full_clear = true;
            }
            KeyCode::Char('1') => {
                self.current_tab = Tab::Sprints;
                self.needs_full_clear = true;
            }
            KeyCode::Char('2') => {
                self.current_tab = Tab::Meetings;
                self.needs_full_clear = true;
            }
            KeyCode::Char('3') => {
                self.current_tab = Tab::Worklogs;
                self.needs_full_clear = true;
            }
            KeyCode::Char('4') => {
                self.current_tab = Tab::GitHub;
                self.needs_full_clear = true;
            }
            KeyCode::Char('5') => {
                self.current_tab = Tab::History;
                self.needs_full_clear = true;
            }
            KeyCode::Char('6') => {
                self.current_tab = Tab::Settings;
                self.needs_full_clear = true;
            }
            KeyCode::Char('7') => {
                // Only allow switching to Achievements if user has unlocked at least one
                if self.achievement_service.has_any_unlocked() {
                    self.current_tab = Tab::Achievements;
                    self.needs_full_clear = true;
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
                let sprints_tab = self.sprints_tab;
                sprints_tab.handle_key(self, key);
            }
            Tab::Meetings => {
                let meetings_tab = self.meetings_tab;
                meetings_tab.handle_key(self, key);
            }
            Tab::Worklogs => {
                let worklogs_tab = self.worklogs_tab;
                worklogs_tab.handle_key(self, key);
            }
            Tab::GitHub => {
                let github_tab = self.github_tab;
                github_tab.handle_key(self, key);
            }
            Tab::History => {
                let history_tab = self.history_tab;
                history_tab.handle_key(self, key);
            }
            Tab::Achievements => {
                let achievements_tab = self.achievements_tab;
                achievements_tab.handle_key(self, key);
            }
            Tab::Settings => {
                let settings_tab = self.settings_tab;
                settings_tab.handle_key(self, key);
            }
        }
    }

    fn render_current_tab(&self, frame: &mut ratatui::Frame, area: &ratatui::layout::Rect) {
        match self.current_tab {
            Tab::Sprints => self.sprints_tab.render(frame, area, &self.data),
            Tab::Meetings => self.meetings_tab.render(frame, area, &self.data),
            Tab::Worklogs => self.worklogs_tab.render(frame, area, &self.data),
            Tab::GitHub => self.github_tab.render(frame, area, &self.data),
            Tab::History => self.history_tab.render(frame, area, &self.data),
            Tab::Achievements => self.achievements_tab.render(frame, area, &self.data),
            Tab::Settings => self.settings_tab.render(frame, area, &self.data),
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
                        let sprints = JiraService::production().get_followed_sprint();
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
                        let sprints = JiraService::production().get_followed_sprint();
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
                        MeetingsService::production().get_meeting_by_id(meeting_id.clone())
                    {
                        meeting.jira_link = Some(issue_key.clone());
                        MeetingsService::production().save(&meeting);
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
                            wtf_lib::services::jira_service::IssueService::production().save_issue(&issue);

                            // Link meeting
                            if let Some(mut meeting) =
                                MeetingsService::production().get_meeting_by_id(meeting_id.clone())
                            {
                                meeting.jira_link = Some(issue_key.clone());
                                MeetingsService::production().save(&meeting);
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
                        wtf_lib::services::jira_service::JiraService::production().unfollow_sprint(&sprint_id)
                    } else {
                        wtf_lib::services::jira_service::JiraService::production().follow_sprint(&sprint_id)
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
                            "⚠️  Already at or over daily limit - skipping GitHub session"
                                .to_string(),
                        );
                        // Still advance so the wizard doesn't get stuck
                        self.wizard_advance_github_session();
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
                                // For partial, divide the suggested total evenly across issues
                                let time_per_issue_partial = if jira_issues.len() > 1 {
                                    suggested_seconds / jira_issues.len() as i64
                                } else {
                                    suggested_seconds
                                };
                                let created = self.create_worklogs_from_session(
                                    &session_clone,
                                    &jira_issues,
                                    time_per_issue_partial,
                                );

                                // If in wizard mode, track and advance
                                if let Some(wizard) = &mut self.wizard_state {
                                    wizard.summary.worklogs_from_github += created;
                                    wizard.summary.total_hours +=
                                        (time_per_issue_partial * created as i64) as f64 / 3600.0;
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
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Skip - cancel worklog creation, advance to next session
                self.worklog_creation_confirmation = None;
                logger::log("⏭️  Worklog creation skipped".to_string());
                self.wizard_advance_github_session();
            }
            KeyCode::Esc => {
                // Cancel wizard
                self.worklog_creation_confirmation = None;
                self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
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
                                let gaps = LocalWorklogService::production().find_gap_days(
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

                    // Esc skips gap filling; use wizard cancel from other steps to abort entirely
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

                        let worklog = LocalWorklogService::production().create_new_local_worklogs(
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
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Skip gap filling, advance to review
                self.gap_fill_confirmation = None;

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
            KeyCode::Esc => {
                // Cancel wizard
                self.gap_fill_confirmation = None;
                if self.wizard_state.is_some() {
                    self.wizard_cancel_confirmation = Some(WizardCancelConfirmation);
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
            KeyCode::Char('k') | KeyCode::Char('K') => {
                // Exit wizard but keep all created worklogs/links staged (no rollback)
                logger::log(
                    "⏹️  Wizard exited, worklogs kept staged for manual handling".to_string(),
                );
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

    fn handle_wizard_pre_launch_key(&mut self, key: KeyEvent) {
        let prompt = match self.wizard_pre_launch_prompt.take() {
            Some(p) => p,
            None => return,
        };
        match key.code {
            KeyCode::Char('k') | KeyCode::Char('K') => {
                // Keep existing worklogs, launch wizard as-is
                logger::log(
                    "▶️  Keeping existing worklogs, starting wizard...".to_string(),
                );
                self.do_launch_wizard(prompt.sprint_id, &prompt.sprint_name);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Reset: delete all unpushed worklogs, then launch
                logger::log("🗑️  Resetting unpushed worklogs before wizard launch...".to_string());
                let to_delete = LocalWorklogService::production()
                    .get_all_local_worklogs_by_status(vec![
                        LocalWorklogState::Created,
                        LocalWorklogState::Staged,
                    ]);
                for wl in &to_delete {
                    LocalWorklogService::production().remove_local_worklog(wl);
                }
                logger::log(format!("🗑️  Deleted {} unpushed worklog(s)", to_delete.len()));
                self.do_launch_wizard(prompt.sprint_id, &prompt.sprint_name);
            }
            KeyCode::Esc => {
                // Abort wizard launch
                logger::log("⚠️  Wizard launch aborted".to_string());
                // prompt was already taken, nothing to restore
            }
            _ => {
                // Put the prompt back; ignore unrecognized keys
                self.wizard_pre_launch_prompt = Some(prompt);
            }
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

    pub(in crate::tui) fn revert_history(&mut self, history_id: String) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.revert_receiver = Some(receiver);

        // Spawn thread to do the async work
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                // Get the history entry first
                if let Some(history) = LocalWorklogService::production().get_worklog_history(&history_id) {
                    info!("Reverting worklog history: {}", history_id);
                    LocalWorklogService::production().revert_worklog_history(&history).await;
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

    pub(in crate::tui) fn apply_settings_field_edit(&mut self) {
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
            4 => config.github.organisation = if value.is_empty() { None } else { Some(value) },
            5 => {
                if let Some(ref mut g) = config.google {
                    g.credentials_path = value;
                } else if !value.is_empty() {
                    config.google = Some(GoogleConfig {
                        credentials_path: value,
                        token_cache_path: String::new(),
                        color_labels: std::collections::HashMap::new(),
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
                        color_labels: std::collections::HashMap::new(),
                    });
                }
            }
            7 => {
                if let Ok(hours) = value.parse::<f64>() {
                    config.worklog.daily_hours_limit = hours;
                }
            }
            8..=18 => {
                use wtf_lib::config::GOOGLE_CALENDAR_EVENT_COLORS;
                let color_name = GOOGLE_CALENDAR_EVENT_COLORS[field_idx - 8].to_string();
                if let Some(ref mut g) = config.google {
                    if value.is_empty() {
                        g.color_labels.remove(&color_name);
                    } else {
                        g.color_labels.insert(color_name, value);
                    }
                }
            }
            _ => {}
        }

        self.data.ui_state.settings_editing = false;
        self.data.ui_state.settings_input_buffer.clear();
        self.data.ui_state.settings_dirty = true;
    }

    pub(in crate::tui) fn save_settings(&mut self) {
        match self.data.config.save() {
            Ok(()) => {
                self.data.ui_state.settings_dirty = false;
                self.data.ui_state.settings_status =
                    Some("✓ Settings saved successfully".to_string());
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
                    sequence_name: name.clone(),
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
        let recent_keys: Vec<String> = self
            .key_sequence_buffer
            .iter()
            .skip(start)
            .cloned()
            .collect();

        recent_keys.iter().zip(sequence.iter()).all(|(a, b)| a == b)
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

    let required_achievement = match category {
        "secret" | "friend" => Achievement::ChroniesFriend,
        "startup" | "random" | "overwork" | "wizard_complete" => Achievement::ChroniesApprentice,
        _ => Achievement::ChroniesApprentice,
    };

    if !AchievementService::production().is_unlocked(required_achievement) {
        return None;
    }

    get_branding_text(category)
}

/// Log a Chronie message if user has unlocked the ability to hear from Chronie
/// Handles achievement check and formatting automatically
pub fn log_chronie_message(category: &str, prefix: &str) {
    use wtf_lib::models::achievement::Achievement;
    use wtf_lib::services::achievement_service::AchievementService;

    let required = match category {
        "secret" | "friend" => Achievement::ChroniesFriend,
        _ => Achievement::ChroniesApprentice,
    };

    let is_unlocked = AchievementService::production().is_unlocked(required);
    crate::logger::debug(format!(
        "Chronie check - category: {}, required: {:?}, unlocked: {}",
        category, required, is_unlocked
    ));

    if let Some(msg) = get_chronie_message(category) {
        crate::logger::log(format!("{} {}", prefix, msg));
    } else {
        crate::logger::debug(format!("No message returned for category: {}", category));
    }
}
