use crate::commands::Command;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};
use inquire::{Confirm, CustomUserError, Password, Select, Text};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use wtf_lib::config::{
    Config, GithubConfig, GoogleConfig, JiraConfig, SensitiveString, WorklogConfig,
};
use wtf_lib::models::data::{Board, Sprint};
use wtf_lib::services::jira_service::JiraService;
use crate::{error, info, success, warn};

pub struct InitCommand;

#[async_trait]
impl Command for InitCommand {
    fn name(&self) -> &'static str {
        "init"
    }

    async fn execute(&self, _matches: &ArgMatches) {
        if let Err(e) = run_init_wizard().await {
            eprintln!("❌ Setup failed: {}", e);
            std::process::exit(1);
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name()).about("Interactive setup wizard for first-time configuration")
    }
}

// ============================================================================
// Path utilities
// ============================================================================

fn get_config_path() -> PathBuf {
    if let Ok(custom_path) = std::env::var("WTF_CONFIG_HOME") {
        PathBuf::from(custom_path).join("config.toml")
    } else {
        let home = std::env::var("HOME").expect("HOME environment variable not set");
        PathBuf::from(home).join(".config/wtf/config.toml")
    }
}

fn get_db_path() -> PathBuf {
    if let Ok(custom_path) = std::env::var("WTF_CONFIG_HOME") {
        PathBuf::from(custom_path).join(".wtf_db")
    } else {
        let home = std::env::var("HOME").expect("HOME environment variable not set");
        PathBuf::from(home).join(".config/wtf/.wtf_db")
    }
}

fn get_backup_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(".config/wtf/backups")
}

// ============================================================================
// Backup utilities
// ============================================================================

fn backup_existing_config() -> Result<(), Box<dyn Error>> {
    let config_path = get_config_path();
    let db_path = get_db_path();
    let backup_dir = get_backup_dir();

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_path = backup_dir.join(timestamp.to_string());

    fs::create_dir_all(&backup_path)?;

    if config_path.exists() {
        fs::copy(&config_path, backup_path.join("config.toml"))?;
    }

    if db_path.exists() {
        copy_dir_all(&db_path, &backup_path.join(".wtf_db"))?;
    }

    println!("Backup created: {}", backup_path.display());
    Ok(())
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// ============================================================================
// Autocomplete helpers
// ============================================================================

fn board_autocomplete(
    boards: Vec<Board>,
) -> Box<dyn Fn(&str) -> Result<Vec<String>, CustomUserError>> {
    Box::new(move |input: &str| {
        let input_lower = input.to_lowercase();
        Ok(boards
            .iter()
            .filter(|b| {
                input.is_empty()
                    || b.name.to_lowercase().contains(&input_lower)
                    || b.id.to_string().contains(&input_lower)
            })
            .map(|b| format!("[{}] {}", b.id, b.name))
            .take(15)
            .collect())
    })
}

fn sprint_autocomplete(
    sprints: Vec<Sprint>,
) -> Box<dyn Fn(&str) -> Result<Vec<String>, CustomUserError>> {
    Box::new(move |input: &str| {
        let input_lower = input.to_lowercase();
        Ok(sprints
            .iter()
            .filter(|s| {
                input.is_empty()
                    || s.name.to_lowercase().contains(&input_lower)
                    || s.id.to_string().contains(&input_lower)
            })
            .map(|s| {
                let dates = if let (Some(start), Some(end)) = (&s.start, &s.end) {
                    format!(" ({} - {})", start.format("%b %d"), end.format("%b %d"))
                } else {
                    String::new()
                };
                format!("[{}] {}{}", s.id, s.name, dates)
            })
            .take(15)
            .collect())
    })
}

// ============================================================================
// Setup steps
// ============================================================================

fn step1_check_existing_config() -> Result<bool, Box<dyn Error>> {
    info!("[Step 1/6] Checking existing configuration");

    let config_path = get_config_path();

    if !config_path.exists() {
        return Ok(true); // Continue with setup
    }

    warn!("Existing configuration found at {}", config_path.display());

    let choices = vec![
        "Cancel (keep existing config)",
        "Create backup and reconfigure",
        "Overwrite (dangerous!)",
    ];

    let selection = Select::new("What would you like to do?", choices).prompt()?;

    match selection {
        "Cancel (keep existing config)" => {
            success!("Keeping existing configuration");
            Ok(false) // Don't continue
        }
        "Create backup and reconfigure" => {
            backup_existing_config()?;
            // println!("");
            Ok(true)
        }
        "Overwrite (dangerous!)" => {
            let confirmed = Confirm::new("⚠️  This will overwrite your config. Are you sure?")
                .with_default(false)
                .prompt()?;

            if !confirmed {
                success!("Cancelled");
                return Ok(false);
            }
            Ok(true)
        }
        _ => unreachable!(),
    }
}

fn step2_setup_directory() -> Result<(), Box<dyn Error>> {
    info!("[Step 2/6] Setting up directory");

    let config_path = get_config_path();
    let config_dir = config_path.parent().unwrap();

    fs::create_dir_all(config_dir)?;
    success!("Config directory: {}", config_dir.display());

    Ok(())
}

fn step3_configure_jira() -> Result<JiraConfig, Box<dyn Error>> {
    info!("[Step 3/7] Configuring Jira");

    let jira_url = Text::new("Jira URL:")
        .with_default("https://your-company.atlassian.net")
        .prompt()?;

    let jira_email = Text::new("Jira email:").prompt()?;

    let jira_token = Password::new("Jira API token (hidden):")
        .without_confirmation()
        .prompt()?;

    let jira_config = JiraConfig {
        base_url: jira_url.clone(),
        username: jira_email.clone(),
        api_token: SensitiveString::from_str(&jira_token).unwrap(),
        auto_follow_sprint_pattern: None,
    };

    // Save temp config so subsequent API calls can pick up credentials
    let temp_config = Config {
        jira: jira_config.clone(),
        github: GithubConfig { organisation: None },
        google: None,
        worklog: WorklogConfig::default(),
    };
    temp_config.save()?;

    Ok(jira_config)
}

async fn step4_select_boards() -> Result<(), Box<dyn Error>> {
    info!("[Step 4/7] Selecting Jira boards");
    info!("🔄 Connecting to Jira and fetching boards...");

    use crate::tasks::jira_tasks::FetchJiraBoard;
    use crate::tasks::Task;

    FetchJiraBoard::new().without_follow_prompt().execute().await.map_err(|e| {
        format!(
            "Failed to connect to Jira: {}\nPlease check your URL, email and API token.",
            e
        )
    })?;

    // Now get boards from database
    let boards = wtf_lib::services::jira_service::BoardService::get_all_boards();

    if boards.is_empty() {
        warn!("No boards found in your Jira instance.");
        return Err("No boards available".into());
    }

    success!("Connected! Found {} board(s)", boards.len());
    info!("Tip: Start typing to filter boards by name or ID");

    let mut selected_boards = Vec::new();

    let board_suggestor: &'static _ = Box::leak(board_autocomplete(boards.clone()));

    loop {
        let prompt_msg = if selected_boards.is_empty() {
            "Select a board to follow (type to search): "
        } else {
            "Select another board (or press Esc to finish): "
        };

        let board_result = Text::new(prompt_msg)
            .with_autocomplete(board_suggestor)
            .with_page_size(10)
            .prompt();

        match board_result {
            Ok(selection) => {
                if selection.trim().is_empty() {
                    if selected_boards.is_empty() {
                        error!("You must select at least one board");
                        continue;
                    }
                    break;
                }

                // Parse board ID from "[123] Board Name" format
                if let Some(id_str) = selection
                    .split(']')
                    .next()
                    .and_then(|s| s.strip_prefix('['))
                {
                    if !selected_boards.contains(&id_str.to_string()) {
                        selected_boards.push(id_str.to_string());
                        success!("Added: {}", selection);
                    } else {
                        info!("  Board already selected");
                    }
                } else {
                    error!("Invalid selection format");
                    continue;
                }

                if selected_boards.len() >= boards.len() {
                    break;
                }

                let add_more = Confirm::new("Add another board?")
                    .with_default(false)
                    .prompt()?;

                if !add_more {
                    break;
                }
            }
            Err(_) => {
                if selected_boards.is_empty() {
                    error!("You must select at least one board");
                    return Err("No boards selected".into());
                }
                break;
            }
        }
    }

    // Follow selected boards
    info!("🔄 Following boards...");
    for board_id in &selected_boards {
        JiraService::follow_board(board_id).map_err(|e| -> Box<dyn Error> { e })?;
    }

    success!("Following {} board(s)", selected_boards.len());

    Ok(())
}

async fn step5_fetch_sprints() -> Result<(), Box<dyn Error>> {
    info!("[Step 5/7] Fetching sprints");
    info!("🔄 Fetching sprints for selected boards...");

    use crate::tasks::jira_tasks::FetchJiraSprint;
    use crate::tasks::Task;

    FetchJiraSprint::new().execute().await?;

    success!("Sprints fetched");
    Ok(())
}

fn step5_select_sprints() -> Result<(), Box<dyn Error>> {
    info!("[Step 6/7] Selecting sprints");
    info!("🔄 Fetching active sprints...");

    let sprints = JiraService::get_available_sprints();
    let active_sprints: Vec<Sprint> = sprints
        .into_iter()
        .filter(|s| {
            use wtf_lib::models::data::SprintState;
            // Only show active/future sprints that are NOT already followed
            !s.followed && matches!(s.state, SprintState::Active | SprintState::Future)
        })
        .collect();

    if active_sprints.is_empty() {
        warn!("No unfollowed active sprints found");
        info!("  All active sprints are already followed, or you can follow sprints later with: wtf sprint follow <id>");
        return Ok(());
    }

    success!("Found {} unfollowed active sprint(s)", active_sprints.len());
    info!("💡 Tip: Start typing to filter sprints by name or ID\n");

    let mut selected_sprints = Vec::new();

    let sprint_suggestor: &'static _ = Box::leak(sprint_autocomplete(active_sprints.clone()));

    loop {
        let prompt_msg = if selected_sprints.is_empty() {
            "Select a sprint to follow (type to search, or press Esc to skip): "
        } else {
            "Select another sprint (or press Esc to finish): "
        };

        let sprint_result = Text::new(prompt_msg)
            .with_autocomplete(sprint_suggestor)
            .with_page_size(10)
            .prompt();

        match sprint_result {
            Ok(selection) => {
                if selection.trim().is_empty() {
                    break;
                }

                // Parse sprint ID from "[123] Sprint Name" format
                if let Some(id_str) = selection
                    .split(']')
                    .next()
                    .and_then(|s| s.strip_prefix('['))
                {
                    if !selected_sprints.contains(&id_str.to_string()) {
                        selected_sprints.push(id_str.to_string());
                        success!("Added: {}", selection);
                    } else {
                        warn!("Sprint already selected");
                    }
                } else {
                    error!("Invalid selection format");
                    continue;
                }

                if selected_sprints.len() >= active_sprints.len() {
                    break;
                }

                let add_more = Confirm::new("Add another sprint?")
                    .with_default(false)
                    .prompt()?;

                if !add_more {
                    break;
                }
            }
            Err(_) => {
                break;
            }
        }
    }

    if selected_sprints.is_empty() {
        warn!("No sprints followed");
        info!("  You can follow sprints later with: wtf sprint follow <id>");
    } else {
        info!("🔄 Following sprints...");
        for sprint_id in &selected_sprints {
            JiraService::follow_sprint(sprint_id).map_err(|e| -> Box<dyn Error> { e })?;
        }
        success!("Following {} sprint(s)", selected_sprints.len());
    }

    Ok(())
}

fn step6_configure_github() -> Result<GithubConfig, Box<dyn Error>> {
    info!("[Step 7/7] Configuring GitHub (optional)");

    let enable_github = Confirm::new("Enable GitHub activity tracking?")
        .with_default(false)
        .prompt()?;

    if !enable_github {
        warn!("Skipping GitHub integration");
        return Ok(GithubConfig { organisation: None });
    }

    info!("🔄 Checking GitHub CLI...");

    let gh_check = std::process::Command::new("gh")
        .arg("auth")
        .arg("status")
        .output();

    match gh_check {
        Ok(output) if output.status.success() => {
            success!("GitHub CLI found and authenticated!");
        }
        _ => {
            error!("❌ GitHub CLI (gh) not found or not authenticated");
            info!("Install it from: https://cli.github.com/");
            info!("Then run: gh auth login\n");
        }
    }

    let org = Text::new("GitHub organisation to track (leave blank to track all repos):")
        .prompt_skippable()?
        .and_then(|s| if s.trim().is_empty() { None } else { Some(s.trim().to_string()) });

    if let Some(ref org_name) = org {
        success!("Will filter events to repos under '{}'", org_name);
    } else {
        success!("Will track events across all repos");
    }

    Ok(GithubConfig { organisation: org })
}

fn step7_configure_google() -> Result<Option<GoogleConfig>, Box<dyn Error>> {
    info!("Google Calendar (optional)");

    // Check if using test environment
    let using_test_env = std::env::var("WTF_CONFIG_HOME").is_ok();
    
    if using_test_env {
        warn!("Note: You're using a test environment (WTF_CONFIG_HOME)");
        info!("   Google Calendar requires OAuth authentication which needs:");
        info!("   - Google API credentials JSON file");
        info!("   - Browser-based OAuth flow");
        info!("   This is complex to set up for testing.");
    }
    
    let enable_google = Confirm::new("Enable Google Calendar integration? (optional)")
        .with_default(false)
        .prompt()?;

    if !enable_google {
        warn!("Skipping Google Calendar integration");
        return Ok(None);
    }

    // Suggest appropriate default paths based on environment
    let default_creds_path = if using_test_env {
        format!("{}/google_credentials.json", std::env::var("WTF_CONFIG_HOME").unwrap())
    } else {
        "~/.config/wtf/google_credentials.json".to_string()
    };
    
    let default_token_path = if using_test_env {
        format!("{}/google_token.json", std::env::var("WTF_CONFIG_HOME").unwrap())
    } else {
        "~/.config/wtf/google_token.json".to_string()
    };

    let credentials_path: String = Text::new("Path to Google credentials JSON file:")
        .with_default(&default_creds_path)
        .prompt()?;

    let token_cache_path: String = Text::new("Path to Google token cache:")
        .with_default(&default_token_path)
        .prompt()?;
    
    // Check if credentials file exists
    let creds_path_expanded = shellexpand::tilde(&credentials_path).to_string();
    if !std::path::Path::new(&creds_path_expanded).exists() {
        warn!("Warning: Credentials file not found at: {}", credentials_path);
        info!("   Download OAuth 2.0 credentials from Google Cloud Console:");
        info!("   https://console.cloud.google.com/apis/credentials");
        info!("   Save as: {}", credentials_path);

        let proceed = Confirm::new("Continue anyway?")
            .with_default(true)
            .prompt()?;
            
        if !proceed {
            warn!("Skipping Google Calendar integration");
            return Ok(None);
        }
    }

    success!("Google Calendar configured");
    
    // If credentials exist, trigger OAuth flow now
    if std::path::Path::new(&creds_path_expanded).exists() {
        info!("🔐 Initiating OAuth flow to authorize Google Calendar access...");
        info!("   Your browser will open for authentication.");

        // Return the config so it can be saved first
        Ok(Some(GoogleConfig {
            credentials_path,
            token_cache_path,
            color_labels: std::collections::HashMap::new(),
        }))
    } else {
        warn!("Remember to add credentials file before fetching meetings");
        Ok(Some(GoogleConfig {
            credentials_path,
            token_cache_path,
            color_labels: std::collections::HashMap::new(),
        }))
    }
}

// Step 8: Complete Google OAuth flow
async fn step8_complete_google_oauth(
    _google_config: &GoogleConfig,
) -> Result<(), Box<dyn Error>> {
    use crate::tasks::google_tasks::FetchGoogleCalendarTask;
    use crate::tasks::Task;
    use chrono::Utc;

    println!("\nYour browser will open in 3 seconds for Google Calendar authorization...");
    for i in (1..=3).rev() {
        print!("  {}...\r", i);
        use std::io::Write;
        let _ = std::io::stdout().flush();
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    println!();

    info!("🔄 Fetching Google Calendar meetings to complete OAuth setup...");
    
    // Fetch meetings for the next 2 weeks to trigger OAuth
    let start = Utc::now();
    let end = start + chrono::Duration::days(14);
    
    let task = FetchGoogleCalendarTask::new(start, end);
    
    match task.execute().await {
        Ok(_) => {
            success!("Google Calendar OAuth completed successfully");
            success!("Token cached for future use");
            Ok(())
        }
        Err(e) => {
            warn!("OAuth flow failed: {}", e);
            info!("   You can complete this later by running: wtf fetch google");
            Ok(()) // Don't fail init if OAuth fails
        }
    }
}

// ============================================================================
// Main wizard orchestrator
// ============================================================================

pub async fn run_init_wizard() -> Result<(), Box<dyn Error>> {
    info!("🚀 Welcome to WTF - Worklog Time Filler!");
    info!("Let's get you set up in a few minutes.");

    // Step 1: Check existing config
    if !step1_check_existing_config()? {
        return Ok(());
    }

    // Step 2: Setup directory
    step2_setup_directory()?;

    // Step 3: Configure Jira credentials
    let jira_config = step3_configure_jira()?;

    // Step 4: Fetch boards from Jira (connection test) + user selects boards
    step4_select_boards().await?;

    // Step 5: Fetch sprints for selected boards
    step5_fetch_sprints().await?;

    // Step 6: Select sprints to follow
    step5_select_sprints()?;

    // Step 7: Configure GitHub
    let github_config = step6_configure_github()?;

    // Step 7: Configure Google Calendar (bonus, not counted in progress)
    let google_config = step7_configure_google()?;

    // Save final configuration before OAuth
    let final_config = Config {
        jira: jira_config,
        github: github_config,
        google: google_config.clone(),
        worklog: WorklogConfig::default(),
    };

    final_config.save()?;

    // Step 8: Complete Google OAuth (if configured and credentials exist)
    if let Some(ref google_cfg) = google_config {
        let creds_path = shellexpand::tilde(&google_cfg.credentials_path).to_string();
        if std::path::Path::new(&creds_path).exists() {
            step8_complete_google_oauth(google_cfg).await?;
        }
    }

    success!("Setup complete!");
    info!("🎉 All done! Your configuration has been saved.");

    // Show different next steps based on whether using custom config home
    if std::env::var("WTF_CONFIG_HOME").is_ok() {
        let config_home = std::env::var("WTF_CONFIG_HOME").unwrap();
        warn!("Note: You used WTF_CONFIG_HOME={}", config_home);
        info!("   This is a separate test environment with an empty database.");
        info!("📋 Next steps to populate the test database:");
        info!("  1. Fetch Jira issues for followed sprints:");
        info!("     WTF_CONFIG_HOME={} cargo run -- fetch issue", config_home);
        info!("  2. Launch the TUI:");
        info!("     WTF_CONFIG_HOME={} cargo run -- tui", config_home);
        info!("💡 Or to use your real config instead, run without WTF_CONFIG_HOME:");
        info!("     cargo run -- tui");
    } else {
        info!("📋 Next steps:");
        info!("  1. Fetch Jira data:");
        info!("     cargo run -- fetch all      # Fetch everything");
        info!("     cargo run -- fetch issue     # Just issues");
        info!("  2. Launch the TUI:");
        info!("     cargo run -- tui");
        info!("  Or use 'cargo run -- --help' to see all available commands");
    }

    Ok(())
}
