use crate::storage::database::Identifiable;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Achievement identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Achievement {
    AboutClicker,
    ChroniesFriend,
    TheUndoer,
    TimelineFixer,
    GitSquashMaster,
    AutoLinkMaster,
    DeclinedButLogged,
    NightOwl,
    QuarterCrunch,
    ColorCoder,
    ForgotFriday,
    PerfectSprint,
}

impl Achievement {
    /// Get all possible achievements
    pub fn all() -> Vec<Achievement> {
        vec![
            Achievement::AboutClicker,
            Achievement::ChroniesFriend,
            Achievement::TheUndoer,
            Achievement::TimelineFixer,
            Achievement::GitSquashMaster,
            Achievement::AutoLinkMaster,
            Achievement::DeclinedButLogged,
            Achievement::NightOwl,
            Achievement::QuarterCrunch,
            Achievement::ColorCoder,
            Achievement::ForgotFriday,
            Achievement::PerfectSprint,
        ]
    }

    /// Get achievement metadata
    pub fn meta(&self) -> AchievementMeta {
        match self {
            Achievement::AboutClicker => AchievementMeta {
                id: *self,
                name: "Curious Explorer".to_string(),
                description: "Discover the About page".to_string(),
                icon: "🔍".to_string(),
                category: AchievementCategory::Meta,
                chronie_message: "Curious, aren't we? I like that! Keep exploring! 🔍".to_string(),
                points: 5,
            },
            Achievement::ChroniesFriend => {
                // Load from PNG metadata
                Self::load_secret_meta("secret_chronie_friend", *self)
            }
            Achievement::TheUndoer => AchievementMeta {
                id: *self,
                name: "The Undoer".to_string(),
                description: "Revert worklogs for the first time".to_string(),
                icon: "🔙".to_string(),
                category: AchievementCategory::Meta,
                chronie_message: "Everyone rewrites history sometimes. That's what I'm here for! 🔙"
                    .to_string(),
                points: 10,
            },
            Achievement::TimelineFixer => AchievementMeta {
                id: *self,
                name: "Timeline Fixer".to_string(),
                description: "Log work for a day more than 60 days in the past".to_string(),
                icon: "⏰".to_string(),
                category: AchievementCategory::Meta,
                chronie_message: "Fixing old temporal anomalies? Risky, but necessary! ⏰"
                    .to_string(),
                points: 25,
            },
            Achievement::GitSquashMaster => AchievementMeta {
                id: *self,
                name: "Squash? Never Heard of It".to_string(),
                description: "Push worklogs for the same day 3+ separate times".to_string(),
                icon: "📚".to_string(),
                category: AchievementCategory::Secret,
                chronie_message:
                    "Three pushes for the same day? Someone needs to learn about squashing! 📚"
                        .to_string(),
                points: 0,
            },
            Achievement::AutoLinkMaster => AchievementMeta {
                id: *self,
                name: "Auto-Link Master".to_string(),
                description: "Auto-link 10+ meetings in a single wizard run".to_string(),
                icon: "🤖".to_string(),
                category: AchievementCategory::Meta,
                chronie_message:
                    "Perfect automation! Your meeting names are so good, I don't even need to think! 🤖"
                        .to_string(),
                points: 50,
            },
            Achievement::DeclinedButLogged => AchievementMeta {
                id: *self,
                name: "Still Committed".to_string(),
                description: "Log time for a meeting you declined".to_string(),
                icon: "🙅".to_string(),
                category: AchievementCategory::Secret,
                chronie_message:
                    "Declined the meeting but worked on it anyway? That's dedication... or poor planning! 🙅"
                        .to_string(),
                points: 0,
            },
            Achievement::NightOwl => AchievementMeta {
                id: *self,
                name: "Night Owl".to_string(),
                description: "Push worklogs after 10pm or before 6am".to_string(),
                icon: "🌙".to_string(),
                category: AchievementCategory::Secret,
                chronie_message: "Logging work at this hour? Even time anomalies need sleep! 🌙"
                    .to_string(),
                points: 0,
            },
            Achievement::QuarterCrunch => AchievementMeta {
                id: *self,
                name: "Quarter Crunch".to_string(),
                description: "Cover 90%+ of working days in a full calendar quarter".to_string(),
                icon: "📊".to_string(),
                category: AchievementCategory::Consistency,
                chronie_message:
                    "A whole quarter with barely a gap? You're a time-logging machine! 📊"
                        .to_string(),
                points: 50,
            },
            Achievement::ColorCoder => AchievementMeta {
                id: *self,
                name: "Color Coder".to_string(),
                description: "Auto-link a meeting using a calendar color label".to_string(),
                icon: "🎨".to_string(),
                category: AchievementCategory::Productivity,
                chronie_message:
                    "Color-coding your calendar for automatic linking? Chronie is impressed by your organizational genius! 🎨"
                        .to_string(),
                points: 25,
            },
            Achievement::ForgotFriday => AchievementMeta {
                id: *self,
                name: "Forgot Friday".to_string(),
                description: "Push worklogs for a Friday after the fact".to_string(),
                icon: "📅".to_string(),
                category: AchievementCategory::Secret,
                chronie_message: "Friday was great, but apparently you forgot to tell me about it! 📅"
                    .to_string(),
                points: 0,
            },
            Achievement::PerfectSprint => AchievementMeta {
                id: *self,
                name: "Perfect Sprint".to_string(),
                description: "Log time for every workday in a sprint".to_string(),
                icon: "🏅".to_string(),
                category: AchievementCategory::Consistency,
                chronie_message: "Not a single day missed! I'm genuinely impressed. 🏅".to_string(),
                points: 50,
            },
        }
    }

    /// Load secret achievement metadata from PNG (result cached after first call)
    fn load_secret_meta(secret_id: &str, achievement: Achievement) -> AchievementMeta {
        use crate::utils::branding::AppBranding;
        use std::sync::OnceLock;

        static BRANDING: OnceLock<Option<AppBranding>> = OnceLock::new();
        let branding = BRANDING.get_or_init(|| AppBranding::load().ok());

        if let Some(branding) = branding {
            if let Some(secrets) = &branding.secrets {
                if let Some(sa) = secrets.achievements.get(secret_id) {
                    return AchievementMeta {
                        id: achievement,
                        name: sa.name.clone(),
                        description: sa.description.clone(),
                        icon: sa.icon.clone(),
                        category: AchievementCategory::Secret,
                        chronie_message: sa.chronie_message.clone(),
                        points: sa.points,
                    };
                }
            }
        }

        // Fallback if PNG not loaded or secret not found
        AchievementMeta {
            id: achievement,
            name: "Secret Achievement".to_string(),
            description: "???".to_string(),
            icon: "🔒".to_string(),
            category: AchievementCategory::Secret,
            chronie_message: "You found a secret!".to_string(),
            points: 0,
        }
    }

    /// Get unique string identifier for database storage
    pub fn id_string(&self) -> String {
        match self {
            Achievement::AboutClicker => "about_clicker".to_string(),
            Achievement::ChroniesFriend => "chronies_friend".to_string(),
            Achievement::TheUndoer => "the_undoer".to_string(),
            Achievement::TimelineFixer => "timeline_fixer".to_string(),
            Achievement::GitSquashMaster => "git_squash_master".to_string(),
            Achievement::AutoLinkMaster => "auto_link_master".to_string(),
            Achievement::DeclinedButLogged => "declined_but_logged".to_string(),
            Achievement::NightOwl => "night_owl".to_string(),
            Achievement::QuarterCrunch => "quarter_crunch".to_string(),
            Achievement::ColorCoder => "color_coder".to_string(),
            Achievement::ForgotFriday => "forgot_friday".to_string(),
            Achievement::PerfectSprint => "perfect_sprint".to_string(),
        }
    }
}

/// Achievement metadata
#[derive(Debug, Clone)]
pub struct AchievementMeta {
    pub id: Achievement,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: AchievementCategory,
    pub chronie_message: String,
    pub points: u32,
}

/// Achievement category for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AchievementCategory {
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
