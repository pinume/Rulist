use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};

use crate::auth::{encode_argon2_hash, rand_string, rand_token, static_hash};
use crate::model::{User, ROLE_ADMIN};

pub type DbPool = Pool<Sqlite>;

pub async fn init_db(db_path: &Path) -> Result<DbPool> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn_str = format!("sqlite://{}", db_path.to_string_lossy());
    let opts = SqliteConnectOptions::from_str(&conn_str)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(opts)
        .await
        .context("failed to connect to SQLite database")?;

    // Create tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS `x_users` (
            `id` INTEGER PRIMARY KEY AUTOINCREMENT,
            `username` TEXT NOT NULL UNIQUE,
            `pwd_hash` TEXT NOT NULL,
            `pwd_ts` INTEGER NOT NULL,
            `salt` TEXT NOT NULL,
            `password` TEXT,
            `base_path` TEXT NOT NULL DEFAULT '/',
            `role` INTEGER NOT NULL DEFAULT 0,
            `disabled` NUMERIC NOT NULL DEFAULT 0,
            `permission` INTEGER NOT NULL DEFAULT 0,
            `otp_secret` TEXT,
            `sso_id` TEXT
        );

        CREATE TABLE IF NOT EXISTS `x_setting_items` (
            `key` TEXT PRIMARY KEY,
            `value` TEXT NOT NULL,
            `help` TEXT,
            `type` TEXT NOT NULL DEFAULT 'string',
            `options` TEXT,
            `group` INTEGER NOT NULL DEFAULT 0,
            `flag` INTEGER NOT NULL DEFAULT 0,
            `index` INTEGER
        );

        CREATE TABLE IF NOT EXISTS `x_storages` (
            `id` INTEGER PRIMARY KEY AUTOINCREMENT,
            `mount_path` TEXT NOT NULL UNIQUE,
            `order` INTEGER NOT NULL DEFAULT 0,
            `driver` TEXT NOT NULL,
            `cache_expiration` INTEGER NOT NULL DEFAULT 0,
            `status` TEXT,
            `addition` TEXT,
            `remark` TEXT,
            `disabled` NUMERIC NOT NULL DEFAULT 0,
            `enable_sign` NUMERIC NOT NULL DEFAULT 0,
            `order_by` TEXT,
            `order_direction` TEXT
        );

        CREATE TABLE IF NOT EXISTS `x_meta` (
            `id` INTEGER PRIMARY KEY AUTOINCREMENT,
            `path` TEXT NOT NULL UNIQUE,
            `password` TEXT,
            `p_sub` NUMERIC NOT NULL DEFAULT 0,
            `hide` TEXT,
            `h_sub` NUMERIC NOT NULL DEFAULT 0,
            `readme` TEXT,
            `r_sub` NUMERIC NOT NULL DEFAULT 0,
            `header` TEXT,
            `header_sub` NUMERIC NOT NULL DEFAULT 0
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("failed to initialize SQLite schema")?;

    seed_settings(&pool).await?;
    seed_admin(&pool).await?;

    Ok(pool)
}

async fn seed_settings(pool: &DbPool) -> Result<()> {
    let defaults = vec![
        ("site_title", "Rulist", "string", 0, 0),
        ("version", "v0.1.0-rust", "string", 0, 2),
        ("announcement", "", "text", 0, 0),
        ("robots_txt", "User-agent: *\nAllow: /", "text", 0, 0),
        ("logo", "favicon.ico", "text", 1, 0),
        ("favicon", "", "string", 1, 0),
        ("main_color", "#1890ff", "string", 1, 0),
        ("hide_files", r#"/\/README.md/i"#, "text", 0, 0),
        ("home_container", "max_980px", "select", 1, 0),
        ("home_icon", "🏠", "string", 1, 0),
        ("package_download", "true", "bool", 0, 0),
        ("sso_login_enabled", "false", "bool", 0, 0),
        ("sign_all", "false", "bool", 0, 0),
    ];

    for (k, v, ty, grp, flag) in defaults {
        sqlx::query(
            "INSERT OR IGNORE INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(k)
        .bind(v)
        .bind(ty)
        .bind(grp)
        .bind(flag)
        .execute(pool)
        .await?;
    }

    // Ensure a default token exists
    let existing_token: Option<String> = sqlx::query_scalar(
        "SELECT `value` FROM `x_setting_items` WHERE `key` = 'token'",
    )
    .fetch_optional(pool)
    .await?;

    if existing_token.is_none() {
        let token = rand_token();
        sqlx::query(
            "INSERT INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES ('token', ?, 'string', 0, 1)",
        )
        .bind(token)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn seed_admin(pool: &DbPool) -> Result<()> {
    let admin_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM `x_users` WHERE `role` = ?",
    )
    .bind(ROLE_ADMIN)
    .fetch_one(pool)
    .await?;

    if admin_count == 0 {
        let mut admin_password = rand_string(8);
        if let Ok(env_pass) = env::var("OPENLIST_ADMIN_PASSWORD") {
            if !env_pass.is_empty() {
                admin_password = env_pass;
            }
        }

        let salt = rand_string(16);
        let s_hash = static_hash(&admin_password);
        let encoded_pwd = encode_argon2_hash(&s_hash, &salt);
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query(
            r#"
            INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`)
            VALUES ('admin', ?, ?, ?, '/', ?, 0, 0)
            "#,
        )
        .bind(&encoded_pwd)
        .bind(now_ts)
        .bind(&salt)
        .bind(ROLE_ADMIN)
        .execute(pool)
        .await?;

        println!("Successfully created the admin user and the initial password is: {}", admin_password);
    }

    Ok(())
}

pub async fn get_admin(pool: &DbPool) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM `x_users` WHERE `role` = ? LIMIT 1",
    )
    .bind(ROLE_ADMIN)
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

pub async fn set_admin_password(pool: &DbPool, new_password: &str) -> Result<()> {
    let admin = get_admin(pool).await?.context("admin user not found")?;

    let salt = rand_string(16);
    let s_hash = static_hash(new_password);
    let encoded_pwd = encode_argon2_hash(&s_hash, &salt);
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    sqlx::query(
        "UPDATE `x_users` SET `pwd_hash` = ?, `pwd_ts` = ?, `salt` = ? WHERE `id` = ?",
    )
    .bind(encoded_pwd)
    .bind(now_ts)
    .bind(salt)
    .bind(admin.id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_setting(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let val: Option<String> = sqlx::query_scalar(
        "SELECT `value` FROM `x_setting_items` WHERE `key` = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(val)
}

pub async fn get_public_settings(pool: &DbPool) -> Result<HashMap<String, String>> {
    let rows = sqlx::query("SELECT `key`, `value` FROM `x_setting_items` WHERE `flag` IN (0, 2)")
        .fetch_all(pool)
        .await?;

    let mut map = HashMap::new();
    for row in rows {
        let k: String = row.get("key");
        let v: String = row.get("value");
        map.insert(k, v);
    }
    Ok(map)
}

pub async fn get_user_by_name(pool: &DbPool, username: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM `x_users` WHERE `username` = ? LIMIT 1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

#[allow(dead_code)]
pub async fn get_meta_for_path(pool: &DbPool, path: &str) -> Result<Option<crate::model::Meta>> {
    let meta = sqlx::query_as::<_, crate::model::Meta>(
        "SELECT * FROM `x_meta` WHERE `path` = ? LIMIT 1",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?;
    Ok(meta)
}

pub async fn get_all_users(pool: &DbPool) -> Result<Vec<User>> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM `x_users` ORDER BY `id` ASC")
        .fetch_all(pool)
        .await?;
    Ok(users)
}

pub async fn get_user_by_id(pool: &DbPool, id: i64) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

pub async fn delete_user_by_id(pool: &DbPool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_storages(pool: &DbPool) -> Result<Vec<crate::model::Storage>> {
    let storages = sqlx::query_as::<_, crate::model::Storage>("SELECT * FROM `x_storages` WHERE `disabled` = 0")
        .fetch_all(pool)
        .await?;
    Ok(storages)
}

pub fn compute_local_path(base_path: &str, storages: &[crate::model::Storage]) -> String {
    let mut matched: Option<&crate::model::Storage> = None;
    for s in storages {
        if s.driver == "Local" && (base_path == s.mount_path || base_path.starts_with(&format!("{}/", s.mount_path.trim_end_matches('/')))) {
            if matched.is_none() || s.mount_path.len() > matched.unwrap().mount_path.len() {
                matched = Some(s);
            }
        }
    }

    if let Some(s) = matched {
        if let Some(ref addition_str) = s.addition {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(addition_str) {
                let root = val.get("root_folder_path")
                    .or_else(|| val.get("root_folder"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !root.is_empty() {
                    let sub = base_path.trim_start_matches(&s.mount_path).trim_start_matches('/');
                    if sub.is_empty() {
                        return root.to_string();
                    } else {
                        return format!("{}/{}", root.trim_end_matches('/'), sub);
                    }
                }
            }
        }
    }
    String::new()
}

