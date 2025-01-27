use crate::commands::Command;
use crate::tui::Tui;
use async_trait::async_trait;
use clap::{ArgMatches, Command as ClapCommand};

pub struct TuiCommand;

#[async_trait]
impl Command for TuiCommand {
    fn name(&self) -> &'static str {
        "tui"
    }

    async fn execute(&self, matches: &ArgMatches) {
        // Reset achievements if flag is set (only check if flag exists in matches)
        if matches.try_contains_id("reset_achievements").unwrap_or(false) 
            && matches.get_flag("reset_achievements") 
        {
            use wtf_lib::services::AchievementService;
            
            println!("🗑️  Resetting all achievements...");
            if let Err(e) = AchievementService::reset_all() {
                eprintln!("❌ Failed to reset achievements: {}", e);
                return;
            }
            println!("✅ All achievements have been reset!");
            println!();
        }

        use wtf_lib::config::Config;
        if !Config::load().map(|c| c.is_configured()).unwrap_or(false) {
            println!("No configuration found. Starting setup wizard...\n");
            if let Err(e) = crate::commands::init::run_init_wizard().await {
                eprintln!("Setup failed: {}", e);
                return;
            }
            println!("\n✓ Setup complete! Press any key to launch WTF...");
            {
                use crossterm::event::{read, Event};
                use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
                let _ = enable_raw_mode();
                loop {
                    if let Ok(Event::Key(_)) = read() { break; }
                }
                let _ = disable_raw_mode();
            }
            println!();
        }

        let mut tui = Tui::new();
        if let Err(e) = tui.run() {
            eprintln!("Failed to run dashboard: {}", e);
        }
    }

    fn clap_command(&self) -> ClapCommand {
        ClapCommand::new(self.name())
            .about("Launch interactive TUI")
            .arg(
                clap::Arg::new("reset_achievements")
                    .long("reset-achievements")
                    .help("Reset all achievements before starting")
                    .action(clap::ArgAction::SetTrue)
            )
    }
}
