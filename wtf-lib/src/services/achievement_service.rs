use crate::models::achievement::{Achievement, AchievementUnlock};
use crate::storage::database::{GenericDatabase, DATABASE};
use log::error;
use std::sync::Mutex;

// Achievements that should be revoked if unlocked before a given version.
// Format: (achievement_id, revoke_if_version_less_than)
const REVOKE_SCHEDULE: &[(&str, &str)] = &[
    ("git_squash_master", "0.1.0-beta.3"), // trigger was wrong before beta.3
];

pub struct AchievementService {
    db: GenericDatabase<AchievementUnlock>,
    cache: Mutex<Vec<AchievementUnlock>>,
}

impl AchievementService {
    /// Create a service from any database handle. Use this in tests with a temp DB.
    pub fn new(db: GenericDatabase<AchievementUnlock>) -> Self {
        let unlocks = db.get_all().unwrap_or_else(|e| {
            error!("Failed to load achievements from database: {}", e);
            Vec::new()
        });
        Self {
            db,
            cache: Mutex::new(unlocks),
        }
    }

    /// Create a service backed by the production sled database.
    pub fn production() -> Self {
        let db = GenericDatabase::new(&DATABASE, "achievements").unwrap_or_else(|e| {
            panic!("Failed to initialize achievements database: {}", e);
        });
        Self::new(db)
    }

    /// Check if an achievement is unlocked.
    pub fn is_unlocked(&self, achievement: Achievement) -> bool {
        self.cache
            .lock()
            .unwrap()
            .iter()
            .any(|u| u.achievement == achievement)
    }

    /// Unlock an achievement.
    /// Returns true if newly unlocked, false if already unlocked.
    pub fn unlock(&self, achievement: Achievement) -> bool {
        let mut cache = self.cache.lock().unwrap();

        if cache.iter().any(|u| u.achievement == achievement) {
            return false;
        }

        let unlock = AchievementUnlock {
            achievement,
            unlocked_at: chrono::Utc::now(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        if let Err(e) = self.db.insert(&unlock) {
            error!("Failed to save achievement unlock: {}", e);
            return false;
        }

        cache.push(unlock);
        true
    }

    /// Revoke achievements that were unlocked before a bugfix version.
    /// Should be called once at app startup.
    pub fn run_revoke_schedule(&self) {
        use crate::utils::version::is_older_than;
        let mut cache = self.cache.lock().unwrap();
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
                        return false;
                    }
                }
                true
            });
        }

        for id in &revoked {
            if let Err(e) = self.db.remove(id) {
                error!("Failed to revoke achievement '{}': {}", id, e);
            }
        }

        if !revoked.is_empty() {
            log::info!("Revoked {} achievement(s): {:?}", revoked.len(), revoked);
        }
    }

    /// Get all unlocked achievements.
    pub fn get_all_unlocked(&self) -> Vec<AchievementUnlock> {
        self.cache.lock().unwrap().clone()
    }

    /// Get count of unlocked achievements.
    pub fn unlock_count(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Check if any achievements are unlocked (for tab visibility).
    pub fn has_any_unlocked(&self) -> bool {
        self.unlock_count() > 0
    }

    /// Clear all achievement records. Useful for resetting state.
    pub fn reset_all(&self) -> Result<(), String> {
        self.db
            .clear()
            .map_err(|e| format!("Failed to clear achievements database: {}", e))?;
        self.cache.lock().unwrap().clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_service() -> AchievementService {
        let tmp_db = crate::storage::database::Database::temporary();
        let db = GenericDatabase::new(&tmp_db, "achievements").expect("temp achievements db");
        AchievementService::new(db)
    }

    #[test]
    fn test_unlock_new_achievement() {
        let svc = temp_service();
        assert!(svc.unlock(Achievement::ChroniesApprentice));
        assert!(svc.is_unlocked(Achievement::ChroniesApprentice));
    }

    #[test]
    fn test_unlock_idempotent() {
        let svc = temp_service();
        assert!(svc.unlock(Achievement::ChroniesApprentice));
        assert!(!svc.unlock(Achievement::ChroniesApprentice)); // second call returns false
        assert_eq!(svc.unlock_count(), 1);
    }

    #[test]
    fn test_is_unlocked_false_initially() {
        let svc = temp_service();
        assert!(!svc.is_unlocked(Achievement::ChroniesApprentice));
        assert!(!svc.has_any_unlocked());
    }

    #[test]
    fn test_unlock_multiple_achievements() {
        let svc = temp_service();
        svc.unlock(Achievement::ChroniesApprentice);
        svc.unlock(Achievement::NightOwl);
        assert_eq!(svc.unlock_count(), 2);
        assert!(svc.is_unlocked(Achievement::ChroniesApprentice));
        assert!(svc.is_unlocked(Achievement::NightOwl));
    }

    #[test]
    fn test_reset_all() {
        let svc = temp_service();
        svc.unlock(Achievement::ChroniesApprentice);
        svc.unlock(Achievement::NightOwl);
        assert_eq!(svc.unlock_count(), 2);
        svc.reset_all().unwrap();
        assert_eq!(svc.unlock_count(), 0);
        assert!(!svc.has_any_unlocked());
    }

    #[test]
    fn test_revoke_schedule_removes_old_achievement() {
        let svc = temp_service();
        // Manually insert an unlock with an old version
        let old_unlock = AchievementUnlock {
            achievement: Achievement::GitSquashMaster,
            unlocked_at: chrono::Utc::now(),
            app_version: "0.1.0-beta.1".to_string(), // older than threshold 0.1.0-beta.3
        };
        svc.db.insert(&old_unlock).unwrap();
        svc.cache.lock().unwrap().push(old_unlock);

        svc.run_revoke_schedule();

        assert!(!svc.is_unlocked(Achievement::GitSquashMaster));
        assert_eq!(svc.unlock_count(), 0);
    }

    #[test]
    fn test_revoke_schedule_keeps_new_achievement() {
        let svc = temp_service();
        let new_unlock = AchievementUnlock {
            achievement: Achievement::GitSquashMaster,
            unlocked_at: chrono::Utc::now(),
            app_version: "0.1.0-beta.4".to_string(), // newer than threshold
        };
        svc.db.insert(&new_unlock).unwrap();
        svc.cache.lock().unwrap().push(new_unlock);

        svc.run_revoke_schedule();

        assert!(svc.is_unlocked(Achievement::GitSquashMaster));
    }

    #[test]
    fn test_persistence_across_instances() {
        let tmp_db = crate::storage::database::Database::temporary();

        {
            let db = GenericDatabase::new(&tmp_db, "achievements").unwrap();
            let svc = AchievementService::new(db);
            svc.unlock(Achievement::ChroniesApprentice);
        }

        // New instance from same DB should load persisted data
        let db2 = GenericDatabase::new(&tmp_db, "achievements").unwrap();
        let svc2 = AchievementService::new(db2);
        assert!(svc2.is_unlocked(Achievement::ChroniesApprentice));
        assert_eq!(svc2.unlock_count(), 1);
    }
}
