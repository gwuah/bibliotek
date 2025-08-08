use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use serde_yaml;
use std::fs;

#[derive(Parser, Debug)]
#[command(name = "bibliotek")]
#[command(about = "Runs the bibliotek service", long_about = None)]
pub struct Cli {
    #[arg(short = 'c', long = "config")]
    pub config_path: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct App {
    database: String,
    schema: String,
    port: i32,
}

impl App {
    pub fn get_db(&self) -> &str {
        return &self.database;
    }

    pub fn get_port(&self) -> i32 {
        return self.port;
    }

    pub fn get_schema_path(&self) -> &str {
        return &self.schema;
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub app: App,
}

impl Config {
    pub fn new(path: &str) -> Result<Self> {
        let cfg = Config::load_config(path)?;
        Ok(cfg)
    }

    fn load_config(path: &str) -> Result<Config> {
        let yaml_str = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&yaml_str)?;
        Ok(config)
    }
}
