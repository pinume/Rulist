use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::auth::{
    generate_jwt, hash_identifier, hash_password, matching_totp_step, parse_jwt, verify_password,
};
use crate::db::{get_setting, get_user_by_name};
use crate::model::{LoginReq, PERM_ALLOW_EMPTY_PASSWORD, UpdateCurrentReq};
use crate::server::{SharedState, api_error, api_success, authenticate_user};

const LOGIN_FAILURE_LIMIT: i64 = 5;
const LOGIN_FAILURE_WINDOW_SECS: i64 = 15 * 60;
const LOGIN_ATTEMPT_CAP: i64 = 10_000;
const PASSWORD_VERIFY_CONCURRENCY: usize = 4;
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXlzYWx0MTIzNDU2$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
static PASSWORD_VERIFY_SEMAPHORE: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(PASSWORD_VERIFY_CONCURRENCY);

fn login_attempt_key(username: &str) -> String {
    hash_identifier(username.trim())
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn reserve_login_attempt(
    pool: &crate::db::DbPool,
    key: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let cutoff = now_ts() - LOGIN_FAILURE_WINDOW_SECS;
    sqlx::query("DELETE FROM `x_login_attempts` WHERE `window_started` < ?")
        .bind(cutoff)
        .execute(pool)
        .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_login_attempts`")
        .fetch_one(pool)
        .await?;
    if total >= LOGIN_ATTEMPT_CAP {
        sqlx::query(
            "DELETE FROM x_login_attempts WHERE username_hash IN (SELECT username_hash FROM x_login_attempts ORDER BY window_started ASC LIMIT 100)",
        )
        .execute(pool)
        .await?;
    }

    let now = now_ts();
    sqlx::query_scalar(
        "INSERT INTO `x_login_attempts` (`username_hash`, `failed_count`, `window_started`) \
         SELECT ?, 1, ? WHERE EXISTS (SELECT 1 FROM `x_login_attempts` WHERE `username_hash` = ?) \
             OR (SELECT COUNT(*) FROM `x_login_attempts`) < ? \
         ON CONFLICT(`username_hash`) DO UPDATE SET `failed_count` = `failed_count` + 1 \
         RETURNING `failed_count`",
    )
    .bind(key)
    .bind(now)
    .bind(key)
    .bind(LOGIN_ATTEMPT_CAP)
    .fetch_optional(pool)
    .await
}

async fn verify_password_bounded(password: &str, pwd_hash: &str) -> Result<bool, StatusCode> {
    let _permit = PASSWORD_VERIFY_SEMAPHORE
        .try_acquire()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let password = password.to_owned();
    let pwd_hash = pwd_hash.to_owned();
    tokio::task::spawn_blocking(move || verify_password(&password, &pwd_hash))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn password_verify_error(status: StatusCode) -> Response {
    if status == StatusCode::TOO_MANY_REQUESTS {
        api_error(status, 429, "Too many password verification attempts")
    } else {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        )
    }
}

pub async fn login_handler(
    State(state): State<SharedState>,
    Json(req): Json<LoginReq>,
) -> Response {
    let attempt_key = login_attempt_key(&req.username);
    match reserve_login_attempt(&state.pool, &attempt_key).await {
        Ok(Some(count)) if count > LOGIN_FAILURE_LIMIT => {
            return api_error(
                StatusCode::TOO_MANY_REQUESTS,
                429,
                "Too many login attempts",
            );
        }
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::warn!(
                username = %req.username,
                "rate limit store at capacity, proceeding with bounded credential verification"
            );
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to reserve login attempt");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    }

    let user = match get_user_by_name(&state.pool, &req.username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            if let Err(status) = verify_password_bounded(&req.password, DUMMY_PASSWORD_HASH).await {
                return password_verify_error(status);
            }
            tracing::warn!(username = %req.username, "login failed: user not found");
            return api_error(
                StatusCode::UNAUTHORIZED,
                401,
                "invalid username or password",
            );
        }
        Err(err) => {
            tracing::error!(error = %err, "login database error");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    if user.disabled {
        tracing::warn!(username = %user.username, "login failed: user is disabled");
        return api_error(
            StatusCode::UNAUTHORIZED,
            401,
            "invalid username or password",
        );
    }

    if !user.is_admin()
        && req.password.is_empty()
        && user.permission & (1 << PERM_ALLOW_EMPTY_PASSWORD) == 0
    {
        tracing::warn!(username = %user.username, "login failed: empty password is not permitted");
        return api_error(
            StatusCode::UNAUTHORIZED,
            401,
            "invalid username or password",
        );
    }

    match verify_password_bounded(&req.password, &user.pwd_hash).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(username = %user.username, "login failed: invalid password");
            return api_error(
                StatusCode::UNAUTHORIZED,
                401,
                "invalid username or password",
            );
        }
        Err(status) => return password_verify_error(status),
    }

    if let Some(ref secret) = user.otp_secret
        && !secret.trim().is_empty()
    {
        let otp_code = req.otp_code.as_deref().unwrap_or("").trim();
        if otp_code.is_empty() {
            return api_error(StatusCode::UNAUTHORIZED, 402, "OTP code is required");
        }
        let Some(step) = matching_totp_step(secret, otp_code) else {
            return api_error(StatusCode::UNAUTHORIZED, 400, "invalid otp code");
        };
        let accepted = match sqlx::query(
            "UPDATE `x_users` SET `last_otp_step` = ? WHERE `id` = ? AND `last_otp_step` < ?",
        )
        .bind(step)
        .bind(user.id)
        .bind(step)
        .execute(&state.pool)
        .await
        {
            Ok(result) => result.rows_affected() == 1,
            Err(err) => {
                tracing::error!(error = %err, "failed to record accepted otp step");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        };
        if !accepted {
            return api_error(StatusCode::UNAUTHORIZED, 400, "invalid otp code");
        }
    }

    match generate_jwt(
        user.id,
        &user.username,
        user.pwd_ts,
        &state.config.jwt_secret,
        state.config.token_expires_in,
    ) {
        Ok(token) => {
            if let Err(err) =
                sqlx::query("DELETE FROM `x_login_attempts` WHERE `username_hash` = ?")
                    .bind(&attempt_key)
                    .execute(&state.pool)
                    .await
            {
                tracing::error!(error = %err, "failed to clear login failures");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
            api_success(serde_json::json!({ "token": token }))
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to generate jwt");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn logout_handler(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let Some(auth_header) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return api_success(());
    };
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    match get_setting(&state.pool, "token").await {
        Ok(Some(master))
            if !master.is_empty()
                && subtle::ConstantTimeEq::ct_eq(master.as_bytes(), token.as_bytes()).into() =>
        {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Master token cannot be revoked",
            );
        }
        Ok(_) => {}
        Err(err) => {
            tracing::error!(error = %err, "failed to load master token during logout");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    }

    if let Ok(claims) = parse_jwt(token, &state.config.jwt_secret) {
        let now = now_ts();
        if let Err(err) = sqlx::query("DELETE FROM `x_revoked_tokens` WHERE `expires_at` < ?")
            .bind(now)
            .execute(&state.pool)
            .await
        {
            tracing::error!(error = %err, "failed to prune revoked tokens");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
        if let Err(err) = sqlx::query(
            "INSERT OR REPLACE INTO `x_revoked_tokens` (`jti`, `expires_at`) VALUES (?, ?)",
        )
        .bind(claims.jti)
        .bind(claims.exp as i64)
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %err, "failed to revoke jwt");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    }

    api_success(())
}

pub async fn current_user_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Response {
    if let Some(user) = authenticate_user(&headers, &state).await {
        api_success(user)
    } else {
        api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required")
    }
}

pub async fn update_current_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<UpdateCurrentReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(user) => user,
        None => return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required"),
    };

    let username_changed = req
        .username
        .as_deref()
        .map(|name| !name.trim().is_empty() && name.trim() != user.username)
        .unwrap_or(false);
    let password_changed = req.password.is_some()
        && (!user.is_admin() || req.password.as_deref().is_some_and(|pwd| !pwd.is_empty()));

    if user.is_admin() && username_changed {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "admin username cannot be changed",
        );
    }
    if user.password_unset && !password_changed {
        return api_error(StatusCode::BAD_REQUEST, 400, "set a new password first");
    }

    if (username_changed || password_changed) && !user.password_unset {
        let Some(current_password) = req.current_password.as_deref() else {
            return api_error(StatusCode::BAD_REQUEST, 400, "Current password is required");
        };
        match verify_password_bounded(current_password, &user.pwd_hash).await {
            Ok(true) => {}
            Ok(false) => {
                return api_error(StatusCode::FORBIDDEN, 403, "Current password is incorrect");
            }
            Err(status) => return password_verify_error(status),
        }
    }

    if let Some(new_password) = &req.password {
        if new_password.is_empty() {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Password cannot be empty in personal profile",
            );
        }
        if !crate::auth::valid_password(new_password) {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Password length must be between 8 and 128 characters",
            );
        }
    }

    let clean_name = req.username.as_deref().map(str::trim).unwrap_or("");
    if username_changed {
        if clean_name.len() > 64 {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Username length cannot exceed 64 characters",
            );
        }
        let exists: Result<Option<i64>, _> =
            sqlx::query_scalar("SELECT `id` FROM `x_users` WHERE `username` = ? AND `id` != ?")
                .bind(clean_name)
                .bind(user.id)
                .fetch_optional(&state.pool)
                .await;
        match exists {
            Ok(Some(_)) => {
                return api_error(StatusCode::CONFLICT, 409, "Username already exists");
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!(error = %err, "failed to check existing username");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        }
    }

    if !username_changed && !password_changed {
        return api_success(());
    }

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(error = %err, "failed to start profile update transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    if let Some(new_password) = &req.password
        && password_changed
    {
        let encoded_pwd = hash_password(new_password);
        let now = now_ts();
        let update = sqlx::query(
            "UPDATE `x_users` SET `pwd_hash` = ?, `pwd_ts` = MAX(`pwd_ts` + 1, ?), `password_unset` = 0 WHERE `id` = ? AND `pwd_ts` = ?",
        )
        .bind(&encoded_pwd)
        .bind(now)
        .bind(user.id)
        .bind(user.pwd_ts)
        .execute(&mut *tx)
        .await;

        match update {
            Ok(result) if result.rows_affected() == 0 => {
                let _ = tx.rollback().await;
                return api_error(
                    StatusCode::CONFLICT,
                    409,
                    "Password has been changed concurrently, please log in again",
                );
            }
            Ok(_) => {}
            Err(err) => {
                tracing::error!(error = %err, "failed to update user password in transaction");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        }
    }

    if username_changed {
        if let Err(err) = sqlx::query("UPDATE `x_users` SET `username` = ? WHERE `id` = ?")
            .bind(clean_name)
            .bind(user.id)
            .execute(&mut *tx)
            .await
        {
            tracing::error!(error = %err, "failed to update username in transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    }

    if let Err(err) = tx.commit().await {
        tracing::error!(error = %err, "failed to commit profile update transaction");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    api_success(())
}
