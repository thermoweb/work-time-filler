use crate::models::tiered_achievement::TieredProgress;
use crate::storage::database::{GenericDatabase, DATABASE};
use log::error;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct TieredAchievementService {
    db: GenericDatabase<TieredProgress>,
    cache: Mutex<HashMap<String, u64>>,
}

impl TieredAchievementService {
    pub fn new(db: GenericDatabase<TieredProgress>) -> Self {
        let all = db.get_all().unwrap_or_else(|e| {
            error!("Failed to load tiered achievements: {}", e);
            Vec::new()
        });
        let cache = all.into_iter().map(|p| (p.id, p.count)).collect();
        Self {
            db,
            cache: Mutex::new(cache),
        }
    }

    pub fn production() -> Self {
        let db = GenericDatabase::new(&DATABASE, "tiered_achievements").unwrap_or_else(|e| {
            panic!("Failed to initialize tiered achievements database: {}", e);
        });
        Self::new(db)
    }

    /// Increment a counter. Returns (old_count, new_count).
    pub fn increment(&self, id: &str, amount: u64) -> (u64, u64) {
        let mut cache = self.cache.lock().unwrap();
        let old = *cache.get(id).unwrap_or(&0);
        let new = old + amount;
        cache.insert(id.to_string(), new);
        let progress = TieredProgress {
            id: id.to_string(),
            count: new,
        };
        if let Err(e) = self.db.insert(&progress) {
            error!("Failed to save tiered progress for '{}': {}", id, e);
        }
        (old, new)
    }

    /// Set counter to value only if greater than current (used for migration).
    pub fn set_if_greater(&self, id: &str, value: u64) {
        let current = self.get_count(id);
        if value > current {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(id.to_string(), value);
            let progress = TieredProgress {
                id: id.to_string(),
                count: value,
            };
            if let Err(e) = self.db.insert(&progress) {
                error!("Failed to save tiered progress for '{}': {}", id, e);
            }
        }
    }

    pub fn get_count(&self, id: &str) -> u64 {
        *self.cache.lock().unwrap().get(id).unwrap_or(&0)
    }

    pub fn get_all_progress(&self) -> HashMap<String, u64> {
        self.cache.lock().unwrap().clone()
    }

    pub fn reset_all(&self) -> Result<(), String> {
        self.db
            .clear()
            .map_err(|e| format!("Failed to clear tiered achievements: {}", e))?;
        self.cache.lock().unwrap().clear();
        Ok(())
    }
}
