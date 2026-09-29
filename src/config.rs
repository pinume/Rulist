use crate::auth::rand_string;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
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
    #[serde(default = "default_robots_txt")]
    pub robots_txt: String,
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
    vec!["/\\/README.md/i".to_string()]
}

const fn default_package_download() -> bool {
    true
}

fn default_robots_txt() -> String {
    "User-agent: *\nAllow: /\n".to_string()
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            site_title: default_site_title(),
            logo: default_logo(),
            favicon: String::new(),
            main_color: default_main_color(),
            hide_files: default_hide_files(),
            package_download: default_package_download(),
            robots_txt: default_robots_txt(),
            announcement: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub jwt_secret: String,
    #[serde(default = "default_token_expires_in")]
    pub token_expires_in: u32,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub scheme: SchemeConfig,
    #[serde(default)]
    pub site: SiteConfig,
}

fn default_secret() -> String {
    rand_string(32)
}

const fn default_token_expires_in() -> u32 {
    48
}

impl Default for Config {
    fn default() -> Self {
        Self {
            jwt_secret: default_secret(),
            token_expires_in: default_token_expires_in(),
            database: DatabaseConfig::default(),
            scheme: SchemeConfig::default(),
            site: SiteConfig::default(),
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
            Ok((serde_json::from_str(&content)?, config_path))
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
