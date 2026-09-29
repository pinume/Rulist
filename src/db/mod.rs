use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;

pub mod security;
pub use security::*;
pub mod users;
pub use users::*;

pub type DbPool = Pool<Sqlite>;

async fn init_schema(pool: &DbPool) -> Result<()> {
    for statement in [
        r#"
        CREATE TABLE IF NOT EXISTS `users` (
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
        CREATE TABLE IF NOT EXISTS `login_attempts` (
            `username_hash` TEXT PRIMARY KEY,
            `failed_count` INTEGER NOT NULL,
            `window_started` INTEGER NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS `revoked_tokens` (
            `jti` TEXT PRIMARY KEY,
            `expires_at` INTEGER NOT NULL
        )
        "#,
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS `users_single_admin`
            ON `users` (`role`) WHERE `role` = 2
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

    cleanup_revoked_tokens(&pool).await?;
    validate_admin_invariants(&pool).await?;
    seed_admin(&pool, home_path).await?;

    Ok(pool)
}
