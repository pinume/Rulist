use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::auth::{generate_jwt, hash_identifier, matching_totp_step, parse_jwt, verify_password};
use crate::db::PERM_ALLOW_EMPTY_PASSWORD;
use crate::db::{SessionUser, get_user_by_name};
use crate::server::{SharedState, api_error, api_success, authenticate_user};

const LOGIN_FAILURE_LIMIT: i64 = 5;
const LOGIN_FAILURE_WINDOW_SECS: i64 = 15 * 60;
const LOGIN_ATTEMPT_CAP: i64 = 10_000;
const PASSWORD_VERIFY_CONCURRENCY: usize = 4;
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXlzYWx0MTIzNDU2$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
static PASSWORD_VERIFY_SEMAPHORE: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(PASSWORD_VERIFY_CONCURRENCY);

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub otp_code: Option<String>,
}

fn login_attempt_key(username: &str) -> String {
    hash_identifier(username.trim())
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
    match crate::db::reserve_login_attempt(
        &state.pool,
        &attempt_key,
        LOGIN_FAILURE_WINDOW_SECS,
        LOGIN_ATTEMPT_CAP,
    )
    .await
    {
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
        let accepted = match crate::db::accept_otp_step(&state.pool, user.id, step).await {
            Ok(accepted) => accepted,
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
            if let Err(err) = crate::db::clear_login_attempt(&state.pool, &attempt_key).await {
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

    if let Ok(claims) = parse_jwt(token, &state.config.jwt_secret) {
        if let Err(err) = crate::db::revoke_token(&state.pool, &claims.jti, claims.exp as i64).await
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
        api_success(SessionUser::from(&user))
    } else {
        api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required")
    }
}
