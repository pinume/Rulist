use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use std::path::Path;

use crate::db::{compute_local_path, get_all_users, get_storages, get_user_by_id};
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
        if storage.driver != "Local"
            || storage.mount_path == "/.users"
            || storage.mount_path.starts_with("/.users/")
        {
            continue;
        }
        let Ok(driver) =
            crate::driver::local::LocalDriver::new(storage.addition.as_deref().unwrap_or_default())
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
    u: User,
    storages: &[crate::model::Storage],
) -> UserWithMount {
    let local_path = if u.is_admin() {
        String::new()
    } else {
        compute_local_path(&u.base_path, storages)
    };
    let directory_path = directory_path_for_local_path(state, &local_path, storages);
    UserWithMount {
        id: u.id,
        username: u.username,
        base_path: u.base_path,
        role: u.role,
        disabled: u.disabled,
        permission: u.permission,
        local_path,
        directory_path,
        otp: u.otp,
    }
}

pub async fn admin_user_list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
) -> Response {
    if let Err(res) = require_admin(&headers, &state).await {
        return *res;
    }

    let users = match get_all_users(&state.pool).await {
        Ok(u) => u,
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
        .map(|u| to_user_with_mount(&state, u, &storages))
        .collect();

    let total = content.len() as i64;
    api_success(serde_json::json!({
        "content": content,
        "total": total
    }))
}

pub async fn admin_user_get_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    if let Err(res) = require_admin(&headers, &state).await {
        return *res;
    }

    let id = match query.id {
        Some(id) => id,
        None => return api_error(StatusCode::BAD_REQUEST, 400, "missing id"),
    };

    let target_user = match get_user_by_id(&state.pool, id).await {
        Ok(Some(u)) => u,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(err) => {
            tracing::error!(error = %err, id = id, "failed to get user by id");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    let storages = get_storages(&state.pool).await.unwrap_or_default();
    api_success(to_user_with_mount(&state, target_user, &storages))
}

fn validate_local_path(path: &str) -> Result<(), anyhow::Error> {
    let addition = serde_json::json!({
        "root_folder_path": path.trim()
    })
    .to_string();

    crate::driver::local::LocalDriver::new(&addition)?;

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
    if path == "/.users" || path.starts_with("/.users/") {
        anyhow::bail!("user storage paths cannot be assigned");
    }
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

pub async fn admin_user_create_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    let admin = match require_admin(&headers, &state).await {
        Ok(user) => user,
        Err(res) => return *res,
    };

    if req.username.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Username cannot be empty");
    }

    let raw_pwd = req.password.as_deref().unwrap_or("");
    let allow_empty_pwd =
        req.permission.unwrap_or(0) & (1 << crate::model::PERM_ALLOW_EMPTY_PASSWORD) != 0;
    if raw_pwd.is_empty() && !allow_empty_pwd {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "Password cannot be empty unless 'allow empty password' permission is granted",
        );
    }

    let role = req.role.unwrap_or(0);
    if role == ROLE_ADMIN {
        return api_error(StatusCode::BAD_REQUEST, 400, "admin user cannot be created");
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

    let salt = crate::auth::rand_string(16);
    let s_hash = crate::auth::static_hash(raw_pwd);
    let encoded_pwd = crate::auth::encode_argon2_hash(&s_hash, &salt);
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin user creation transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    let res = sqlx::query(
        "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.username)
    .bind(&encoded_pwd)
    .bind(now_ts)
    .bind(&salt)
    .bind("/")
    .bind(role)
    .bind(if req.disabled.unwrap_or(false) { 1 } else { 0 })
    .bind(req.permission.unwrap_or(0))
    .execute(&mut *tx)
    .await;

    let new_id = match res {
        Ok(r) => r.last_insert_rowid(),
        Err(err) => {
            tracing::warn!(error = %err, "failed to insert user");
            let err_str = err.to_string();
            if err_str.contains("UNIQUE") || err_str.contains("unique") {
                return api_error(StatusCode::CONFLICT, 409, "Username already exists");
            }
            return api_error(StatusCode::BAD_REQUEST, 400, "Failed to create user");
        }
    };

    let mount_path = format!("/.users/{new_id}");
    if let Err(e) = sqlx::query(
        "UPDATE `x_users`
             SET `base_path` = ?
             WHERE `id` = ?",
    )
    .bind(&mount_path)
    .bind(new_id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!(error = %e, "failed to update user base path");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    let addition = serde_json::json!({
        "root_folder_path": local_path
    })
    .to_string();

    if let Err(e) = sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `order`, `driver`, `addition`, `status`, `disabled`) VALUES (?, 0, 'Local', ?, 'work', 0)",
        )
        .bind(&mount_path)
        .bind(&addition)
        .execute(&mut *tx)
        .await
        {
            tracing::error!(error = %e, "failed to create user storage");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit user creation transaction");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = state.storage.reload_from_db(&state.pool).await {
        tracing::error!(
            error = %err,
            "failed to reload storage manager"
        );
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
        Err(res) => return *res,
    };

    if req.username.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 400, "Username cannot be empty");
    }

    let target_id = match req.id {
        Some(id) => id,
        None => return api_error(StatusCode::BAD_REQUEST, 400, "missing id"),
    };

    let mut target_user = match get_user_by_id(&state.pool, target_id).await {
        Ok(Some(u)) => u,
        _ => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
    };

    if target_user.is_admin() {
        if req.username != "admin" {
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

    let allow_empty_pwd = if target_user.is_admin() {
        false
    } else {
        req.permission.unwrap_or(target_user.permission)
            & (1 << crate::model::PERM_ALLOW_EMPTY_PASSWORD)
            != 0
    };

    if let Some(pwd) = req.password.as_deref()
        && (!target_user.is_admin() || !pwd.is_empty())
    {
        if pwd.is_empty() && !allow_empty_pwd {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Password cannot be empty unless 'allow empty password' permission is granted",
            );
        }
        if target_user.is_admin() && !crate::auth::valid_password(pwd) {
            return api_error(
                StatusCode::BAD_REQUEST,
                400,
                "Password length must be between 8 and 128 characters",
            );
        }
        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash(pwd);
        let encoded_pwd = crate::auth::encode_argon2_hash(&s_hash, &salt);
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        target_user.pwd_hash = encoded_pwd;
        target_user.salt = salt;
        target_user.pwd_ts = now_ts.max(target_user.pwd_ts + 1);
        target_user.password_unset = false;
    }

    target_user.username = req.username.clone();
    if let Some(dis) = req.disabled {
        target_user.disabled = dis;
    }
    if let Some(perm) = req.permission {
        target_user.permission = perm;
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
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin user update transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    if let Err(e) = sqlx::query(
        "UPDATE `x_users` SET `username` = ?, `pwd_hash` = ?, `salt` = ?, `pwd_ts` = ?, `password_unset` = ?, `disabled` = ?, `permission` = ? WHERE `id` = ?"
    )
    .bind(&target_user.username)
    .bind(&target_user.pwd_hash)
    .bind(&target_user.salt)
    .bind(target_user.pwd_ts)
    .bind(target_user.password_unset)
    .bind(if target_user.disabled { 1 } else { 0 })
    .bind(target_user.permission)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!(error = %e, "failed to update user");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if !target_user.is_admin()
        && let Some(local_path) = local_path
    {
        let user_mount = format!("/.users/{}", target_id);
        let addition = serde_json::json!({
            "root_folder_path": local_path
        })
        .to_string();

        // Check if storage exists
        let exists: Option<i64> =
            match sqlx::query_scalar("SELECT `id` FROM `x_storages` WHERE `mount_path` = ?")
                .bind(&user_mount)
                .fetch_optional(&mut *tx)
                .await
            {
                Ok(value) => value,
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        mount_path = %user_mount,
                        "failed to query user storage"
                    );
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Internal server error",
                    );
                }
            };

        if exists.is_some() {
            if let Err(e) =
                sqlx::query("UPDATE `x_storages` SET `addition` = ? WHERE `mount_path` = ?")
                    .bind(&addition)
                    .bind(&user_mount)
                    .execute(&mut *tx)
                    .await
            {
                tracing::error!(error = %e, "failed to update user storage");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        } else {
            if let Err(e) = sqlx::query(
                "INSERT INTO `x_storages` (`mount_path`, `order`, `driver`, `addition`, `status`, `disabled`) VALUES (?, 0, 'Local', ?, 'work', 0)"
            )
            .bind(&user_mount)
            .bind(&addition)
            .execute(&mut *tx)
            .await
            {
                tracing::error!(error = %e, "failed to insert user storage");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        }

        if target_user.base_path != user_mount
            && let Err(e) = sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
                .bind(&user_mount)
                .bind(target_id)
                .execute(&mut *tx)
                .await
        {
            tracing::error!(error = %e, "failed to update user mount path");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    } else if target_user.is_admin() && target_user.base_path != "/" {
        if let Err(err) = sqlx::query("UPDATE `x_users` SET `base_path` = '/' WHERE `id` = ?")
            .bind(target_id)
            .execute(&mut *tx)
            .await
        {
            tracing::error!(
                error = %err,
                user_id = target_id,
                "failed to restore admin base path"
            );
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit user update transaction");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = state.storage.reload_from_db(&state.pool).await {
        tracing::error!(
            error = %err,
            "failed to reload storage manager"
        );
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
    if let Err(res) = require_admin(&headers, &state).await {
        return *res;
    }

    let id = match query.id {
        Some(id) => id,
        None => return api_error(StatusCode::BAD_REQUEST, 400, "missing id"),
    };

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin user delete transaction");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    let target_user = match sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(e) => {
            tracing::error!(error = %e, id = id, "failed to query user for deletion");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    if target_user.is_admin() {
        return api_error(StatusCode::BAD_REQUEST, 400, "admin user cannot be deleted");
    }

    if let Err(e) = sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(error = %e, id = id, "failed to delete otp pending for user");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(e) = sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(error = %e, id = id, "failed to delete user from database");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    let user_mount = format!("/.users/{}", id);
    if let Err(e) = sqlx::query("DELETE FROM `x_storages` WHERE `mount_path` = ?")
        .bind(&user_mount)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(error = %e, mount = %user_mount, "failed to delete user storage");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit user deletion transaction");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    if let Err(err) = state.storage.reload_from_db(&state.pool).await {
        tracing::error!(
            error = %err,
            "failed to reload storage manager"
        );
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Storage configuration was saved but failed to reload",
        );
    }

    api_success(())
}

pub async fn admin_user_cancel_2fa_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    if let Err(res) = require_admin(&headers, &state).await {
        return *res;
    }

    let Some(id) = query.id else {
        return api_error(StatusCode::BAD_REQUEST, 400, "missing id");
    };
    let target = match get_user_by_id(&state.pool, id).await {
        Ok(Some(user)) => user,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 404, "user not found"),
        Err(err) => {
            tracing::error!(error = %err, "failed to get user for 2fa cancellation");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Failed to cancel 2FA",
            );
        }
    };
    if target.is_admin() {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "admin 2FA must be cancelled by its owner",
        );
    }
    if let Err(err) =
        sqlx::query("UPDATE `x_users` SET `otp_secret` = '', `last_otp_step` = -1 WHERE `id` = ?")
            .bind(id)
            .execute(&state.pool)
            .await
    {
        tracing::error!(error = %err, "failed to cancel 2fa");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Failed to cancel 2FA",
        );
    }
    api_success(())
}
