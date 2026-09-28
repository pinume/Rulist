use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

use crate::auth::{hash_password, rand_string, rand_token};
use crate::model::{ROLE_ADMIN, User};

pub type DbPool = Pool<Sqlite>;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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

    if db_path.exists() {
        fs::set_permissions(db_path, fs::Permissions::from_mode(0o600))?;
    }

    MIGRATOR
        .run(&pool)
        .await
        .context("failed to run SQLite migrations")?;

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("DELETE FROM `x_revoked_tokens` WHERE `expires_at` < ?")
        .bind(now_ts)
        .execute(&pool)
        .await?;

    validate_admin_invariants(&pool).await?;
    seed_settings(&pool).await?;
    seed_admin(&pool).await?;

    Ok(pool)
}

async fn validate_admin_invariants(pool: &DbPool) -> Result<()> {
    let invalid_admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM `x_users` WHERE `role` = ? AND (`username` != 'admin' OR `disabled` != 0)",
    )
    .bind(ROLE_ADMIN)
    .fetch_one(pool)
    .await?;
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_users` WHERE `role` = ?")
        .bind(ROLE_ADMIN)
        .fetch_one(pool)
        .await?;

    if invalid_admins > 0 || admin_count > 1 {
        bail!("database must contain at most one enabled administrator named admin");
    }
    if admin_count == 0 && get_user_by_name(pool, "admin").await?.is_some() {
        bail!("username admin is already assigned to a non-administrator");
    }

    Ok(())
}

async fn seed_settings(pool: &DbPool) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES
            ('site_title', 'Rulist', 'string', 0, 0),
            ('version', 'v0.1.2-rust', 'string', 0, 2),
            ('announcement', '', 'text', 0, 0),
            ('robots_txt', 'User-agent: *\nAllow: /', 'text', 0, 0),
            ('logo', 'rulist.svg'||char(10)||'rulist-dark.svg', 'text', 1, 0),
            ('favicon', '', 'string', 1, 0),
            ('main_color', '#1890ff', 'string', 1, 0),
            ('hide_files', '/\/README.md/i', 'text', 0, 0),
            ('home_container', 'max_980px', 'select', 1, 0),
            ('home_icon', '🏠', 'string', 1, 0),
            ('package_download', 'true', 'bool', 0, 0),
            ('sign_all', 'false', 'bool', 0, 0)
        "#,
    )
    .execute(pool)
    .await?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT `value` FROM `x_setting_items` WHERE `key` = 'token'")
            .fetch_optional(pool)
            .await?;

    if existing.is_none() {
        sqlx::query(
            "INSERT INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES ('token', ?, 'string', 0, 1)",
        )
        .bind(rand_token())
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn seed_admin(pool: &DbPool) -> Result<()> {
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_users` WHERE `role` = ?")
        .bind(ROLE_ADMIN)
        .fetch_one(pool)
        .await?;

    if admin_count == 0 {
        let initial_pwd = rand_string(16);
        let encoded_pwd = hash_password(&initial_pwd);
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query(
            r#"
            INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `base_path`, `role`, `disabled`, `permission`, `password_unset`)
            VALUES ('admin', ?, ?, '/', ?, 0, 0, 0)
            "#,
        )
        .bind(&encoded_pwd)
        .bind(now_ts)
        .bind(ROLE_ADMIN)
        .execute(pool)
        .await?;

        println!(
            "\n==================================================================\n\
             Initial admin user created:\n\
             Username: admin\n\
             Password: {}\n\
             Save this password and change it after first login.\n\
             You can reset it later from the Rulist interactive console.\n\
             ==================================================================\n",
            initial_pwd
        );
    }

    Ok(())
}

pub async fn get_admin(pool: &DbPool) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `role` = ? LIMIT 1")
        .bind(ROLE_ADMIN)
        .fetch_optional(pool)
        .await?;
    if let Some(ref mut user) = user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn set_user_password(pool: &DbPool, username: &str, new_password: &str) -> Result<()> {
    let user = get_user_by_name(pool, username)
        .await?
        .context("user not found")?;
    let encoded_pwd = hash_password(new_password);
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    sqlx::query(
        "UPDATE `x_users` SET `pwd_hash` = ?, `pwd_ts` = MAX(`pwd_ts` + 1, ?), `password_unset` = ? WHERE `id` = ?",
    )
    .bind(encoded_pwd)
    .bind(now_ts)
    .bind(new_password.is_empty())
    .bind(user.id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn set_admin_password(pool: &DbPool, new_password: &str) -> Result<()> {
    set_user_password(pool, "admin", new_password).await
}

pub async fn get_setting(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT `value` FROM `x_setting_items` WHERE `key` = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value)
}

pub async fn get_public_settings(pool: &DbPool) -> Result<HashMap<String, String>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT `key`, `value` FROM `x_setting_items` WHERE `flag` IN (0, 2)",
    )
    .fetch_all(pool)
    .await?;

    let mut settings: HashMap<_, _> = rows.into_iter().collect();
    settings.insert(
        "version".to_string(),
        format!("v{}-rust", env!("CARGO_PKG_VERSION")),
    );
    Ok(settings)
}

pub async fn get_user_by_name(pool: &DbPool, username: &str) -> Result<Option<User>> {
    let mut user =
        sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `username` = ? LIMIT 1")
            .bind(username)
            .fetch_optional(pool)
            .await?;
    if let Some(ref mut user) = user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn get_all_users(pool: &DbPool) -> Result<Vec<User>> {
    let mut users = sqlx::query_as::<_, User>("SELECT * FROM `x_users` ORDER BY `id` ASC")
        .fetch_all(pool)
        .await?;
    for user in &mut users {
        user.update_otp();
    }
    Ok(users)
}

pub async fn get_user_by_id(pool: &DbPool, id: i64) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some(ref mut user) = user {
        user.update_otp();
    }
    Ok(user)
}

pub async fn get_storages(pool: &DbPool) -> Result<Vec<crate::model::Storage>> {
    let storages = sqlx::query_as::<_, crate::model::Storage>(
        "SELECT * FROM `x_storages` WHERE `disabled` = 0",
    )
    .fetch_all(pool)
    .await?;
    Ok(storages)
}

pub fn compute_local_path(base_path: &str, storages: &[crate::model::Storage]) -> String {
    let mut matched: Option<&crate::model::Storage> = None;
    for storage in storages {
        if (base_path == storage.mount_path
            || base_path.starts_with(&format!("{}/", storage.mount_path.trim_end_matches('/'))))
            && (matched.is_none()
                || storage.mount_path.len() > matched.expect("matched storage").mount_path.len())
        {
            matched = Some(storage);
        }
    }

    if let Some(storage) = matched
        && let Some(ref addition) = storage.addition
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(addition)
    {
        let root = value
            .get("root_folder_path")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if !root.is_empty() {
            let sub = base_path
                .trim_start_matches(&storage.mount_path)
                .trim_start_matches('/');
            if sub.is_empty() {
                return root.to_string();
            }
            return format!("{}/{}", root.trim_end_matches('/'), sub);
        }
    }

    String::new()
}

pub async fn get_all_storages(pool: &DbPool) -> Result<Vec<crate::model::Storage>> {
    let storages = sqlx::query_as::<_, crate::model::Storage>(
        "SELECT * FROM `x_storages` ORDER BY `order` ASC, `id` ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(storages)
}

pub async fn get_all_settings(pool: &DbPool) -> Result<Vec<(String, String, String)>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT `key`, `value`, `type` FROM `x_setting_items` ORDER BY `group` ASC, `key` ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn set_setting(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES (?, ?, 'string', 0, 0)\n         ON CONFLICT(`key`) DO UPDATE SET `value` = excluded.`value`",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_user(pool: &DbPool, user_id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM `x_storages` WHERE `mount_path` = ?")
        .bind(format!("/.users/{user_id}"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn enable_user_2fa(
    pool: &DbPool,
    user_id: i64,
    secret: &str,
    accepted_step: i64,
) -> Result<()> {
    let result = sqlx::query(
        "UPDATE `x_users` SET `otp_secret` = ?, `last_otp_step` = ? WHERE `id` = ? AND (`otp_secret` IS NULL OR TRIM(`otp_secret`) = '')",
    )
    .bind(secret)
    .bind(accepted_step)
    .bind(user_id)
    .execute(pool)
    .await?;

    if result.rows_affected() != 1 {
        bail!("user not found or 2FA is already enabled");
    }
    Ok(())
}

pub async fn cancel_user_2fa(pool: &DbPool, user_id: i64) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `otp_secret` = '', `last_otp_step` = -1 WHERE `id` = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
pub async fn set_user_disabled(pool: &DbPool, user_id: i64, disabled: bool) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `disabled` = ? WHERE `id` = ?")
        .bind(if disabled { 1 } else { 0 })
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_user_permission(pool: &DbPool, user_id: i64, permission: i32) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `permission` = ? WHERE `id` = ?")
        .bind(permission)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_user_dir(pool: &DbPool, user_id: i64, local_path: &str) -> Result<()> {
    let user_mount = format!("/.users/{user_id}");
    let addition = serde_json::json!({ "root_folder_path": local_path }).to_string();

    let mut tx = pool.begin().await?;
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT `id` FROM `x_storages` WHERE `mount_path` = ? LIMIT 1")
            .bind(&user_mount)
            .fetch_optional(&mut *tx)
            .await?;

    if exists.is_some() {
        sqlx::query("UPDATE `x_storages` SET `addition` = ? WHERE `mount_path` = ?")
            .bind(&addition)
            .bind(&user_mount)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `order`, `addition`, `status`, `disabled`) VALUES (?, 0, ?, 'work', 0)",
        )
        .bind(&user_mount)
        .bind(&addition)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
        .bind(&user_mount)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn create_user_direct(
    pool: &DbPool,
    username: &str,
    password: &str,
    role: i32,
    local_path: Option<&str>,
    permission: i32,
    disabled: bool,
) -> Result<i64> {
    let encoded_pwd = hash_password(password);
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `base_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES (?, ?, ?, '/', ?, ?, ?, ?)",
    )
    .bind(username)
    .bind(&encoded_pwd)
    .bind(now_ts)
    .bind(role)
    .bind(if disabled { 1 } else { 0 })
    .bind(permission)
    .bind(password.is_empty())
    .execute(&mut *tx)
    .await?;

    let new_id = result.last_insert_rowid();

    if role != ROLE_ADMIN {
        let user_mount = format!("/.users/{new_id}");
        sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
            .bind(&user_mount)
            .bind(new_id)
            .execute(&mut *tx)
            .await?;

        if let Some(local_path) = local_path {
            let addition = serde_json::json!({ "root_folder_path": local_path }).to_string();
            sqlx::query(
                "INSERT INTO `x_storages` (`mount_path`, `order`, `addition`, `status`, `disabled`) VALUES (?, 0, ?, 'work', 0)",
            )
            .bind(&user_mount)
            .bind(&addition)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(new_id)
}
