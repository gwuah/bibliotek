use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use serde_yaml;
use std::env;
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

#[derive(Debug, Deserialize, Default)]
pub struct Storage {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub aws_endpoint_url_s3: String,
    pub aws_endpoint_url_iam: String,
    pub aws_region: String,
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
    pub storage: Storage,
}

impl Config {
    pub fn new(path: &str) -> Result<Self> {
        let cfg = Config::load_config(path)?;
        Ok(cfg)
    }

    fn load_config(path: &str) -> Result<Config> {
        let yaml_str = fs::read_to_string(path)?;
        let yaml_with_env = Config::substitute_env_vars(&yaml_str)?;
        let config: Config = serde_yaml::from_str(&yaml_with_env)?;
        Ok(config)
    }

    fn substitute_env_vars(yaml_str: &str) -> Result<String> {
        let mut result = yaml_str.to_string();
        let mut offset = 0;

        while let Some(start) = result[offset..].find("${") {
            let actual_start = offset + start;
            if let Some(end) = result[actual_start..].find("}") {
                let var_name = &result[actual_start + 2..actual_start + end];

                // Handle default values like ${VAR:-default}
                let env_value = if let Some(default_start) = var_name.find(":-") {
                    let actual_var = &var_name[..default_start];
                    let default_val = &var_name[default_start + 2..];
                    env::var(actual_var).unwrap_or_else(|_| default_val.to_string())
                } else {
                    env::var(var_name).unwrap_or_else(|_| {
                        println!("Warning: Environment variable '{}' not found", var_name);
                        String::new()
                    })
                };

                result.replace_range(actual_start..actual_start + end + 1, &env_value);
                offset = actual_start + env_value.len();
            } else {
                break;
            }
        }

        Ok(result)
    }
}
