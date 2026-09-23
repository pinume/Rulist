use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::auth::{
    generate_jwt, generate_otp_secret, generate_totp_qr, verify_password, verify_totp,
};
use crate::db::{get_setting, get_user_by_name};
use crate::model::{LoginReq, TwoFaGenerateReq, TwoFaVerifyReq, UpdateCurrentReq};
use crate::server::{SharedState, api_error, api_success, authenticate_user};

pub async fn login_handler(
    State(state): State<SharedState>,
    Json(req): Json<LoginReq>,
) -> Response {
    let user = match get_user_by_name(&state.pool, &req.username).await {
        Ok(Some(u)) => u,
        Ok(None) => {
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

    if !verify_password(&req.password, &user.pwd_hash, &user.salt) {
        tracing::warn!(username = %user.username, "login failed: invalid password");
        return api_error(
            StatusCode::UNAUTHORIZED,
            401,
            "invalid username or password",
        );
    }

    // Check 2FA if enabled
    if let Some(ref secret) = user.otp_secret
        && !secret.trim().is_empty()
    {
        let otp_code = req.otp_code.as_deref().unwrap_or("").trim();
        if otp_code.is_empty() {
            return api_error(StatusCode::UNAUTHORIZED, 402, "OTP code is required");
        }
        if !verify_totp(secret, otp_code) {
            return api_error(StatusCode::UNAUTHORIZED, 400, "invalid otp code");
        }
    }

    match generate_jwt(
        &user.username,
        user.pwd_ts,
        &state.config.jwt_secret,
        state.config.token_expires_in,
    ) {
        Ok(token) => api_success(serde_json::json!({ "token": token })),
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

pub async fn logout_handler() -> Response {
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
        Some(u) => u,
        None => return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required"),
    };

    let username_changed = req
        .username
        .as_deref()
        .map(|name| !name.trim().is_empty() && name.trim() != user.username)
        .unwrap_or(false);
    let password_changed = req
        .password
        .as_deref()
        .map(|pwd| !pwd.is_empty())
        .unwrap_or(false);

    if username_changed || password_changed {
        let current_password = req.current_password.as_deref().unwrap_or("");
        if current_password.is_empty() {
            return api_error(StatusCode::BAD_REQUEST, 400, "Current password is required");
        }
        if !verify_password(current_password, &user.pwd_hash, &user.salt) {
            return api_error(StatusCode::FORBIDDEN, 403, "Current password is incorrect");
        }
    }

    if let Some(new_pwd) = &req.password
        && !new_pwd.is_empty()
        && !crate::auth::valid_password(new_pwd)
    {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Password length must be between 8 and 128 characters",
        );
    }

    let clean_name = req.username.as_deref().map(|n| n.trim()).unwrap_or("");
    if username_changed {
        if clean_name.len() > 64 {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Username length cannot exceed 64 characters",
            );
        }

        let exists_res: Result<Option<i64>, _> =
            sqlx::query_scalar("SELECT `id` FROM `x_users` WHERE `username` = ? AND `id` != ?")
                .bind(clean_name)
                .bind(user.id)
                .fetch_optional(&state.pool)
                .await;

        match exists_res {
            Ok(Some(_)) => {
                return api_error(StatusCode::CONFLICT, 409, "Username already exists");
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to check existing username");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
            Ok(None) => {}
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

    if let Some(new_pwd) = &req.password
        && !new_pwd.is_empty()
    {
        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash(new_pwd);
        let encoded_pwd = crate::auth::encode_argon2_hash(&s_hash, &salt);
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = sqlx::query(
            "UPDATE `x_users` SET `pwd_hash` = ?, `salt` = ?, `pwd_ts` = MAX(`pwd_ts` + 1, ?) WHERE `id` = ?",
        )
        .bind(&encoded_pwd)
        .bind(&salt)
        .bind(now_ts)
        .bind(user.id)
        .execute(&mut *tx)
        .await
        {
            tracing::error!(error = %e, "failed to update user password in transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    }

    if username_changed {
        match sqlx::query("UPDATE `x_users` SET `username` = ? WHERE `id` = ?")
            .bind(clean_name)
            .bind(user.id)
            .execute(&mut *tx)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                tracing::error!(error = %e, "failed to update username in transaction");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
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

pub async fn two_factor_generate_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<TwoFaGenerateReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required"),
    };

    let current_password = req.current_password.trim();
    if current_password.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Current password is required");
    }

    if !verify_password(current_password, &user.pwd_hash, &user.salt) {
        return api_error(StatusCode::FORBIDDEN, 403, "Current password is incorrect");
    }

    if user.otp {
        return api_error(StatusCode::BAD_REQUEST, 400, "2FA is already enabled");
    }

    let site_title = get_setting(&state.pool, "site_title")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "Rulist".to_string());

    let secret = generate_otp_secret();
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        + 600;

    if let Err(err) = sqlx::query(
        "INSERT OR REPLACE INTO `x_otp_pending` (`user_id`, `secret`, `expires_at`) VALUES (?, ?, ?)",
    )
    .bind(user.id)
    .bind(&secret)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    {
        tracing::error!(error = %err, "failed to insert pending 2fa session");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    let qr = match generate_totp_qr(&site_title, &user.username, &secret) {
        Ok(data_uri) => data_uri,
        Err(err) => {
            tracing::error!(error = %err, "failed to generate totp qr");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    api_success(serde_json::json!({
        "qr": qr,
        "secret": secret,
    }))
}

pub async fn two_factor_verify_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<TwoFaVerifyReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required"),
    };

    let clean_code = req.code.trim();
    if clean_code.is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Verification code is required",
        );
    }

    let pending: Option<(String, i64)> = match sqlx::query_as(
        "SELECT `secret`, `expires_at` FROM `x_otp_pending` WHERE `user_id` = ?",
    )
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(p) => p,
        Err(err) => {
            tracing::error!(error = %err, "failed to query pending 2fa session");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    let Some((secret, expires_at)) = pending else {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "No pending 2FA session. Please generate a new secret.",
        );
    };

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if now_ts > expires_at {
        let _ = sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
            .bind(user.id)
            .execute(&state.pool)
            .await;
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "2FA setup session expired. Please generate a new secret.",
        );
    }

    if !verify_totp(&secret, clean_code) {
        return api_error(StatusCode::BAD_REQUEST, 400, "Invalid verification code");
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(err) => {
            tracing::error!(error = %err, "failed to begin 2fa verify transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    if let Err(err) = sqlx::query("UPDATE `x_users` SET `otp_secret` = ? WHERE `id` = ?")
        .bind(&secret)
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(error = %err, "failed to update user otp secret");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(error = %err, "failed to delete pending 2fa session");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = tx.commit().await {
        tracing::error!(error = %err, "failed to commit 2fa verify transaction");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    api_success(())
}
