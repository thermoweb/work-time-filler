use crate::storage::database::Identifiable;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AchievementTier {
    pub threshold: u64,
    pub name: &'static str,
    pub icon: &'static str,
    pub points: u32,
    pub chronie_message: &'static str,
}

#[derive(Debug, Clone)]
pub struct TieredAchievementDef {
    pub id: &'static str,
    pub unit: &'static str,
    pub tiers: Vec<AchievementTier>,
}

impl TieredAchievementDef {
    pub fn all() -> Vec<TieredAchievementDef> {
        vec![Self::wizard_runs(), Self::hours_logged()]
    }

    pub fn wizard_runs() -> TieredAchievementDef {
        TieredAchievementDef {
            id: "wizard_runs",
            unit: "wizard runs",
            tiers: vec![
                AchievementTier {
                    threshold: 1,
                    name: "Apprentice",
                    icon: "🧙",
                    points: 10,
                    chronie_message: "Your first run — the journey begins! 🧙",
                },
                AchievementTier {
                    threshold: 5,
                    name: "Adept",
                    icon: "✨",
                    points: 25,
                    chronie_message: "Five runs in! You're starting to feel the rhythm! ✨",
                },
                AchievementTier {
                    threshold: 15,
                    name: "Mage",
                    icon: "🔮",
                    points: 50,
                    chronie_message:
                        "Fifteen runs! The arcane arts of time logging bend to your will! 🔮",
                },
                AchievementTier {
                    threshold: 30,
                    name: "Sorcerer",
                    icon: "⚗️",
                    points: 75,
                    chronie_message: "Thirty runs! Few reach this level of dedication! ⚗️",
                },
                AchievementTier {
                    threshold: 50,
                    name: "Archmage",
                    icon: "🌟",
                    points: 100,
                    chronie_message: "Fifty runs! You've surpassed all but the greatest! 🌟",
                },
                AchievementTier {
                    threshold: 100,
                    name: "Chronomancer",
                    icon: "⚡",
                    points: 200,
                    chronie_message: "One hundred runs! You've mastered time itself! ⚡",
                },
            ],
        }
    }

    pub fn hours_logged() -> TieredAchievementDef {
        TieredAchievementDef {
            id: "hours_logged",
            unit: "hours logged",
            tiers: vec![
                AchievementTier {
                    threshold: 100,
                    name: "Timekeeper",
                    icon: "⏱️",
                    points: 25,
                    chronie_message: "One hundred hours tracked — you're building a real habit! ⏱️",
                },
                AchievementTier {
                    threshold: 300,
                    name: "Chronicler",
                    icon: "📜",
                    points: 50,
                    chronie_message:
                        "Three hundred hours! Your work history tells quite a story! 📜",
                },
                AchievementTier {
                    threshold: 600,
                    name: "Lorekeeper",
                    icon: "📚",
                    points: 75,
                    chronie_message: "Six hundred hours! The records of your labor grow vast! 📚",
                },
                AchievementTier {
                    threshold: 1000,
                    name: "Sage",
                    icon: "🔭",
                    points: 100,
                    chronie_message: "A thousand hours! Wisdom comes from logging every moment! 🔭",
                },
                AchievementTier {
                    threshold: 2500,
                    name: "Oracle",
                    icon: "🌙",
                    points: 150,
                    chronie_message:
                        "Twenty-five hundred hours! You see the pattern in all time! 🌙",
                },
                AchievementTier {
                    threshold: 5000,
                    name: "Omniscient",
                    icon: "⭐",
                    points: 250,
                    chronie_message: "Five thousand hours! You transcend time logging itself! ⭐",
                },
            ],
        }
    }

    /// Index of the highest tier reached for this count, if any.
    pub fn current_tier_index(&self, count: u64) -> Option<usize> {
        self.tiers
            .iter()
            .enumerate()
            .filter(|(_, t)| count >= t.threshold)
            .map(|(i, _)| i)
            .last()
    }

    /// Next tier not yet reached.
    pub fn next_tier(&self, count: u64) -> Option<&AchievementTier> {
        self.tiers.iter().find(|t| count < t.threshold)
    }
}

/// Stored progress counter for a tiered track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredProgress {
    pub id: String,
    pub count: u64,
}

impl Identifiable for TieredProgress {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}
