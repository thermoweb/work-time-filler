use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sled::{Db, Tree};
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use std::{env, fmt};

pub trait Identifiable {
    fn get_id(&self) -> String;
}
pub trait CollectionItem: Serialize + for<'de> Deserialize<'de> + Clone + Identifiable {}

impl<T: Serialize + for<'de> Deserialize<'de> + Clone + Identifiable> CollectionItem for T {}

#[derive(Debug)]
pub enum DatabaseError {
    NotFound,
    AlreadyExists,
    DatabaseFailure(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::NotFound => write!(f, "Record not found in the database"),
            DatabaseError::AlreadyExists => write!(f, "Record already exists in the database"),
            DatabaseError::DatabaseFailure(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl Error for DatabaseError {}

#[derive(Clone)]
pub struct Database {
    db: Db,
}

impl Database {
    fn new(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    fn get_tree(&self, collection: &str) -> Result<Tree, Box<dyn Error + Send + Sync>> {
        Ok(self.db.open_tree(collection)?)
    }
}

#[derive(Clone)]
pub struct GenericDatabase<T: CollectionItem> {
    tree: Tree,
    _marker: PhantomData<T>,
}

impl<T: CollectionItem> GenericDatabase<T> {
    pub(crate) fn new(
        database: &Database,
        collection_name: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let tree = database.get_tree(collection_name)?;
        Ok(Self {
            tree,
            _marker: PhantomData,
        })
    }

    pub fn clear(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.tree.clear()?;
        self.tree.flush()?;
        Ok(())
    }

    pub fn insert(&self, item: &T) -> Result<(), Box<dyn Error + Send + Sync>> {
        let serialized_item = bincode::serialize(item)?;
        self.tree.insert(item.get_id(), serialized_item)?;
        self.tree.flush()?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<T>, Box<dyn Error + Send + Sync>> {
        if let Some(value) = self.tree.get(key)? {
            let item: T = bincode::deserialize(&value)?;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    pub fn remove(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.tree.remove(key)?;
        self.tree.flush()?;
        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<T>, Box<dyn Error + Send + Sync>> {
        let mut items = Vec::new();
        let mut corrupted_keys = Vec::new();

        for item in self.tree.iter() {
            let (key, value) = item?;
            match bincode::deserialize(&value) {
                Ok(deserialized_item) => items.push(deserialized_item),
                Err(_) => {
                    // Track corrupted record keys for cleanup
                    corrupted_keys.push(key.to_vec());
                    continue;
                }
            }
        }

        // Clean up corrupted records
        if !corrupted_keys.is_empty() {
            eprintln!(
                "Warning: Found {} corrupted record(s), removing them...",
                corrupted_keys.len()
            );
            for key in corrupted_keys {
                self.tree.remove(key)?;
            }
            self.tree.flush()?;
        }

        Ok(items)
    }

    pub fn save_all(&self, items: Vec<T>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut batch = sled::Batch::default();
        for item in items {
            let serialized_item = bincode::serialize(&item)?;
            let key = sled::IVec::from(item.get_id().as_bytes());
            batch.insert(key, serialized_item);
        }

        self.tree.apply_batch(batch)?;
        self.tree.flush()?;
        Ok(())
    }
}

pub static DATABASE: Lazy<Arc<Database>> = Lazy::new(|| {
    let home = env::var("HOME").expect("HOME env var not set");

    // Check for WTF_CONFIG_HOME environment variable
    let path = if let Ok(custom_path) = env::var("WTF_CONFIG_HOME") {
        format!("{}/.wtf_db", custom_path)
    } else {
        format!("{}/.config/wtf/.wtf_db", home)
    };

    match Database::new(&path) {
        Ok(db) => Arc::new(db),
        Err(e) => {
            eprintln!("Failed to open database at {}: {}", path, e);
            eprintln!("\nPossible causes:");
            eprintln!("  1. Another instance of wtf is already running");
            eprintln!("  2. Database files are corrupted");
            eprintln!("  3. Insufficient permissions");
            eprintln!("\nTry:");
            eprintln!("  - Close any other running wtf instances");
            eprintln!("  - Check file permissions: ls -la {}", path);
            panic!("Could not create database: {}", e);
        }
    }
});
