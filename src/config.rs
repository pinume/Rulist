use crate::auth::rand_string;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_address")]
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub allow_cors: bool,
}

fn default_address() -> String {
    "127.0.0.1".to_string()
}

const fn default_port() -> u16 {
    5244
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: default_address(),
            port: default_port(),
            allow_cors: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    pub jwt_secret: String,
    pub signing_secret: String,
    #[serde(default = "default_token_expires_hours")]
    pub token_expires_hours: u32,
}

fn default_secret() -> String {
    rand_string(32)
}

const fn default_token_expires_hours() -> u32 {
    48
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            jwt_secret: default_secret(),
            signing_secret: default_secret(),
            token_expires_hours: default_token_expires_hours(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiConfig {
    #[serde(default = "default_site_title")]
    pub site_title: String,
    #[serde(default = "default_logo")]
    pub logo: String,
    #[serde(default)]
    pub favicon: String,
    #[serde(default = "default_main_color")]
    pub main_color: String,
    #[serde(default = "default_hide_files")]
    pub hide_files: Vec<String>,
    #[serde(default = "default_package_download")]
    pub package_download: bool,
    #[serde(default)]
    pub announcement: String,
}

fn default_site_title() -> String {
    "Rulist".to_string()
}

fn default_logo() -> String {
    "rulist.svg\nrulist-dark.svg".to_string()
}

fn default_main_color() -> String {
    "#1890ff".to_string()
}

fn default_hide_files() -> Vec<String> {
    vec!["/README.md".to_string()]
}

const fn default_package_download() -> bool {
    true
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            site_title: default_site_title(),
            logo: default_logo(),
            favicon: String::new(),
            main_color: default_main_color(),
            hide_files: default_hide_files(),
            package_download: default_package_download(),
            announcement: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_file")]
    pub file: String,
}

fn default_db_file() -> String {
    "data.db".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            file: default_db_file(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub ui: UiConfig,
    pub database: DatabaseConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            security: SecurityConfig::default(),
            ui: UiConfig::default(),
            database: DatabaseConfig::default(),
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
            let config = serde_json::from_str(&content)?;
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
        let path = Path::new(&self.database.file);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            data_dir.join(path)
        }
    }
}
