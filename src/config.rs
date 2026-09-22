use crate::auth::rand_string;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_file")]
    pub db_file: String,
}

fn default_db_file() -> String {
    "data.db".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_file: default_db_file(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeConfig {
    #[serde(default = "default_address")]
    pub address: String,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
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
}

fn default_jwt_secret() -> String {
    rand_string(32)
}

fn default_token_expires_in() -> u32 {
    48
}

impl Default for Config {
    fn default() -> Self {
        Self {
            jwt_secret: default_jwt_secret(),
            token_expires_in: default_token_expires_in(),
            database: DatabaseConfig::default(),
            scheme: SchemeConfig::default(),
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
            return p.to_path_buf();
        }

        let direct = data_dir.join(p);
        if direct.exists() {
            return direct;
        }

        if let Some(file_name) = p.file_name() {
            let flat = data_dir.join(file_name);
            if flat.exists() {
                return flat;
            }
        }

        if p.exists() {
            return p.to_path_buf();
        }

        direct
    }
}
