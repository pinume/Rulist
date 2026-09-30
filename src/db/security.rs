use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::Error;

use super::DbPool;

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub async fn record_login_failure(
    pool: &DbPool,
    key: &str,
    window_secs: i64,
    cap: i64,
) -> Result<Option<i64>, Error> {
    delete_expired_login_attempts(pool, window_secs).await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `login_attempts`")
        .fetch_one(pool)
        .await?;
    if total >= cap {
        sqlx::query("DELETE FROM login_attempts WHERE username_hash IN (SELECT username_hash FROM login_attempts ORDER BY window_started ASC LIMIT 100)").execute(pool).await?;
    }
    sqlx::query_scalar("INSERT INTO `login_attempts` (`username_hash`, `failed_count`, `window_started`) SELECT ?, 1, ? WHERE EXISTS (SELECT 1 FROM `login_attempts` WHERE `username_hash` = ?) OR (SELECT COUNT(*) FROM `login_attempts`) < ? ON CONFLICT(`username_hash`) DO UPDATE SET `failed_count` = `failed_count` + 1 RETURNING `failed_count`")
        .bind(key).bind(now_ts()).bind(key).bind(cap).fetch_optional(pool).await
}

pub async fn check_login_limit(
    pool: &DbPool,
    key: &str,
    window_secs: i64,
    limit: i64,
) -> Result<bool, Error> {
    delete_expired_login_attempts(pool, window_secs).await?;
    let count: Option<i64> =
        sqlx::query_scalar("SELECT `failed_count` FROM `login_attempts` WHERE `username_hash` = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(count.is_some_and(|count| count > limit))
}

async fn delete_expired_login_attempts(pool: &DbPool, window_secs: i64) -> Result<(), Error> {
    sqlx::query("DELETE FROM `login_attempts` WHERE `window_started` < ?")
        .bind(now_ts() - window_secs)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn clear_login_attempt(pool: &DbPool, key: &str) -> Result<(), Error> {
    sqlx::query("DELETE FROM `login_attempts` WHERE `username_hash` = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn revoke_token(pool: &DbPool, jti: &str, expires_at: i64) -> Result<(), Error> {
    cleanup_revoked_tokens(pool).await?;
    sqlx::query("INSERT OR REPLACE INTO `revoked_tokens` (`jti`, `expires_at`) VALUES (?, ?)")
        .bind(jti)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn cleanup_revoked_tokens(pool: &DbPool) -> Result<(), Error> {
    sqlx::query("DELETE FROM `revoked_tokens` WHERE `expires_at` < ?")
        .bind(now_ts())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn is_token_revoked(pool: &DbPool, jti: &str) -> Result<bool, Error> {
    sqlx::query_scalar("SELECT 1 FROM `revoked_tokens` WHERE `jti` = ? AND `expires_at` >= ?")
        .bind(jti)
        .bind(now_ts())
        .fetch_optional(pool)
        .await
        .map(|token: Option<i64>| token.is_some())
}
