use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::storage::database::Identifiable;

/// Achievement identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Achievement {
    /// Complete your first wizard run
    ChroniesApprentice,
    /// View the About popup (easter egg)
    AboutClicker,
    /// Secret: Type "chronie" to summon the wizard
    ChroniesFriend,
    /// Revert worklogs for the first time
    TheUndoer,
    /// Log work for a day more than 60 days in the past
    TimelineFixer,
    /// Push worklogs 3+ times for the same day
    GitSquashMaster,
    /// Have 10+ meetings all auto-linked
    AutoLinkMaster,
    /// Log time for a meeting you declined
    DeclinedButLogged,
    /// Push a worklog started after 10pm or before 6am
    NightOwl,
    /// Cover 90%+ of working days in a full calendar quarter
    QuarterCrunch,
}

impl Achievement {
    /// Get all possible achievements
    pub fn all() -> Vec<Achievement> {
        vec![
            Achievement::ChroniesApprentice,
            Achievement::AboutClicker,
            Achievement::ChroniesFriend,
            Achievement::TheUndoer,
            Achievement::TimelineFixer,
            Achievement::GitSquashMaster,
            Achievement::AutoLinkMaster,
            Achievement::DeclinedButLogged,
            Achievement::NightOwl,
            Achievement::QuarterCrunch,
        ]
    }

    /// Get achievement metadata
    pub fn meta(&self) -> AchievementMeta {
        match self {
            Achievement::ChroniesApprentice => AchievementMeta {
                id: *self,
                name: "Chronie's Apprentice",
                description: "Complete your first wizard run with Chronie",
                icon: "🧙",
                category: AchievementCategory::Wizard,
                chronie_message: "Well done, apprentice! You've mastered the basics! 🧙",
            },
            Achievement::AboutClicker => AchievementMeta {
                id: *self,
                name: "Curious Explorer",
                description: "Discover the About page",
                icon: "🔍",
                category: AchievementCategory::Meta,
                chronie_message: "Curious, aren't we? I like that! Keep exploring! 🔍",
            },
            Achievement::ChroniesFriend => {
                // Load from PNG metadata
                Self::load_secret_meta("secret_chronie_friend", *self)
            }
            Achievement::TheUndoer => AchievementMeta {
                id: *self,
                name: "The Undoer",
                description: "Revert worklogs for the first time",
                icon: "🔙",
                category: AchievementCategory::Meta,
                chronie_message: "Everyone rewrites history sometimes. That's what I'm here for! 🔙",
            },
            Achievement::TimelineFixer => AchievementMeta {
                id: *self,
                name: "Timeline Fixer",
                description: "Log work for a day more than 60 days in the past",
                icon: "⏰",
                category: AchievementCategory::Meta,
                chronie_message: "Fixing old temporal anomalies? Risky, but necessary! ⏰",
            },
            Achievement::GitSquashMaster => AchievementMeta {
                id: *self,
                name: "Squash? Never Heard of It",
                description: "Push worklogs for the same day 3+ separate times",
                icon: "📚",
                category: AchievementCategory::Meta,
                chronie_message: "Three pushes for the same day? Someone needs to learn about squashing! 📚",
            },
            Achievement::AutoLinkMaster => AchievementMeta {
                id: *self,
                name: "Auto-Link Master",
                description: "Have 10+ meetings all automatically linked",
                icon: "🤖",
                category: AchievementCategory::Meta,
                chronie_message: "Perfect automation! Your meeting names are so good, I don't even need to think! 🤖",
            },
            Achievement::DeclinedButLogged => AchievementMeta {
                id: *self,
                name: "Still Committed",
                description: "Log time for a meeting you declined",
                icon: "🙅",
                category: AchievementCategory::Ironic,
                chronie_message: "Declined the meeting but worked on it anyway? That's dedication... or poor planning! 🙅",
            },
            Achievement::NightOwl => AchievementMeta {
                id: *self,
                name: "Night Owl",
                description: "Push worklogs after 10pm or before 6am",
                icon: "🌙",
                category: AchievementCategory::Ironic,
                chronie_message: "Logging work at this hour? Even time anomalies need sleep! 🌙",
            },
            Achievement::QuarterCrunch => AchievementMeta {
                id: *self,
                name: "Quarter Crunch",
                description: "Cover 90%+ of working days in a full calendar quarter",
                icon: "📊",
                category: AchievementCategory::Consistency,
                chronie_message: "A whole quarter with barely a gap? You're a time-logging machine! 📊",
            },
        }
    }
    
    /// Load secret achievement metadata from PNG
    fn load_secret_meta(secret_id: &str, achievement: Achievement) -> AchievementMeta {
        use crate::utils::branding::AppBranding;
        
        if let Ok(branding) = AppBranding::load() {
            if let Some(secrets) = &branding.secrets {
                if let Some(secret_achievement) = secrets.achievements.get(secret_id) {
                    // Leak strings to get 'static lifetime (they live for program duration anyway)
                    let name: &'static str = Box::leak(secret_achievement.name.clone().into_boxed_str());
                    let description: &'static str = Box::leak(secret_achievement.description.clone().into_boxed_str());
                    let icon: &'static str = Box::leak(secret_achievement.icon.clone().into_boxed_str());
                    let chronie_message: &'static str = Box::leak(secret_achievement.chronie_message.clone().into_boxed_str());
                    
                    return AchievementMeta {
                        id: achievement,
                        name,
                        description,
                        icon,
                        category: AchievementCategory::Secret,
                        chronie_message,
                    };
                }
            }
        }
        
        // Fallback if PNG not loaded
        AchievementMeta {
            id: achievement,
            name: "Secret Achievement",
            description: "???",
            icon: "🔒",
            category: AchievementCategory::Secret,
            chronie_message: "You found a secret!",
        }
    }
    
    /// Get unique string identifier for database storage
    pub fn id_string(&self) -> String {
        match self {
            Achievement::ChroniesApprentice => "chronies_apprentice".to_string(),
            Achievement::AboutClicker => "about_clicker".to_string(),
            Achievement::ChroniesFriend => "chronies_friend".to_string(),
            Achievement::TheUndoer => "the_undoer".to_string(),
            Achievement::TimelineFixer => "timeline_fixer".to_string(),
            Achievement::GitSquashMaster => "git_squash_master".to_string(),
            Achievement::AutoLinkMaster => "auto_link_master".to_string(),
            Achievement::DeclinedButLogged => "declined_but_logged".to_string(),
            Achievement::NightOwl => "night_owl".to_string(),
            Achievement::QuarterCrunch => "quarter_crunch".to_string(),
        }
    }
}

/// Achievement metadata
#[derive(Debug, Clone)]
pub struct AchievementMeta {
    pub id: Achievement,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub category: AchievementCategory,
    pub chronie_message: &'static str,
}

/// Achievement category for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AchievementCategory {
    Wizard,
    Consistency,
    Productivity,
    Milestones,
    Meta,
    Secret,
    Ironic,
}

/// Achievement unlock record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementUnlock {
    pub achievement: Achievement,
    pub unlocked_at: DateTime<Utc>,
    #[serde(default)]
    pub app_version: String,
}

impl Identifiable for AchievementUnlock {
    fn get_id(&self) -> String {
        self.achievement.id_string()
    }
}

/// Achievement progress storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AchievementData {
    pub unlocks: Vec<AchievementUnlock>,
}

impl AchievementData {
    /// Check if an achievement is unlocked
    pub fn is_unlocked(&self, achievement: Achievement) -> bool {
        self.unlocks.iter().any(|u| u.achievement == achievement)
    }

    /// Get unlock timestamp for an achievement
    pub fn unlock_time(&self, achievement: Achievement) -> Option<DateTime<Utc>> {
        self.unlocks
            .iter()
            .find(|u| u.achievement == achievement)
            .map(|u| u.unlocked_at)
    }

    /// Unlock an achievement
    pub fn unlock(&mut self, achievement: Achievement) -> bool {
        if self.is_unlocked(achievement) {
            return false; // Already unlocked
        }

        self.unlocks.push(AchievementUnlock {
            achievement,
            unlocked_at: Utc::now(),
            app_version: String::new(),
        });
        true
    }

    /// Get count of unlocked achievements
    pub fn unlock_count(&self) -> usize {
        self.unlocks.len()
    }

    /// Get total achievement count
    pub fn total_count(&self) -> usize {
        Achievement::all().len()
    }
}
