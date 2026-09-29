use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use std::path::Path;

use crate::db::{create_user_direct, delete_user, get_all_users, get_user_by_id};
use crate::model::{AdminUserSaveReq, ROLE_ADMIN, User};
use crate::server::{SharedState, api_error, api_success, authenticate_user};

#[derive(Debug, Deserialize)]
pub struct IdQuery {
    pub id: Option<i64>,
}

async fn require_admin(
    headers: &HeaderMap,
    state: &crate::server::AppState,
) -> Result<User, Box<Response>> {
    let Some(user) = authenticate_user(headers, state).await else {
        return Err(Box::new(api_error(
            StatusCode::UNAUTHORIZED,
            401,
            "Authentication required",
        )));
    };
    if !user.is_admin() {
        return Err(Box::new(api_error(
            StatusCode::FORBIDDEN,
            403,
            "Permission denied",
        )));
    }
    Ok(user)
}

fn local_path(path: &str) -> anyhow::Result<String> {
    let path = path.trim();
    if path.is_empty() || !Path::new(path).is_absolute() {
        anyhow::bail!("local path must be absolute");
    }
    let path = Path::new(path).canonicalize()?;
    if !path.is_dir() {
        anyhow::bail!("local path must be a directory");
    }
    Ok(path.to_string_lossy().into_owned())
}

fn validate_password_request(password: &str, allow_empty: bool) -> Result<(), Response> {
    if password.is_empty() && !allow_empty {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Password cannot be empty unless passwordless login is enabled",
        ));
    }
    if !password.is_empty() && !crate::auth::valid_password(password) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Password length must be between 8 and 128 characters",
        ));
    }
    Ok(())
}

pub async fn admin_user_list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) = require_admin(&headers, &state).await {
        return *response;
    }
    match get_all_users(&state.pool).await {
        Ok(content) => {
            api_success(serde_json::json!({ "total": content.len(), "content": content }))
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to get all users");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn admin_user_get_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) = require_admin(&headers, &state).await {
        return *response;
    }
    let Some(id) = query.id else {
        return api_error(StatusCode::BAD_REQUEST, 400, "missing id");
    };
    match get_user_by_id(&state.pool, id).await {
        Ok(Some(user)) => api_success(user),
        Ok(None) => api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(err) => {
            tracing::error!(error = %err, id, "failed to get user");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn admin_user_create_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    if let Err(response) = require_admin(&headers, &state).await {
        return *response;
    }
    let username = req.username.trim();
    if username.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Username cannot be empty");
    }
    if req.role.unwrap_or(0) == ROLE_ADMIN {
        return api_error(StatusCode::BAD_REQUEST, 400, "admin user cannot be created");
    }
    let permission = req.permission.unwrap_or(0);
    let password = req.password.as_deref().unwrap_or("");
    if let Err(response) = validate_password_request(
        password,
        permission & (1 << crate::model::PERM_ALLOW_EMPTY_PASSWORD) != 0,
    ) {
        return response;
    }
    let Some(raw_path) = req.local_path.as_deref() else {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Local directory is required for non-admin users",
        );
    };
    let path = match local_path(raw_path) {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!(error = %err, "invalid user local path");
            return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
        }
    };
    match create_user_direct(
        &state.pool,
        username,
        password,
        0,
        Some(&path),
        permission,
        req.disabled.unwrap_or(false),
    )
    .await
    {
        Ok(_) => api_success(()),
        Err(err) => {
            tracing::warn!(error = %err, "failed to create user");
            if err.to_string().to_lowercase().contains("unique") {
                api_error(StatusCode::CONFLICT, 409, "Username already exists")
            } else {
                api_error(StatusCode::BAD_REQUEST, 400, "Failed to create user")
            }
        }
    }
}

pub async fn admin_user_update_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    if let Err(response) = require_admin(&headers, &state).await {
        return *response;
    }
    let Some(id) = req.id else {
        return api_error(StatusCode::BAD_REQUEST, 400, "missing id");
    };
    let mut target = match get_user_by_id(&state.pool, id).await {
        Ok(Some(user)) => user,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(_) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };
    let username = req.username.trim();
    if username.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Username cannot be empty");
    }
    if target.is_admin() && (username != "admin" || req.disabled == Some(true)) {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "admin username cannot be changed or disabled",
        );
    }
    let permission = req.permission.unwrap_or(target.permission);
    let allow_empty =
        !target.is_admin() && permission & (1 << crate::model::PERM_ALLOW_EMPTY_PASSWORD) != 0;
    if !target.is_admin()
        && target.password_unset
        && !allow_empty
        && req.password.as_deref().is_none_or(str::is_empty)
    {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Set a non-empty password before disabling passwordless login",
        );
    }
    if let Some(password) = req.password.as_deref()
        && (!target.is_admin() || !password.is_empty())
    {
        if let Err(response) = validate_password_request(password, allow_empty) {
            return response;
        }
        target.pwd_hash = crate::auth::hash_password(password);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        target.pwd_ts = now.max(target.pwd_ts.saturating_add(1));
        target.password_unset = password.is_empty();
    }
    let path = match req.local_path.as_deref() {
        Some(path) => match local_path(path) {
            Ok(path) => path,
            Err(err) => {
                tracing::warn!(error = %err, "invalid user local path");
                return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
            }
        },
        None => target.local_path.clone(),
    };
    target.username = username.to_string();
    target.permission = permission;
    if let Some(disabled) = req.disabled {
        target.disabled = disabled;
    }
    match sqlx::query("UPDATE `x_users` SET `username` = ?, `pwd_hash` = ?, `pwd_ts` = ?, `local_path` = ?, `password_unset` = ?, `disabled` = ?, `permission` = ? WHERE `id` = ?")
        .bind(&target.username).bind(&target.pwd_hash).bind(target.pwd_ts).bind(path).bind(target.password_unset).bind(target.disabled).bind(target.permission).bind(id).execute(&state.pool).await {
        Ok(_) => api_success(()), Err(err) => { tracing::error!(error = %err, "failed to update user"); api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, "Internal server error") }
    }
}

pub async fn admin_user_delete_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) = require_admin(&headers, &state).await {
        return *response;
    }
    let Some(id) = query.id else {
        return api_error(StatusCode::BAD_REQUEST, 400, "missing id");
    };
    match get_user_by_id(&state.pool, id).await {
        Ok(Some(user)) if user.is_admin() => {
            api_error(StatusCode::BAD_REQUEST, 400, "admin user cannot be deleted")
        }
        Ok(Some(_)) => match delete_user(&state.pool, id).await {
            Ok(()) => api_success(()),
            Err(err) => {
                tracing::error!(error = %err, "failed to delete user");
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                )
            }
        },
        Ok(None) => api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        ),
    }
}
