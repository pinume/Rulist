use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use std::path::Path;

use crate::db::{
    compute_local_path, create_user_direct, delete_user, get_all_users, get_storages,
    get_user_by_id,
};
use crate::model::{AdminUserSaveReq, ROLE_ADMIN, User, UserWithMount};
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

fn directory_path_for_local_path(
    state: &crate::server::AppState,
    local_path: &str,
    storages: &[crate::model::Storage],
) -> String {
    if local_path.is_empty() {
        return String::new();
    }
    let Ok(local_path) = Path::new(local_path).canonicalize() else {
        return String::new();
    };
    let mut best: Option<(usize, String)> = None;

    for storage in storages {
        if storage.mount_path == "/.users" || storage.mount_path.starts_with("/.users/") {
            continue;
        }
        let Ok(driver) =
            crate::driver::local::LocalDriver::new(&storage.local_path, storage.show_hidden)
        else {
            continue;
        };
        let Ok(root) = driver.safe_resolve("") else {
            continue;
        };
        let Ok(relative) = local_path.strip_prefix(&root) else {
            continue;
        };
        let candidate = if storage.mount_path == "/" {
            format!("/{}", relative.to_string_lossy())
        } else if relative.as_os_str().is_empty() {
            storage.mount_path.clone()
        } else {
            format!(
                "{}/{}",
                storage.mount_path.trim_end_matches('/'),
                relative.to_string_lossy()
            )
        };
        let Some((mounted, subpath)) = state.storage.find_storage(&candidate) else {
            continue;
        };
        if mounted.storage.id != storage.id
            || mounted.driver.safe_resolve(&subpath).ok().as_deref() != Some(local_path.as_path())
        {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(root_len, _)| root.as_os_str().len() > *root_len)
        {
            best = Some((root.as_os_str().len(), candidate));
        }
    }

    best.map_or_else(String::new, |(_, path)| path)
}

fn to_user_with_mount(
    state: &crate::server::AppState,
    user: User,
    storages: &[crate::model::Storage],
) -> UserWithMount {
    let local_path = if user.is_admin() {
        String::new()
    } else {
        compute_local_path(&user.base_path, storages)
    };
    let directory_path = directory_path_for_local_path(state, &local_path, storages);

    UserWithMount {
        id: user.id,
        username: user.username,
        base_path: user.base_path,
        role: user.role,
        disabled: user.disabled,
        permission: user.permission,
        local_path,
        directory_path,
        otp: user.otp,
    }
}

pub async fn admin_user_list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) = require_admin(&headers, &state).await {
        return *response;
    }

    let users = match get_all_users(&state.pool).await {
        Ok(users) => users,
        Err(err) => {
            tracing::error!(error = %err, "failed to get all users");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };
    let storages = get_storages(&state.pool).await.unwrap_or_default();
    let content: Vec<UserWithMount> = users
        .into_iter()
        .map(|user| to_user_with_mount(&state, user, &storages))
        .collect();

    api_success(serde_json::json!({
        "total": content.len(),
        "content": content
    }))
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
    let user = match get_user_by_id(&state.pool, id).await {
        Ok(Some(user)) => user,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(err) => {
            tracing::error!(error = %err, id, "failed to get user by id");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };
    let storages = get_storages(&state.pool).await.unwrap_or_default();
    api_success(to_user_with_mount(&state, user, &storages))
}

fn validate_local_path(path: &str) -> Result<(), anyhow::Error> {
    crate::driver::local::LocalDriver::new(path.trim(), false)?;
    Ok(())
}

fn resolve_directory_path(
    state: &crate::server::AppState,
    admin: &User,
    path: &str,
) -> Result<String, anyhow::Error> {
    if path.trim() != path || !path.starts_with('/') || path.contains('\\') {
        anyhow::bail!("directory path must be absolute and canonical");
    }
    let path = if path == "/" {
        path
    } else {
        path.strip_suffix('/').unwrap_or(path)
    };
    if path != "/"
        && path[1..]
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        anyhow::bail!("directory path must be canonical");
    }
    if path == "/.users" || path.starts_with("/.users/") {
        anyhow::bail!("user storage paths cannot be assigned");
    }

    let path = crate::server::user_path(admin, path).map_err(anyhow::Error::msg)?;
    let (storage, subpath) = state
        .storage
        .find_storage(&path)
        .ok_or_else(|| anyhow::anyhow!("directory is not in a mounted storage"))?;
    let local_path = storage.driver.safe_resolve(&subpath)?;
    if !local_path.is_dir() {
        anyhow::bail!("selected path is not a directory");
    }
    Ok(local_path.to_string_lossy().into_owned())
}

fn requested_local_path(
    state: &crate::server::AppState,
    admin: &User,
    req: &AdminUserSaveReq,
) -> Result<Option<String>, anyhow::Error> {
    match req.directory_path.as_deref() {
        Some(path) if !path.is_empty() => resolve_directory_path(state, admin, path).map(Some),
        Some(_) => Ok(None),
        None => Ok(req
            .local_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_owned)),
    }
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

pub async fn admin_user_create_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    let admin = match require_admin(&headers, &state).await {
        Ok(user) => user,
        Err(response) => return *response,
    };

    let username = req.username.trim();
    if username.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Username cannot be empty");
    }
    if req.role.unwrap_or(0) == ROLE_ADMIN {
        return api_error(StatusCode::BAD_REQUEST, 400, "admin user cannot be created");
    }

    let permission = req.permission.unwrap_or(0);
    let allow_empty = permission & (1 << crate::model::PERM_ALLOW_EMPTY_PASSWORD) != 0;
    let password = req.password.as_deref().unwrap_or("");
    if let Err(response) = validate_password_request(password, allow_empty) {
        return response;
    }

    let local_path = match requested_local_path(&state, &admin, &req) {
        Ok(Some(path)) => path,
        Ok(None) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Local directory is required for non-admin users",
            );
        }
        Err(err) => {
            tracing::warn!(error = %err, "invalid user local path");
            return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
        }
    };
    if req.directory_path.is_none()
        && let Err(err) = validate_local_path(&local_path)
    {
        tracing::warn!(error = %err, "invalid user local path");
        return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
    }

    if let Err(err) = create_user_direct(
        &state.pool,
        username,
        password,
        0,
        Some(&local_path),
        permission,
        req.disabled.unwrap_or(false),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to create user");
        let message = err.to_string();
        if message.contains("UNIQUE") || message.contains("unique") {
            return api_error(StatusCode::CONFLICT, 409, "Username already exists");
        }
        return api_error(StatusCode::BAD_REQUEST, 400, "Failed to create user");
    }

    if let Err(err) = state.storage.reload_from_db(&state.pool).await {
        tracing::error!(error = %err, "failed to reload storage manager");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Storage configuration was saved but failed to reload",
        );
    }

    api_success(())
}

pub async fn admin_user_update_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    let admin = match require_admin(&headers, &state).await {
        Ok(user) => user,
        Err(response) => return *response,
    };

    let username = req.username.trim();
    if username.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Username cannot be empty");
    }
    let Some(target_id) = req.id else {
        return api_error(StatusCode::BAD_REQUEST, 400, "missing id");
    };
    let mut target = match get_user_by_id(&state.pool, target_id).await {
        Ok(Some(user)) => user,
        _ => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
    };

    if target.is_admin() {
        if username != "admin" {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "admin username cannot be changed",
            );
        }
        if req.disabled == Some(true) {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "admin user cannot be disabled",
            );
        }
    }

    let next_permission = req.permission.unwrap_or(target.permission);
    let allow_empty =
        !target.is_admin() && next_permission & (1 << crate::model::PERM_ALLOW_EMPTY_PASSWORD) != 0;
    let nonempty_password_supplied = req
        .password
        .as_deref()
        .is_some_and(|password| !password.is_empty());
    if !target.is_admin() && target.password_unset && !allow_empty && !nonempty_password_supplied {
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
        let old_pwd_ts = target.pwd_ts;
        target.pwd_hash = crate::auth::hash_password(password);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        target.pwd_ts = now.max(old_pwd_ts.saturating_add(1));
        target.password_unset = password.is_empty();
    }

    target.username = username.to_string();
    target.permission = next_permission;
    if let Some(disabled) = req.disabled {
        target.disabled = disabled;
    }

    let local_path = match requested_local_path(&state, &admin, &req) {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!(error = %err, "invalid user local path");
            return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
        }
    };
    if req.directory_path.is_none()
        && let Some(path) = local_path.as_deref()
        && let Err(err) = validate_local_path(path)
    {
        tracing::warn!(error = %err, "invalid user local path");
        return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
    }

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(error = %err, "failed to begin user update transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    if let Err(err) = sqlx::query(
        "UPDATE `x_users` SET `username` = ?, `pwd_hash` = ?, `pwd_ts` = ?, `password_unset` = ?, `disabled` = ?, `permission` = ? WHERE `id` = ?",
    )
    .bind(&target.username)
    .bind(&target.pwd_hash)
    .bind(target.pwd_ts)
    .bind(target.password_unset)
    .bind(if target.disabled { 1 } else { 0 })
    .bind(target.permission)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!(error = %err, "failed to update user");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if target.is_admin()
        && target.base_path != "/"
        && let Err(err) = sqlx::query("UPDATE `x_users` SET `base_path` = '/' WHERE `id` = ?")
            .bind(target_id)
            .execute(&mut *tx)
            .await
    {
        tracing::error!(error = %err, "failed to restore admin base path");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if !target.is_admin()
        && let Some(local_path) = local_path
        && let Err(err) =
            crate::db::set_user_dir_on_connection(&mut tx, target_id, &local_path).await
    {
        tracing::error!(error = %err, "failed to update user storage");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = tx.commit().await {
        tracing::error!(error = %err, "failed to commit user update transaction");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = state.storage.reload_from_db(&state.pool).await {
        tracing::error!(error = %err, "failed to reload storage manager");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Storage configuration was saved but failed to reload",
        );
    }

    api_success(())
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
    let target = match get_user_by_id(&state.pool, id).await {
        Ok(Some(user)) => user,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(err) => {
            tracing::error!(error = %err, "failed to get user for deletion");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };
    if target.is_admin() {
        return api_error(StatusCode::BAD_REQUEST, 400, "admin user cannot be deleted");
    }

    if let Err(err) = delete_user(&state.pool, id).await {
        tracing::error!(error = %err, "failed to delete user");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }
    if let Err(err) = state.storage.reload_from_db(&state.pool).await {
        tracing::error!(error = %err, "failed to reload storage manager");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Storage configuration was saved but failed to reload",
        );
    }

    api_success(())
}
