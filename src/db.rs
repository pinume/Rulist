use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

use crate::auth::{hash_password, rand_string};
use crate::model::{ROLE_ADMIN, User};

pub type DbPool = Pool<Sqlite>;

async fn init_schema(pool: &DbPool) -> Result<()> {
    for statement in [
        r#"
        CREATE TABLE IF NOT EXISTS `x_users` (
            `id` INTEGER PRIMARY KEY AUTOINCREMENT,
            `username` TEXT NOT NULL UNIQUE,
            `pwd_hash` TEXT NOT NULL,
            `pwd_ts` INTEGER NOT NULL,
            `local_path` TEXT NOT NULL,
            `role` INTEGER NOT NULL DEFAULT 0,
            `disabled` NUMERIC NOT NULL DEFAULT 0,
            `permission` INTEGER NOT NULL DEFAULT 0,
            `password_unset` NUMERIC NOT NULL DEFAULT 0
                CHECK (
                    `password_unset` = 0
                    OR (
                        `role` != 2
                        AND (`permission` & 512) != 0
                    )
                ),
            `otp_secret` TEXT,
            `last_otp_step` INTEGER NOT NULL DEFAULT -1
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS `x_login_attempts` (
            `username_hash` TEXT PRIMARY KEY,
            `failed_count` INTEGER NOT NULL,
            `window_started` INTEGER NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS `x_revoked_tokens` (
            `jti` TEXT PRIMARY KEY,
            `expires_at` INTEGER NOT NULL
        )
        "#,
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS `x_users_single_admin`
            ON `x_users` (`role`) WHERE `role` = 2
        "#,
    ] {
        sqlx::query(statement).execute(pool).await?;
    }
    Ok(())
}

pub async fn init_db(db_path: &Path, home_path: &Path) -> Result<DbPool> {
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

    init_schema(&pool)
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

    validate_admin_invariants(&pool).await?;
    seed_admin(&pool, home_path).await?;

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

async fn seed_admin(pool: &DbPool, home_path: &Path) -> Result<()> {
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
            INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `local_path`, `role`, `disabled`, `permission`, `password_unset`)
            VALUES ('admin', ?, ?, ?, ?, 0, 0, 0)
            "#,
        )
        .bind(&encoded_pwd)
        .bind(now_ts)
        .bind(home_path.to_string_lossy().into_owned())
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

pub async fn delete_user(pool: &DbPool, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
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

pub async fn set_user_permission(pool: &DbPool, user_id: i64, permission: i32) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `permission` = ? WHERE `id` = ?")
        .bind(permission)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_user_dir(pool: &DbPool, user_id: i64, local_path: &str) -> Result<()> {
    sqlx::query("UPDATE `x_users` SET `local_path` = ? WHERE `id` = ?")
        .bind(local_path)
        .bind(user_id)
        .execute(pool)
        .await?;
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
        "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `local_path`, `role`, `disabled`, `permission`, `password_unset`) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(username)
    .bind(&encoded_pwd)
    .bind(now_ts)
    .bind(local_path.context("local_path is required")?)
    .bind(role)
    .bind(if disabled { 1 } else { 0 })
    .bind(permission)
    .bind(password.is_empty())
    .execute(&mut *tx)
    .await?;

    let new_id = result.last_insert_rowid();

    tx.commit().await?;
    Ok(new_id)
}
