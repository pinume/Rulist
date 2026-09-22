use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;

use crate::db::{compute_local_path, get_all_users, get_storages, get_user_by_id};
use crate::model::{AdminUserSaveReq, ROLE_ADMIN, User, UserWithMount};
use crate::server::{SharedState, api_error, api_success, authenticate_user};

#[derive(Debug, Deserialize)]
pub struct IdQuery {
    pub id: Option<i64>,
}

pub async fn admin_user_list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::OK, 401, "Authentication required"),
    };
    if !user.is_admin() {
        return api_error(StatusCode::OK, 403, "Permission denied");
    }

    let users = match get_all_users(&state.pool).await {
        Ok(u) => u,
        Err(err) => return api_error(StatusCode::OK, 500, err.to_string()),
    };
    let storages = get_storages(&state.pool).await.unwrap_or_default();

    let content: Vec<UserWithMount> = users
        .into_iter()
        .map(|u| {
            let local_path = compute_local_path(&u.base_path, &storages);
            UserWithMount {
                id: u.id,
                username: u.username,
                base_path: u.base_path,
                role: u.role,
                disabled: u.disabled,
                permission: u.permission,
                sso_id: u.sso_id,
                local_path,
                otp: u.otp,
            }
        })
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
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::OK, 401, "Authentication required"),
    };
    if !user.is_admin() {
        return api_error(StatusCode::OK, 403, "Permission denied");
    }

    let id = match query.id {
        Some(id) => id,
        None => return api_error(StatusCode::OK, 400, "missing id"),
    };

    let target_user = match get_user_by_id(&state.pool, id).await {
        Ok(Some(u)) => u,
        Ok(None) => return api_error(StatusCode::OK, 404, "user not found"),
        Err(err) => return api_error(StatusCode::OK, 500, err.to_string()),
    };

    let storages = get_storages(&state.pool).await.unwrap_or_default();
    let local_path = compute_local_path(&target_user.base_path, &storages);

    let res = UserWithMount {
        id: target_user.id,
        username: target_user.username,
        base_path: target_user.base_path,
        role: target_user.role,
        disabled: target_user.disabled,
        permission: target_user.permission,
        sso_id: target_user.sso_id,
        local_path,
        otp: target_user.otp,
    };

    api_success(res)
}

fn validate_local_path(path: &str) -> Result<(), anyhow::Error> {
    let addition = serde_json::json!({
        "root_folder_path": path.trim()
    })
    .to_string();

    crate::driver::local::LocalDriver::new(&addition)?;

    Ok(())
}

pub async fn admin_user_create_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::OK, 401, "Authentication required"),
    };
    if !user.is_admin() {
        return api_error(StatusCode::OK, 403, "Permission denied");
    }

    let raw_pwd = req.password.as_deref().unwrap_or("").trim();
    if raw_pwd.is_empty() {
        return api_error(StatusCode::OK, 400, "password is required");
    }

    if let Some(local_path) = req.local_path.as_deref()
        && !local_path.trim().is_empty()
        && let Err(err) = validate_local_path(local_path)
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
        Err(e) => return api_error(StatusCode::OK, 500, e.to_string()),
    };

    let res = sqlx::query(
        "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&req.username)
    .bind(&encoded_pwd)
    .bind(now_ts)
    .bind(&salt)
    .bind("/")
    .bind(req.role.unwrap_or(0))
    .bind(if req.disabled.unwrap_or(false) { 1 } else { 0 })
    .bind(req.permission.unwrap_or(0))
    .execute(&mut *tx)
    .await;

    let new_id = match res {
        Ok(r) => r.last_insert_rowid(),
        Err(err) => return api_error(StatusCode::OK, 400, err.to_string()),
    };

    if let Some(local_path) = req.local_path
        && !local_path.trim().is_empty()
    {
        let mount_path = format!("/.users/{}", new_id);
        let addition = serde_json::json!({
            "root_folder_path": local_path.trim()
        })
        .to_string();

        if let Err(e) = sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
            .bind(&mount_path)
            .bind(new_id)
            .execute(&mut *tx)
            .await
        {
            return api_error(StatusCode::OK, 500, e.to_string());
        }

        if let Err(e) = sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `order`, `driver`, `addition`, `status`, `disabled`) VALUES (?, 0, 'Local', ?, 'work', 0)"
        )
        .bind(&mount_path)
        .bind(&addition)
        .execute(&mut *tx)
        .await
        {
            return api_error(StatusCode::OK, 500, e.to_string());
        }
    }

    if let Err(e) = tx.commit().await {
        return api_error(StatusCode::OK, 500, e.to_string());
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
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::OK, 401, "Authentication required"),
    };
    if !user.is_admin() {
        return api_error(StatusCode::OK, 403, "Permission denied");
    }

    let target_id = match req.id {
        Some(id) => id,
        None => return api_error(StatusCode::OK, 400, "missing id"),
    };

    let mut target_user = match get_user_by_id(&state.pool, target_id).await {
        Ok(Some(u)) => u,
        _ => return api_error(StatusCode::OK, 404, "user not found"),
    };

    if target_user.is_admin() && req.disabled.unwrap_or(false) {
        return api_error(StatusCode::OK, 400, "admin user can not be disabled");
    }

    if let Some(pwd) = req.password
        && !pwd.trim().is_empty()
    {
        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash(&pwd);
        let encoded_pwd = crate::auth::encode_argon2_hash(&s_hash, &salt);
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        target_user.pwd_hash = encoded_pwd;
        target_user.salt = salt;
        target_user.pwd_ts = now_ts;
    }

    target_user.username = req.username;
    if let Some(dis) = req.disabled {
        target_user.disabled = dis;
    }
    if let Some(perm) = req.permission {
        target_user.permission = perm;
    }

    if let Some(local_path) = req.local_path.as_deref()
        && !local_path.trim().is_empty()
        && let Err(err) = validate_local_path(local_path)
    {
        tracing::warn!(error = %err, "invalid user local path");
        return api_error(StatusCode::BAD_REQUEST, 400, "Invalid local directory");
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => return api_error(StatusCode::OK, 500, e.to_string()),
    };

    if let Err(e) = sqlx::query(
        "UPDATE `x_users` SET `username` = ?, `pwd_hash` = ?, `salt` = ?, `pwd_ts` = ?, `disabled` = ?, `permission` = ? WHERE `id` = ?"
    )
    .bind(&target_user.username)
    .bind(&target_user.pwd_hash)
    .bind(&target_user.salt)
    .bind(target_user.pwd_ts)
    .bind(if target_user.disabled { 1 } else { 0 })
    .bind(target_user.permission)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    {
        return api_error(StatusCode::OK, 500, e.to_string());
    }

    if let Some(local_path) = req.local_path
        && !local_path.trim().is_empty()
    {
        let user_mount = format!("/.users/{}", target_id);
        let addition = serde_json::json!({
            "root_folder_path": local_path.trim()
        })
        .to_string();

        // Check if storage exists
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT `id` FROM `x_storages` WHERE `mount_path` = ?")
                .bind(&user_mount)
                .fetch_optional(&mut *tx)
                .await
                .unwrap_or(None);

        if exists.is_some() {
            if let Err(e) =
                sqlx::query("UPDATE `x_storages` SET `addition` = ? WHERE `mount_path` = ?")
                    .bind(&addition)
                    .bind(&user_mount)
                    .execute(&mut *tx)
                    .await
            {
                return api_error(StatusCode::OK, 500, e.to_string());
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
                return api_error(StatusCode::OK, 500, e.to_string());
            }

            if let Err(e) = sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
                .bind(&user_mount)
                .bind(target_id)
                .execute(&mut *tx)
                .await
            {
                return api_error(StatusCode::OK, 500, e.to_string());
            }
        }
    }

    if let Err(e) = tx.commit().await {
        return api_error(StatusCode::OK, 500, e.to_string());
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
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::OK, 401, "Authentication required"),
    };
    if !user.is_admin() {
        return api_error(StatusCode::OK, 403, "Permission denied");
    }

    let id = match query.id {
        Some(id) => id,
        None => return api_error(StatusCode::OK, 400, "missing id"),
    };

    if id == 1 {
        return api_error(StatusCode::OK, 400, "cannot delete initial admin");
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => return api_error(StatusCode::OK, 500, e.to_string()),
    };

    let target_user = match sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return api_error(StatusCode::OK, 404, "user not found"),
        Err(e) => return api_error(StatusCode::OK, 500, e.to_string()),
    };

    if target_user.is_admin() {
        let admin_count: i64 =
            match sqlx::query_scalar("SELECT count(*) FROM `x_users` WHERE `role` = ?")
                .bind(ROLE_ADMIN)
                .fetch_one(&mut *tx)
                .await
            {
                Ok(c) => c,
                Err(e) => return api_error(StatusCode::OK, 500, e.to_string()),
            };
        if admin_count <= 1 {
            return api_error(StatusCode::OK, 400, "cannot delete last admin user");
        }
    }

    if let Err(e) = sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return api_error(StatusCode::OK, 500, e.to_string());
    }

    if let Err(e) = sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return api_error(StatusCode::OK, 500, e.to_string());
    }

    let user_mount = format!("/.users/{}", id);
    if let Err(e) = sqlx::query("DELETE FROM `x_storages` WHERE `mount_path` = ?")
        .bind(&user_mount)
        .execute(&mut *tx)
        .await
    {
        return api_error(StatusCode::OK, 500, e.to_string());
    }

    if let Err(e) = tx.commit().await {
        return api_error(StatusCode::OK, 500, e.to_string());
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
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => return api_error(StatusCode::OK, 401, "Authentication required"),
    };
    if !user.is_admin() {
        return api_error(StatusCode::OK, 403, "Permission denied");
    }

    if let Some(id) = query.id
        && let Err(err) = sqlx::query("UPDATE `x_users` SET `otp_secret` = '' WHERE `id` = ?")
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
