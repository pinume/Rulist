use crate::auth::rand_string;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
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
    #[serde(default)]
    pub allow_cors: bool,
}
fn default_address() -> String {
    "127.0.0.1".to_string()
}
const fn default_http_port() -> u16 {
    5244
}
impl Default for SchemeConfig {
    fn default() -> Self {
        Self {
            address: default_address(),
            http_port: default_http_port(),
            allow_cors: false,
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
const fn default_token_expires_in() -> u32 {
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
    pub fn load_or_create(data_dir: &Path) -> Result<(Self, PathBuf), anyhow::Error> {
        fs::create_dir_all(data_dir)?;
        fs::set_permissions(data_dir, fs::Permissions::from_mode(0o700))?;
        let config_path = data_dir.join("config.json");

        if config_path.exists() {
            fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600))?;
            let content = fs::read_to_string(&config_path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok((config, config_path))
        } else {
            let config = Config::default();
            let json_str = serde_json::to_string_pretty(&config)?;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            options.mode(0o600);
            let mut file = options.open(&config_path)?;
            file.write_all(json_str.as_bytes())?;
            fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600))?;
            Ok((config, config_path))
        }
    }

    pub fn resolved_db_path(&self, data_dir: &Path) -> PathBuf {
        let path = Path::new(&self.database.db_file);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            data_dir.join(path)
        }
    }
}
