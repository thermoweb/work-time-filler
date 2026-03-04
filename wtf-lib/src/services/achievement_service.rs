use crate::models::achievement::{Achievement, AchievementUnlock};
use crate::storage::database::{GenericDatabase, DATABASE};
use log::error;
use once_cell::sync::Lazy;
use std::sync::Mutex;

// Achievements that should be revoked if unlocked before a given version.
// Format: (achievement_id, revoke_if_version_less_than)
const REVOKE_SCHEDULE: &[(&str, &str)] = &[
    ("git_squash_master", "0.1.0-beta.3"), // trigger was wrong before beta.3
];

// Lazy database handle
static ACHIEVEMENTS_DATABASE: Lazy<GenericDatabase<AchievementUnlock>> = Lazy::new(|| {
    GenericDatabase::new(&DATABASE, "achievements").unwrap_or_else(|e| {
        panic!("Failed to initialize achievements database: {}", e);
    })
});

// In-memory cache for quick access
static ACHIEVEMENT_CACHE: Lazy<Mutex<Vec<AchievementUnlock>>> = Lazy::new(|| {
    let unlocks = ACHIEVEMENTS_DATABASE.get_all().unwrap_or_else(|e| {
        error!("Failed to load achievements from database: {}", e);
        Vec::new()
    });
    Mutex::new(unlocks)
});

pub struct AchievementService;

impl AchievementService {
    /// Check if an achievement is unlocked
    pub fn is_unlocked(achievement: Achievement) -> bool {
        ACHIEVEMENT_CACHE
            .lock()
            .unwrap()
            .iter()
            .any(|u| u.achievement == achievement)
    }

    /// Unlock an achievement
    /// Returns true if newly unlocked, false if already unlocked
    pub fn unlock(achievement: Achievement) -> bool {
        let mut cache = ACHIEVEMENT_CACHE.lock().unwrap();

        // Check if already unlocked
        if cache.iter().any(|u| u.achievement == achievement) {
            return false;
        }

        // Create unlock record
        let unlock = AchievementUnlock {
            achievement,
            unlocked_at: chrono::Utc::now(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Save to database
        if let Err(e) = ACHIEVEMENTS_DATABASE.insert(&unlock) {
            error!("Failed to save achievement unlock: {}", e);
            return false;
        }

        // Update cache
        cache.push(unlock);

        true
    }

    /// Revoke achievements that were unlocked before a bugfix version.
    /// Should be called once at app startup.
    pub fn run_revoke_schedule() {
        use crate::utils::version::is_older_than;
        let mut cache = ACHIEVEMENT_CACHE.lock().unwrap();
        let mut revoked = Vec::new();

        for (achievement_id, threshold) in REVOKE_SCHEDULE {
            cache.retain(|u| {
                if u.achievement.id_string() == *achievement_id {
                    let version = if u.app_version.is_empty() {
                        "0.0.0"
                    } else {
                        &u.app_version
                    };
                    if is_older_than(version, threshold) {
                        revoked.push(u.achievement.id_string());
                        return false; // remove from cache
                    }
                }
                true
            });
        }

        // Persist removals to DB
        for id in &revoked {
            if let Err(e) = ACHIEVEMENTS_DATABASE.remove(id) {
                error!("Failed to revoke achievement '{}': {}", id, e);
            }
        }

        if !revoked.is_empty() {
            log::info!("Revoked {} achievement(s): {:?}", revoked.len(), revoked);
        }
    }

    /// Get all unlocked achievements
    pub fn get_all_unlocked() -> Vec<AchievementUnlock> {
        ACHIEVEMENT_CACHE.lock().unwrap().clone()
    }

    /// Get count of unlocked achievements
    pub fn unlock_count() -> usize {
        ACHIEVEMENT_CACHE.lock().unwrap().len()
    }

    /// Check if any achievements are unlocked (for tab visibility)
    pub fn has_any_unlocked() -> bool {
        Self::unlock_count() > 0
    }

    /// Reset all achievements (for testing)
    /// Deletes all achievement records from database and clears cache
    pub fn reset_all() -> Result<(), String> {
        // Clear database
        ACHIEVEMENTS_DATABASE
            .clear()
            .map_err(|e| format!("Failed to clear achievements database: {}", e))?;

        // Clear cache
        let mut cache = ACHIEVEMENT_CACHE.lock().unwrap();
        cache.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlock_count() {
        let count = AchievementService::unlock_count();
        assert!(count >= 0);
    }
}
