use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::auth::rand_string;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_file")]
    pub db_file: String,
    #[serde(default = "default_table_prefix")]
    pub table_prefix: String,
}

fn default_db_file() -> String {
    "data.db".to_string()
}

fn default_table_prefix() -> String {
    "x_".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_file: default_db_file(),
            table_prefix: default_table_prefix(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeConfig {
    #[serde(default = "default_address")]
    pub address: String,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default)]
    pub unix_file: String,
}

fn default_address() -> String {
    "0.0.0.0".to_string()
}

fn default_http_port() -> u16 {
    5244
}

impl Default for SchemeConfig {
    fn default() -> Self {
        Self {
            address: default_address(),
            http_port: default_http_port(),
            unix_file: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default = "default_log_name")]
    pub name: String,
    #[serde(default = "default_max_size")]
    pub max_size: usize,
}

fn default_true() -> bool {
    true
}

fn default_log_name() -> String {
    "log/log.log".to_string()
}

fn default_max_size() -> usize {
    50
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enable: default_true(),
            name: default_log_name(),
            max_size: default_max_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    #[serde(default = "default_token_expires_in")]
    pub token_expires_in: u32,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub scheme: SchemeConfig,
    #[serde(default = "default_temp_dir")]
    pub temp_dir: String,
    #[serde(default)]
    pub log: LogConfig,
}

fn default_jwt_secret() -> String {
    rand_string(32)
}

fn default_token_expires_in() -> u32 {
    48
}

fn default_temp_dir() -> String {
    "temp".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            jwt_secret: default_jwt_secret(),
            token_expires_in: default_token_expires_in(),
            database: DatabaseConfig::default(),
            scheme: SchemeConfig::default(),
            temp_dir: default_temp_dir(),
            log: LogConfig::default(),
        }
    }
}

impl Config {
    /// Load existing config.json from data directory or generate a new one
    pub fn load_or_create(data_dir: &Path) -> Result<(Self, PathBuf), anyhow::Error> {
        fs::create_dir_all(data_dir)?;
        let config_path = data_dir.join("config.json");

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok((config, config_path))
        } else {
            let config = Config::default();
            let json_str = serde_json::to_string_pretty(&config)?;
            fs::write(&config_path, json_str)?;
            Ok((config, config_path))
        }
    }

    /// Resolve relative database path against data directory
    pub fn resolved_db_path(&self, data_dir: &Path) -> PathBuf {
        let p = Path::new(&self.database.db_file);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            data_dir.join(p)
        }
    }
}
