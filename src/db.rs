use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

use crate::auth::{encode_argon2_hash, rand_string, rand_token, static_hash, verify_password};
use crate::model::{ROLE_ADMIN, User};

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
    #[cfg(unix)]
    if db_path.exists() {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(db_path, fs::Permissions::from_mode(0o600))?;
    }

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
            `password_unset` NUMERIC NOT NULL DEFAULT 0,
            `otp_secret` TEXT,
            `last_otp_step` INTEGER NOT NULL DEFAULT -1,
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

        CREATE TABLE IF NOT EXISTS `x_otp_pending` (
            `user_id` INTEGER PRIMARY KEY,
            `secret` TEXT NOT NULL,
            `expires_at` INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS `x_login_attempts` (
            `username_hash` TEXT PRIMARY KEY,
            `failed_count` INTEGER NOT NULL,
            `window_started` INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS `x_revoked_tokens` (
            `jti` TEXT PRIMARY KEY,
            `expires_at` INTEGER NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("failed to initialize SQLite schema")?;

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("DELETE FROM `x_revoked_tokens` WHERE `expires_at` < ?")
        .bind(now_ts)
        .execute(&pool)
        .await?;

    let has_password_unset: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('x_users') WHERE name = 'password_unset'",
    )
    .fetch_one(&pool)
    .await?;
    if has_password_unset == 0 {
        sqlx::query("ALTER TABLE `x_users` ADD COLUMN `password_unset` NUMERIC NOT NULL DEFAULT 0")
            .execute(&pool)
            .await?;
        let users: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT `id`, `pwd_hash`, `salt` FROM `x_users`")
                .fetch_all(&pool)
                .await?;
        for (id, pwd_hash, salt) in users {
            if verify_password("", &pwd_hash, &salt) {
                sqlx::query("UPDATE `x_users` SET `password_unset` = 1 WHERE `id` = ?")
                    .bind(id)
                    .execute(&pool)
                    .await?;
            }
        }
    }

    let has_last_otp_step: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('x_users') WHERE name = 'last_otp_step'",
    )
    .fetch_one(&pool)
    .await?;
    if has_last_otp_step == 0 {
        sqlx::query("ALTER TABLE `x_users` ADD COLUMN `last_otp_step` INTEGER NOT NULL DEFAULT -1")
            .execute(&pool)
            .await?;
    }

    let invalid_admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM `x_users` WHERE `role` = ? AND (`username` != 'admin' OR `disabled` != 0)",
    )
    .bind(ROLE_ADMIN)
    .fetch_one(&pool)
    .await?;
    let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_users` WHERE `role` = ?")
        .bind(ROLE_ADMIN)
        .fetch_one(&pool)
        .await?;
    if invalid_admins > 0 || admin_count > 1 {
        bail!(
            "database must contain at most one enabled administrator named admin; resolve legacy accounts before startup"
        );
    }
    if admin_count == 0 && get_user_by_name(&pool, "admin").await?.is_some() {
        bail!("username admin is already assigned to a non-administrator");
    }

    seed_settings(&pool).await?;
    seed_admin(&pool).await?;
    migrate_overwrite_permission(&pool).await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS `x_users_single_admin` ON `x_users` (`role`) WHERE `role` = 2")
        .execute(&pool)
        .await?;

    let result = sqlx::query(
        "UPDATE `x_users`
         SET `base_path` = '/.users/' || `id`,
             `disabled` = 1
         WHERE `role` != ?
           AND `base_path` = '/'",
    )
    .bind(ROLE_ADMIN)
    .execute(&pool)
    .await?;

    if result.rows_affected() > 0 {
        tracing::warn!(
            users = result.rows_affected(),
            "disabled legacy non-admin users with unsafe root access"
        );
    }

    let admin_fix = sqlx::query(
        "UPDATE `x_users`
         SET `base_path` = '/'
         WHERE `role` = ?
           AND `base_path` LIKE '/.users/%'",
    )
    .bind(ROLE_ADMIN)
    .execute(&pool)
    .await?;

    if admin_fix.rows_affected() > 0 {
        tracing::info!(
            admins = admin_fix.rows_affected(),
            "restored base_path='/' for admin users"
        );
    }

    Ok(pool)
}

async fn migrate_overwrite_permission(pool: &DbPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let marker = sqlx::query(
        "INSERT OR IGNORE INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES ('permission_overwrite_v1_migrated', 'true', 'bool', 0, 1)",
    )
    .execute(&mut *tx)
    .await?;
    if marker.rows_affected() > 0 {
        sqlx::query(
            "UPDATE `x_users` SET `permission` = `permission` | (1 << 8) WHERE `role` != ? AND (`permission` & 120) != 0",
        )
        .bind(ROLE_ADMIN)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
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
        let salt = rand_string(16);
        let s_hash = static_hash(&initial_pwd);
        let encoded_pwd = encode_argon2_hash(&s_hash, &salt);
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        sqlx::query(
            r#"
            INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`, `password_unset`)
            VALUES ('admin', ?, ?, ?, '/', ?, 0, 0, 0)
            "#,
        )
        .bind(&encoded_pwd)
        .bind(now_ts)
        .bind(&salt)
        .bind(ROLE_ADMIN)
        .execute(pool)
        .await?;

        println!(
            "\n==================================================================\n\
             Initial admin user created:\n\
             Username: admin\n\
             Password: {}\n\
             Please save this password or change it in the Web UI.\n\
             You can also reset it anytime using: rulist admin random-password\n\
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
    if let Some(ref mut u) = user {
        u.update_otp();
    }
    Ok(user)
}

pub async fn set_user_password(
    pool: &DbPool,
    username: &str,
    new_password: &str,
    reset: bool,
) -> Result<()> {
    let user = get_user_by_name(pool, username)
        .await?
        .context("user not found")?;
    let salt = rand_string(16);
    let s_hash = static_hash(new_password);
    let encoded_pwd = encode_argon2_hash(&s_hash, &salt);
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE `x_users` SET `pwd_hash` = ?, `pwd_ts` = MAX(`pwd_ts` + 1, ?), `salt` = ?, `password_unset` = ?, `otp_secret` = CASE WHEN ? THEN '' ELSE `otp_secret` END, `last_otp_step` = CASE WHEN ? THEN -1 ELSE `last_otp_step` END WHERE `id` = ?")
        .bind(encoded_pwd)
        .bind(now_ts)
        .bind(salt)
        .bind(new_password.is_empty())
        .bind(reset)
        .bind(reset)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    if reset {
        sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        if user.is_admin() {
            sqlx::query("UPDATE `x_setting_items` SET `value` = ? WHERE `key` = 'token'")
                .bind(rand_token())
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn set_admin_password(pool: &DbPool, new_password: &str) -> Result<()> {
    set_user_password(pool, "admin", new_password, false).await
}

#[allow(dead_code)]
pub async fn reset_admin_password(pool: &DbPool, new_password: &str) -> Result<()> {
    set_user_password(pool, "admin", new_password, true).await
}

pub async fn get_setting(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let val: Option<String> =
        sqlx::query_scalar("SELECT `value` FROM `x_setting_items` WHERE `key` = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(val)
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
    if let Some(ref mut u) = user {
        u.update_otp();
    }
    Ok(user)
}

pub async fn get_all_users(pool: &DbPool) -> Result<Vec<User>> {
    let mut users = sqlx::query_as::<_, User>("SELECT * FROM `x_users` ORDER BY `id` ASC")
        .fetch_all(pool)
        .await?;
    for u in &mut users {
        u.update_otp();
    }
    Ok(users)
}

pub async fn get_user_by_id(pool: &DbPool, id: i64) -> Result<Option<User>> {
    let mut user = sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some(ref mut u) = user {
        u.update_otp();
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
    for s in storages {
        if s.driver == "Local"
            && (base_path == s.mount_path
                || base_path.starts_with(&format!("{}/", s.mount_path.trim_end_matches('/'))))
            && (matched.is_none() || s.mount_path.len() > matched.unwrap().mount_path.len())
        {
            matched = Some(s);
        }
    }

    if let Some(s) = matched
        && let Some(ref addition_str) = s.addition
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(addition_str)
    {
        let root = val
            .get("root_folder_path")
            .or_else(|| val.get("root_folder"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !root.is_empty() {
            let sub = base_path
                .trim_start_matches(&s.mount_path)
                .trim_start_matches('/');
            if sub.is_empty() {
                return root.to_string();
            } else {
                return format!("{}/{}", root.trim_end_matches('/'), sub);
            }
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

#[allow(dead_code)]
pub async fn get_all_settings(pool: &DbPool) -> Result<Vec<(String, String, String)>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT `key`, `value`, `type` FROM `x_setting_items` ORDER BY `group` ASC, `key` ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[allow(dead_code)]
pub async fn set_setting(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO `x_setting_items` (`key`, `value`, `type`, `group`, `flag`) VALUES (?, ?, 'string', 0, 0)
         ON CONFLICT(`key`) DO UPDATE SET `value` = excluded.`value`",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_user(pool: &DbPool, user_id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let user_mount = format!("/.users/{user_id}");
    sqlx::query("DELETE FROM `x_storages` WHERE `mount_path` = ?")
        .bind(&user_mount)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn cancel_user_2fa(pool: &DbPool, user_id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE `x_users` SET `otp_secret` = '', `last_otp_step` = -1 WHERE `id` = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_user_disabled(pool: &DbPool, user_id: i64, disabled: bool) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `disabled` = ? WHERE `id` = ?")
        .bind(if disabled { 1 } else { 0 })
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn set_user_role(pool: &DbPool, user_id: i64, role: i32) -> Result<()> {
    let base_path = if role == ROLE_ADMIN {
        "/".to_string()
    } else {
        format!("/.users/{user_id}")
    };
    sqlx::query("UPDATE `x_users` SET `role` = ?, `base_path` = ? WHERE `id` = ?")
        .bind(role)
        .bind(&base_path)
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
    let addition = serde_json::json!({
        "root_folder_path": local_path
    })
    .to_string();

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
            "INSERT INTO `x_storages` (`mount_path`, `order`, `driver`, `addition`, `status`, `disabled`) VALUES (?, 0, 'Local', ?, 'work', 0)",
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
    let salt = rand_string(16);
    let s_hash = static_hash(password);
    let encoded_pwd = encode_argon2_hash(&s_hash, &salt);
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES (?, ?, ?, ?, '/', ?, ?, ?, ?)"
    )
    .bind(username)
    .bind(&encoded_pwd)
    .bind(now_ts)
    .bind(&salt)
    .bind(role)
    .bind(if disabled { 1 } else { 0 })
    .bind(permission)
    .bind(0)
    .execute(&mut *tx)
    .await?;

    let new_id = res.last_insert_rowid();

    if role != ROLE_ADMIN {
        let user_mount = format!("/.users/{new_id}");
        sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
            .bind(&user_mount)
            .bind(new_id)
            .execute(&mut *tx)
            .await?;

        if let Some(local_path) = local_path {
            let addition = serde_json::json!({
                "root_folder_path": local_path
            })
            .to_string();

            sqlx::query(
                "INSERT INTO `x_storages` (`mount_path`, `order`, `driver`, `addition`, `status`, `disabled`) VALUES (?, 0, 'Local', ?, 'work', 0)",
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
