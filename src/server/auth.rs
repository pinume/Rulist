use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::auth::{generate_jwt, hash_identifier, matching_totp_step, parse_jwt, verify_password};
use crate::db::{SessionUser, get_user_by_name};
use crate::permissions::ALLOW_EMPTY_PASSWORD;
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

async fn login_failure_response(
    state: &SharedState,
    attempt_key: &str,
    already_limited: bool,
    status: StatusCode,
    code: i32,
    message: &'static str,
) -> Response {
    if already_limited {
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            429,
            "Too many login attempts",
        );
    }

    match crate::db::record_login_failure(
        &state.pool,
        attempt_key,
        LOGIN_FAILURE_WINDOW_SECS,
        LOGIN_ATTEMPT_CAP,
    )
    .await
    {
        Ok(Some(count)) if count > LOGIN_FAILURE_LIMIT => api_error(
            StatusCode::TOO_MANY_REQUESTS,
            429,
            "Too many login attempts",
        ),
        Ok(Some(_)) => api_error(status, code, message),
        Ok(None) => {
            tracing::warn!("login failure store at capacity; failure was not recorded");
            api_error(status, code, message)
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to record login failure");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn login_handler(
    State(state): State<SharedState>,
    Json(req): Json<LoginReq>,
) -> Response {
    let attempt_key = login_attempt_key(&req.username);
    let already_limited = match crate::db::check_login_limit(
        &state.pool,
        &attempt_key,
        LOGIN_FAILURE_WINDOW_SECS,
        LOGIN_FAILURE_LIMIT,
    )
    .await
    {
        Ok(already_limited) => already_limited,
        Err(err) => {
            tracing::error!(error = %err, "failed to check login limit");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    // The counter is attacker-controlled; it may shape failed responses but must not skip valid credential checks.
    let user = match get_user_by_name(&state.pool, &req.username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            if let Err(status) = verify_password_bounded(&req.password, DUMMY_PASSWORD_HASH).await {
                return password_verify_error(status);
            }
            tracing::warn!(username = %req.username, "login failed: user not found");
            return login_failure_response(
                &state,
                &attempt_key,
                already_limited,
                StatusCode::UNAUTHORIZED,
                401,
                "invalid username or password",
            )
            .await;
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
        return login_failure_response(
            &state,
            &attempt_key,
            already_limited,
            StatusCode::UNAUTHORIZED,
            401,
            "invalid username or password",
        )
        .await;
    }

    if !user.is_admin()
        && req.password.is_empty()
        && user.permission & (1 << ALLOW_EMPTY_PASSWORD) == 0
    {
        tracing::warn!(username = %user.username, "login failed: empty password is not permitted");
        return login_failure_response(
            &state,
            &attempt_key,
            already_limited,
            StatusCode::UNAUTHORIZED,
            401,
            "invalid username or password",
        )
        .await;
    }

    match verify_password_bounded(&req.password, &user.pwd_hash).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(username = %user.username, "login failed: invalid password");
            return login_failure_response(
                &state,
                &attempt_key,
                already_limited,
                StatusCode::UNAUTHORIZED,
                401,
                "invalid username or password",
            )
            .await;
        }
        Err(status) => return password_verify_error(status),
    }

    if let Some(secret) = user.otp_secret.as_deref() {
        if !secret.trim().is_empty() {
            let otp_code = req.otp_code.as_deref().unwrap_or("").trim();
            if otp_code.is_empty() {
                return login_failure_response(
                    &state,
                    &attempt_key,
                    already_limited,
                    StatusCode::UNAUTHORIZED,
                    402,
                    "OTP code is required",
                )
                .await;
            }
            let Some(step) = matching_totp_step(secret, otp_code) else {
                return login_failure_response(
                    &state,
                    &attempt_key,
                    already_limited,
                    StatusCode::UNAUTHORIZED,
                    400,
                    "invalid otp code",
                )
                .await;
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
                return login_failure_response(
                    &state,
                    &attempt_key,
                    already_limited,
                    StatusCode::UNAUTHORIZED,
                    400,
                    "invalid otp code",
                )
                .await;
            }
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
