use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{self};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct DataStore {
    data: HashMap<String, serde_json::Value>,
}

impl DataStore {

    fn get_collection<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<Vec<T>> {
        self.data
            .get(key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    fn replace_collection<T: Serialize>(&mut self, key: &str, collection: Vec<T>) {
        let serialized = serde_json::to_value(collection).unwrap();
        self.data.insert(key.to_string(), serialized);
    }
}

pub struct FileStorage {
    file_path: String,
}

impl FileStorage {
    fn new(file_path: &str) -> Self {
        FileStorage {
            file_path: file_path.to_string(),
        }
    }

    fn save_collection<T: Serialize>(
        &self,
        key: &str,
        collection: Vec<T>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut data = self.load()?;
        data.replace_collection(key, collection);
        self.save(&data)
    }

    fn save(&self, data: &DataStore) -> Result<(), Box<dyn std::error::Error>> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.file_path)
            .unwrap();

        serde_json::to_writer(file, &data)?;
        Ok(())
    }

    fn load(&self) -> Result<DataStore, Box<dyn std::error::Error>> {
        if !Path::new(&self.file_path).exists() {
            let empty_data = DataStore::default();
            self.save(&empty_data)?;
            return Ok(empty_data);
        }

        let mut file = File::open(&self.file_path)?;
        let mut data = String::new();
        file.read_to_string(&mut data)?;

        let deserialized_data: DataStore = serde_json::from_str(&data)?;
        Ok(deserialized_data)
    }

    fn load_collection<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<Vec<T>>, Box<dyn std::error::Error>> {
        let data = self.load()?;
        let collection = data.get_collection::<T>(key);
        Ok(collection)
    }

    pub fn load_data<T: for<'de> Deserialize<'de>>(key: &str) -> Option<Vec<T>> {
        Self::instance()
            .load_collection::<T>(key)
            .unwrap_or_else(|_| Some(Vec::new()))
    }

    pub fn save_data<T: Serialize>(key: &str, collection: Vec<T>) {
        Self::instance().save_collection(key, collection).unwrap();
    }

    pub fn instance() -> &'static Self {
        static INSTANCE: Lazy<FileStorage> = Lazy::new(|| FileStorage::new("data_store.json"));
        &INSTANCE
    }
}
