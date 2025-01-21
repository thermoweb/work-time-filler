use config::{Config as ConfigLoader, File};
use serde::Deserialize;
use std::error::Error;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub jira: JiraConfig,
}

#[derive(Debug, Deserialize)]
pub struct JiraConfig {
    pub base_url: String,
    pub username: String,
    pub api_token: String,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let builder = ConfigLoader::builder()
            .add_source(File::with_name("config").required(false));
        let config = builder.build()?.try_deserialize()?;
        Ok(config)
    }
}
