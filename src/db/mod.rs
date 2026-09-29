use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;

pub mod security;
pub use security::*;
pub mod users;
pub use users::*;

pub type DbPool = Pool<Sqlite>;

const CURRENT_SCHEMA_VERSION: i64 = 1;

async fn validate_unversioned_schema(pool: &DbPool) -> Result<()> {
    let has_tables: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
    )
    .fetch_one(pool)
    .await?;
    if !has_tables {
        return Ok(());
    }

    for (table, required_columns) in [
        (
            "users",
            &[
                "id",
                "username",
                "pwd_hash",
                "pwd_ts",
                "local_path",
                "role",
                "disabled",
                "permission",
                "password_unset",
                "otp_secret",
                "last_otp_step",
            ][..],
        ),
        (
            "login_attempts",
            &["username_hash", "failed_count", "window_started"],
        ),
        ("revoked_tokens", &["jti", "expires_at"]),
    ] {
        let columns = sqlx::query(&format!("PRAGMA table_info(`{table}`)"))
            .fetch_all(pool)
            .await?
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();
        if required_columns
            .iter()
            .any(|required| !columns.iter().any(|column| column == required))
        {
            anyhow::bail!("existing unversioned database has an incomplete `{table}` table");
        }
    }
    Ok(())
}

async fn init_schema(pool: &DbPool) -> Result<()> {
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await?;
    if version < 0 {
        anyhow::bail!("database schema version must not be negative");
    }
    if version > CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
        );
    }
    if version == 0 {
        validate_unversioned_schema(pool).await?;
    }

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
    if version < CURRENT_SCHEMA_VERSION {
        sqlx::query(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION}"))
            .execute(pool)
            .await?;
    }
    Ok(())
}

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

    init_schema(&pool)
        .await
        .context("failed to initialize SQLite schema")?;

    cleanup_revoked_tokens(&pool).await?;
    validate_admin_invariants(&pool).await?;
    seed_admin(&pool).await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn initializes_new_database_schema_version() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("data.db");
        let pool = init_db(&db_path).await.unwrap();

        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        pool.close().await;
    }

    #[tokio::test]
    async fn recognizes_current_unversioned_schema_and_preserves_data() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("data.db");
        let pool = init_db(&db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO users (username, pwd_hash, pwd_ts, local_path, role, disabled, permission, password_unset) VALUES ('legacy-user', '', 0, '/', 0, 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("PRAGMA user_version = 0")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let pool = init_db(&db_path).await.unwrap();
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'legacy-user'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(count, 1);
        pool.close().await;
    }

    #[tokio::test]
    async fn rejects_incomplete_unversioned_schema_without_repairing_it() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("data.db");
        let connection = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(connection)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let error = init_db(&db_path).await.unwrap_err();
        assert!(format!("{error:#}").contains("incomplete `users` table"));

        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display())).unwrap(),
            )
            .await
            .unwrap();
        let columns = sqlx::query("PRAGMA table_info(users)")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();
        assert_eq!(columns, ["id"]);
        pool.close().await;
    }

    #[tokio::test]
    async fn rejects_newer_database_schema_version() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("data.db");
        let pool = init_db(&db_path).await.unwrap();

        sqlx::query("PRAGMA user_version = 2")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let error = init_db(&db_path).await.unwrap_err();
        assert!(format!("{error:#}").contains("newer than supported version"));
    }
}
