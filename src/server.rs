use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::header::{
    ACCEPT_RANGES, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::auth::{
    generate_jwt, generate_otp_secret, generate_totp_qr, parse_jwt, verify_password, verify_totp,
};
use crate::config::Config;
use crate::db::{DbPool, get_admin, get_public_settings, get_setting, get_user_by_name};
use crate::driver::{SharedStorageManager, StorageManager};
use crate::model::{
    AdminUserSaveReq, ApiResponse, BatchRenameReq, ConflictPolicy, DirItem, FsDirNamesReq,
    FsDirsReq, FsGetReq, FsLinkReq, FsLinkResp, FsListReq, FsListResp, FsMoveCopyReq,
    FsRecursiveMoveReq, FsRenameReq, LoginReq, TwoFaGenerateReq, TwoFaVerifyReq, UpdateCurrentReq,
    User, UserWithMount, sort_files,
};
use crate::sign::{sign_path, verify_sign};

pub struct AppState {
    pub pool: DbPool,
    pub config: Config,
    pub storage: SharedStorageManager,
}

pub type SharedState = Arc<AppState>;

pub async fn run_server(
    config: Config,
    pool: DbPool,
    storage: StorageManager,
) -> Result<(), anyhow::Error> {
    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        storage: Arc::new(storage),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health, favicon, manifest, robots
        .route("/ping", get(crate::static_files::ping_handler))
        .route("/favicon.ico", get(crate::static_files::favicon_handler))
        .route("/robots.txt", get(crate::static_files::robots_handler))
        .route("/manifest.json", get(crate::static_files::manifest_handler))
        .route("/rulist.svg", get(crate::static_files::rulist_svg_handler))
        .route("/rulist.png", get(crate::static_files::rulist_png_handler))
        // Static assets from frontend dist
        .route(
            "/assets/{*path}",
            get(crate::static_files::dist_assets_handler),
        )
        .route(
            "/static/{*path}",
            get(crate::static_files::dist_assets_handler),
        )
        .route(
            "/streamer/{*path}",
            get(crate::static_files::dist_assets_handler),
        )
        // Settings
        .route(
            "/api/public/settings",
            get(public_settings_handler).post(public_settings_handler),
        )
        // Authentication
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/me", get(current_user_handler))
        .route("/api/me/update", post(update_current_handler))
        .route("/api/auth/logout", get(logout_handler).post(logout_handler))
        .route("/api/auth/2fa/generate", post(two_factor_generate_handler))
        .route("/api/auth/2fa/verify", post(two_factor_verify_handler))
        // File system read
        .route("/api/fs/list", post(fs_list_handler).get(fs_list_handler))
        .route("/api/fs/get", post(fs_get_handler).get(fs_get_handler))
        .route("/api/fs/dirs", post(fs_dirs_handler).get(fs_dirs_handler))
        // File system write
        .route("/api/fs/mkdir", post(fs_mkdir_handler))
        .route("/api/fs/rename", post(fs_rename_handler))
        .route("/api/fs/move", post(fs_move_handler))
        .route("/api/fs/recursive_move", post(fs_recursive_move_handler))
        .route("/api/fs/copy", post(fs_copy_handler))
        .route("/api/fs/remove", post(fs_remove_handler))
        .route("/api/fs/remove_empty_directory", post(fs_remove_handler))
        .route("/api/fs/put", put(fs_put_handler))
        // File system batch & link
        .route("/api/fs/batch_rename", post(fs_batch_rename_handler))
        .route("/api/fs/link", post(fs_link_handler))
        // Admin User Management
        .route("/api/admin/user/list", get(admin_user_list_handler))
        .route("/api/admin/user/get", get(admin_user_get_handler))
        .route("/api/admin/user/create", post(admin_user_create_handler))
        .route("/api/admin/user/update", post(admin_user_update_handler))
        .route(
            "/api/admin/user/delete",
            post(admin_user_delete_handler).get(admin_user_delete_handler),
        )
        .route(
            "/api/admin/user/cancel_2fa",
            post(admin_user_cancel_2fa_handler).get(admin_user_cancel_2fa_handler),
        )
        // Direct download & streaming
        .route(
            "/d/{*path}",
            get(raw_download_handler).head(raw_download_handler),
        )
        .route(
            "/p/{*path}",
            get(raw_preview_handler).head(raw_preview_handler),
        )
        // SPA Fallback for all other routes
        .fallback(crate::static_files::spa_fallback_handler)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr =
        format!("{}:{}", config.scheme.address, config.scheme.http_port).parse()?;
    info!("start HTTP server @ {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn public_settings_handler(State(state): State<SharedState>) -> Response {
    match get_public_settings(&state.pool).await {
        Ok(settings) => Json(ApiResponse::success(settings)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Authentication Handlers
// ---------------------------------------------------------------------------

async fn login_handler(State(state): State<SharedState>, Json(req): Json<LoginReq>) -> Response {
    let user = match get_user_by_name(&state.pool, &req.username).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "user not found")),
            )
                .into_response();
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    if user.disabled {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "user is disabled")),
        )
            .into_response();
    }

    if !verify_password(&req.password, &user.pwd_hash, &user.salt) {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                400,
                "invalid username or password",
            )),
        )
            .into_response();
    }

    // Check 2FA if enabled
    if let Some(ref secret) = user.otp_secret
        && !secret.trim().is_empty()
    {
        let otp_code = req.otp_code.as_deref().unwrap_or("").trim();
        if otp_code.is_empty() {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(402, "OTP code is required")),
            )
                .into_response();
        }
        if !verify_totp(secret, otp_code) {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "invalid otp code")),
            )
                .into_response();
        }
    }

    match generate_jwt(
        &user.username,
        user.pwd_ts,
        &state.config.jwt_secret,
        state.config.token_expires_in,
    ) {
        Ok(token) => Json(ApiResponse::success(serde_json::json!({
            "token": token
        })))
        .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

async fn logout_handler() -> Response {
    Json(ApiResponse::success(())).into_response()
}

async fn two_factor_generate_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<TwoFaGenerateReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };

    let current_password = req.current_password.trim();
    if current_password.is_empty() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                400,
                "Current password is required",
            )),
        )
            .into_response();
    }

    if !verify_password(current_password, &user.pwd_hash, &user.salt) {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                403,
                "Current password is incorrect",
            )),
        )
            .into_response();
    }

    if user.otp {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "2FA is already enabled")),
        )
            .into_response();
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
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }

    let qr = match generate_totp_qr(&site_title, &user.username, &secret) {
        Ok(data_uri) => data_uri,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    Json(ApiResponse::success(serde_json::json!({
        "qr": qr,
        "secret": secret,
    })))
    .into_response()
}

async fn two_factor_verify_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<TwoFaVerifyReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };

    let clean_code = req.code.trim();
    if clean_code.is_empty() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                400,
                "Verification code is required",
            )),
        )
            .into_response();
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    let Some((secret, expires_at)) = pending else {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                400,
                "No pending 2FA session. Please generate a new secret.",
            )),
        )
            .into_response();
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
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                400,
                "2FA setup session expired. Please generate a new secret.",
            )),
        )
            .into_response();
    }

    if !verify_totp(&secret, clean_code) {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "Invalid verification code")),
        )
            .into_response();
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    if let Err(err) = sqlx::query("UPDATE `x_users` SET `otp_secret` = ? WHERE `id` = ?")
        .bind(&secret)
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }

    if let Err(err) = sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }

    if let Err(err) = tx.commit().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }

    Json(ApiResponse::success(())).into_response()
}

async fn update_current_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<UpdateCurrentReq>,
) -> Response {
    let mut user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
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
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(
                    400,
                    "Current password is required",
                )),
            )
                .into_response();
        }
        if !verify_password(current_password, &user.pwd_hash, &user.salt) {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(
                    403,
                    "Current password is incorrect",
                )),
            )
                .into_response();
        }
    }

    if let Some(new_pwd) = &req.password
        && !new_pwd.is_empty()
    {
        if new_pwd.len() < 8 || new_pwd.len() > 128 {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(
                    400,
                    "Password length must be between 8 and 128 characters",
                )),
            )
                .into_response();
        }

        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash(new_pwd);
        let encoded_pwd = crate::auth::encode_argon2_hash(&s_hash, &salt);
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Err(e) = sqlx::query(
            "UPDATE `x_users` SET `pwd_hash` = ?, `salt` = ?, `pwd_ts` = ? WHERE `id` = ?",
        )
        .bind(&encoded_pwd)
        .bind(&salt)
        .bind(now_ts)
        .bind(user.id)
        .execute(&state.pool)
        .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
        user.pwd_ts = now_ts;
    }

    if let Some(new_name) = &req.username {
        let clean_name = new_name.trim();
        if !clean_name.is_empty() && clean_name != user.username {
            if clean_name.len() > 64 {
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error(
                        400,
                        "Username length cannot exceed 64 characters",
                    )),
                )
                    .into_response();
            }

            if let Err(e) = sqlx::query("UPDATE `x_users` SET `username` = ? WHERE `id` = ?")
                .bind(clean_name)
                .bind(user.id)
                .execute(&state.pool)
                .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(500, e.to_string())),
                )
                    .into_response();
            }
        }
    }

    Json(ApiResponse::success(())).into_response()
}

async fn authenticate_user(headers: &HeaderMap, state: &AppState) -> Option<User> {
    let auth_header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    // Check if token matches admin token
    if let Ok(Some(admin_token)) = get_setting(&state.pool, "token").await
        && !admin_token.is_empty()
        && admin_token.as_bytes().ct_eq(token.as_bytes()).into()
    {
        return get_admin(&state.pool).await.ok().flatten();
    }

    // Parse JWT
    let claims = parse_jwt(token, &state.config.jwt_secret).ok()?;
    let user = get_user_by_name(&state.pool, &claims.username)
        .await
        .ok()
        .flatten()?;

    if user.disabled || user.pwd_ts != claims.pwd_ts {
        return None;
    }

    Some(user)
}

fn user_path(user: &User, requested: &str) -> Result<String, &'static str> {
    if requested
        .split(['/', '\\'])
        .any(|part| part == "." || part == "..")
    {
        return Err("invalid path");
    }
    let relative = requested.trim_start_matches('/');
    let base = user.base_path.trim_end_matches('/');
    Ok(if relative.is_empty() {
        if base.is_empty() {
            "/".to_string()
        } else {
            base.to_string()
        }
    } else {
        format!("{base}/{relative}")
    })
}

fn permitted(user: &User, bit: i32) -> bool {
    user.is_admin() || user.permission & (1 << bit) != 0
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

fn encode_url_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn permission_denied() -> Response {
    Json(ApiResponse::<()>::error(403, "Permission denied")).into_response()
}

async fn current_user_handler(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if let Some(user) = authenticate_user(&headers, &state).await {
        Json(ApiResponse::success(user)).into_response()
    } else {
        (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(401, "Authentication required")),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// File System Read Handlers
// ---------------------------------------------------------------------------

async fn fs_list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsListReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return Json(ApiResponse::<()>::error(401, "Authentication required")).into_response();
    };
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };

    match state.storage.list(&path).await {
        Ok(mut content) => {
            sort_files(&mut content, "name", "asc");

            // Attach signs and raw_urls to files
            let token = get_setting(&state.pool, "token")
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

            for item in &mut content {
                if !item.is_dir {
                    let item_path = format!("{}/{}", path.trim_end_matches('/'), item.name);
                    let sign = sign_path(&token, &item_path);
                    item.sign = sign.clone();
                    item.raw_url = format!("/p{}?sign={}", encode_url_path(&item_path), sign);
                }
            }

            let total = content.len() as i64;
            let resp = FsListResp {
                content,
                total,
                readme: String::new(),
                header: String::new(),
                write: permitted(&user, 3),
                provider: "Local".to_string(),
            };
            Json(ApiResponse::success(resp)).into_response()
        }
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

async fn fs_get_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsGetReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return Json(ApiResponse::<()>::error(401, "Authentication required")).into_response();
    };
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };

    match state.storage.get(&path).await {
        Ok(mut file) => {
            let token = get_setting(&state.pool, "token")
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

            if !file.is_dir {
                let s = sign_path(&token, &path);
                file.sign = s.clone();
                file.raw_url = format!("/p{}?sign={}", encode_url_path(&path), s);
            }

            Json(ApiResponse::success(file)).into_response()
        }
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

async fn fs_dirs_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsDirsReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return Json(ApiResponse::<()>::error(401, "Authentication required")).into_response();
    };
    if req.force_root && !user.is_admin() {
        return permission_denied();
    }
    let path = if req.force_root {
        Ok("/".to_string())
    } else {
        user_path(&user, &req.path)
    };
    let path = match path {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let files = match state.storage.list(&path).await {
        Ok(f) => f,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    let dirs: Vec<DirItem> = files
        .into_iter()
        .filter(|f| f.is_dir)
        .map(|f| DirItem {
            name: f.name,
            modified: f.modified,
        })
        .collect();

    Json(ApiResponse::success(dirs)).into_response()
}

// ---------------------------------------------------------------------------
// File System Write Handlers
// ---------------------------------------------------------------------------

async fn fs_mkdir_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<serde_json::Value>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(401, "unauthorized")),
        )
            .into_response();
    };
    if !permitted(&user, 3) {
        return permission_denied();
    }
    let path = match user_path(
        &user,
        req.get("path").and_then(|v| v.as_str()).unwrap_or(""),
    ) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    match state.storage.mkdir(&path).await {
        Ok(_) => Json(ApiResponse::success(serde_json::Value::Null)).into_response(),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

async fn fs_rename_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(401, "unauthorized")),
        )
            .into_response();
    };
    if !permitted(&user, 4) || !valid_name(&req.name) {
        return permission_denied();
    }
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    match state.storage.rename(&path, &req.name).await {
        Ok(_) => Json(ApiResponse::success(serde_json::Value::Null)).into_response(),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

async fn fs_move_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(401, "unauthorized")),
        )
            .into_response();
    };
    if !permitted(&user, 5) || req.names.iter().any(|name| !valid_name(name)) {
        return permission_denied();
    }
    let (src_dir, dst_dir) = match (
        user_path(&user, &req.src_dir),
        user_path(&user, &req.dst_dir),
    ) {
        (Ok(src), Ok(dst)) => (src, dst),
        _ => return permission_denied(),
    };

    let policy = req.policy();
    let mut moves = Vec::new();
    for name in &req.names {
        let src = format!("{}/{}", src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", dst_dir.trim_end_matches('/'), name);
        let dst_exists = state.storage.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return (
                        StatusCode::OK,
                        Json(ApiResponse::<()>::error(
                            403,
                            format!("file [{name}] exists"),
                        )),
                    )
                        .into_response();
                }
                ConflictPolicy::Skip => {
                    continue;
                }
                ConflictPolicy::Overwrite => {}
            }
        }
        moves.push((src, dst));
    }

    for (src, dst) in moves {
        if policy == ConflictPolicy::Overwrite && state.storage.get(&dst).await.is_ok() {
            let _ = state.storage.remove(&dst).await;
        }
        if let Err(err) = state.storage.move_to(&src, &dst).await {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

async fn fs_recursive_move_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRecursiveMoveReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return Json(ApiResponse::<()>::error(401, "Authentication required")).into_response();
    };
    if !permitted(&user, 5) {
        return permission_denied();
    }
    let (src_dir, dst_dir) = match (
        user_path(&user, &req.src_dir),
        user_path(&user, &req.dst_dir),
    ) {
        (Ok(src), Ok(dst)) => (src, dst),
        _ => return permission_denied(),
    };
    if src_dir == dst_dir || dst_dir.starts_with(&format!("{}/", src_dir.trim_end_matches('/'))) {
        return Json(ApiResponse::<()>::error(400, "invalid destination")).into_response();
    }

    let mut dirs_to_visit = vec![src_dir.clone()];
    let mut dirs_to_create = Vec::new();
    let mut files = Vec::new();

    let src_prefix = src_dir.trim_end_matches('/');

    while let Some(dir) = dirs_to_visit.pop() {
        let entries = match state.storage.list(&dir).await {
            Ok(entries) => entries,
            Err(err) => {
                return Json(ApiResponse::<()>::error(500, err.to_string())).into_response();
            }
        };
        for entry in entries {
            let path = format!("{}/{}", dir.trim_end_matches('/'), entry.name);
            let rel_path = path
                .strip_prefix(src_prefix)
                .unwrap_or(&path)
                .trim_start_matches('/')
                .to_string();

            if entry.is_dir {
                dirs_to_create.push(rel_path.clone());
                dirs_to_visit.push(path);
            } else {
                files.push((path, rel_path));
            }
        }
    }

    let policy = req.conflict_policy;
    let dst_prefix = dst_dir.trim_end_matches('/');
    let mut moves = Vec::new();

    for (src_file, rel_path) in files {
        let dst_file = format!("{}/{}", dst_prefix, rel_path);
        let dst_exists = state.storage.get(&dst_file).await.is_ok();

        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return (
                        StatusCode::OK,
                        Json(ApiResponse::<()>::error(
                            403,
                            format!("file [{rel_path}] exists"),
                        )),
                    )
                        .into_response();
                }
                ConflictPolicy::Skip => {
                    continue;
                }
                ConflictPolicy::Overwrite => {}
            }
        }
        moves.push((src_file, dst_file));
    }

    // Create destination directories if needed
    for rel_dir in &dirs_to_create {
        let target_dir = format!("{}/{}", dst_prefix, rel_dir);
        let _ = state.storage.mkdir(&target_dir).await;
    }

    for (src, dst) in moves {
        if policy == ConflictPolicy::Overwrite && state.storage.get(&dst).await.is_ok() {
            let _ = state.storage.remove(&dst).await;
        }
        if let Err(err) = state.storage.move_to(&src, &dst).await {
            return Json(ApiResponse::<()>::error(500, err.to_string())).into_response();
        }
    }

    Json(ApiResponse::success(())).into_response()
}

async fn fs_copy_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(401, "unauthorized")),
        )
            .into_response();
    };
    if !permitted(&user, 6) || req.names.iter().any(|name| !valid_name(name)) {
        return permission_denied();
    }
    let (src_dir, dst_dir) = match (
        user_path(&user, &req.src_dir),
        user_path(&user, &req.dst_dir),
    ) {
        (Ok(src), Ok(dst)) => (src, dst),
        _ => return permission_denied(),
    };

    let policy = req.policy();
    let mut copies = Vec::new();
    for name in &req.names {
        let src = format!("{}/{}", src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", dst_dir.trim_end_matches('/'), name);
        let dst_exists = state.storage.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return (
                        StatusCode::OK,
                        Json(ApiResponse::<()>::error(
                            403,
                            format!("file [{name}] exists"),
                        )),
                    )
                        .into_response();
                }
                ConflictPolicy::Skip => {
                    continue;
                }
                ConflictPolicy::Overwrite => {}
            }
        }
        copies.push((src, dst));
    }

    for (src, dst) in copies {
        if policy == ConflictPolicy::Overwrite && state.storage.get(&dst).await.is_ok() {
            let _ = state.storage.remove(&dst).await;
        }
        if let Err(err) = state.storage.copy_to(&src, &dst).await {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

async fn fs_remove_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsDirNamesReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(401, "unauthorized")),
        )
            .into_response();
    };
    if !permitted(&user, 7) || req.names.iter().any(|name| !valid_name(name)) {
        return permission_denied();
    }
    let dir = match user_path(&user, &req.dir) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    for name in req.names {
        let target = format!("{}/{}", dir.trim_end_matches('/'), name);
        if let Err(err) = state.storage.remove(&target).await {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

async fn fs_put_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(401, "unauthorized")),
        )
            .into_response();
    };
    if !permitted(&user, 3) {
        return permission_denied();
    }

    let file_path = match headers.get("File-Path").and_then(|h| h.to_str().ok()) {
        Some(p) => percent_decode(p),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error(400, "missing File-Path header")),
            )
                .into_response();
        }
    };
    let file_path = match user_path(&user, &file_path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let overwrite = headers.get("Overwrite").and_then(|h| h.to_str().ok()) == Some("true");

    let body = request.into_body();
    // Stream body to file
    let (ms, sub) = match state.storage.find_storage(&file_path) {
        Some(m) => m,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::error(404, "storage not found")),
            )
                .into_response();
        }
    };

    let target = match ms.driver.safe_resolve(&sub) {
        Ok(t) => t,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::error(400, err.to_string())),
            )
                .into_response();
        }
    };

    if sub.is_empty() {
        return permission_denied();
    }
    if !overwrite && target.exists() {
        return (
            StatusCode::CONFLICT,
            Json(ApiResponse::<()>::error(409, "file already exists")),
        )
            .into_response();
    }
    if let Some(parent) = target.parent()
        && let Err(err) = tokio::fs::create_dir_all(parent).await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let temp = target.with_file_name(format!(".rulist-upload-{}", crate::auth::rand_string(24)));
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
    {
        Ok(f) => f,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    let max_upload_bytes: u64 = 100 * 1024 * 1024 * 1024; // 100 GB default safety limit
    let mut uploaded_bytes: u64 = 0;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                uploaded_bytes += bytes.len() as u64;
                if uploaded_bytes > max_upload_bytes {
                    let _ = tokio::fs::remove_file(&temp).await;
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(ApiResponse::<()>::error(
                            413,
                            "payload too large: maximum upload size exceeded",
                        )),
                    )
                        .into_response();
                }
                if let Err(err) = file.write_all(&bytes).await {
                    let _ = tokio::fs::remove_file(&temp).await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse::<()>::error(500, err.to_string())),
                    )
                        .into_response();
                }
            }
            Err(err) => {
                let _ = tokio::fs::remove_file(&temp).await;
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::<()>::error(400, err.to_string())),
                )
                    .into_response();
            }
        }
    }

    if let Err(err) = file.flush().await {
        let _ = tokio::fs::remove_file(&temp).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }
    drop(file);
    if !overwrite && target.exists() {
        let _ = tokio::fs::remove_file(&temp).await;
        return (
            StatusCode::CONFLICT,
            Json(ApiResponse::<()>::error(409, "file already exists")),
        )
            .into_response();
    }
    if let Err(err) = tokio::fs::rename(&temp, &target).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response();
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

// ---------------------------------------------------------------------------
// File Streaming & Direct Download Handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SignQuery {
    sign: Option<String>,
}

async fn raw_download_handler(
    State(state): State<SharedState>,
    AxumPath(path): AxumPath<String>,
    Query(query): Query<SignQuery>,
    headers: HeaderMap,
) -> Response {
    stream_file(state, path, query.sign, headers, true).await
}

async fn raw_preview_handler(
    State(state): State<SharedState>,
    AxumPath(path): AxumPath<String>,
    Query(query): Query<SignQuery>,
    headers: HeaderMap,
) -> Response {
    stream_file(state, path, query.sign, headers, false).await
}

fn percent_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let input = s.as_bytes();
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%'
            && i + 2 < input.len()
            && let Ok(hex) = std::str::from_utf8(&input[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            bytes.push(byte);
            i += 3;
            continue;
        }
        bytes.push(input[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

async fn stream_file(
    state: SharedState,
    raw_path: String,
    sign: Option<String>,
    headers: HeaderMap,
    as_attachment: bool,
) -> Response {
    let clean_path = format!("/{}", raw_path.trim_start_matches('/'));
    if clean_path.split(['/', '\\']).any(|p| p == "." || p == "..") {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    // Check signature if sign_all is enabled or sign is provided
    let sign_all = get_setting(&state.pool, "sign_all")
        .await
        .ok()
        .flatten()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if sign_all || sign.is_some() {
        let token = get_setting(&state.pool, "token")
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let s = sign.unwrap_or_default();
        if verify_sign(&token, &clean_path, &s).is_err() {
            return (
                StatusCode::FORBIDDEN,
                "Invalid or expired download link signature",
            )
                .into_response();
        }
    } else {
        let Some(user) = authenticate_user(&headers, &state).await else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        let base = user.base_path.trim_end_matches('/');
        if !user.is_admin() && clean_path != base && !clean_path.starts_with(&format!("{base}/")) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    let mut file = match state.storage.open(&clean_path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    let meta = match file.metadata().await {
        Ok(m) => m,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read file metadata",
            )
                .into_response();
        }
    };

    if meta.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot download directory directly",
        )
            .into_response();
    }

    let file_size = meta.len();
    let content_type = mime_guess::from_path(&clean_path)
        .first_or_octet_stream()
        .to_string();

    let filename = Path::new(&clean_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");

    let disposition = safe_content_disposition(filename, as_attachment);

    // Range header handling
    let range_header = headers.get("range").and_then(|r| r.to_str().ok());

    if let Some(range_val) = range_header
        && let Some((start, end)) = parse_range(range_val, file_size)
    {
        let part_len = end - start + 1;
        if file.seek(SeekFrom::Start(start)).await.is_err() {
            return (StatusCode::RANGE_NOT_SATISFIABLE, "Range Not Satisfiable").into_response();
        }

        let stream = ReaderStream::new(file.take(part_len));
        let body = Body::from_stream(stream);

        let mut resp = (StatusCode::PARTIAL_CONTENT, body).into_response();
        let h = resp.headers_mut();
        h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        h.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(&content_type)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );
        h.insert(
            CONTENT_LENGTH,
            HeaderValue::from_str(&part_len.to_string()).unwrap_or(HeaderValue::from_static("0")),
        );
        h.insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size)).unwrap(),
        );
        h.insert(CONTENT_DISPOSITION, disposition.clone());
        return resp;
    }

    // Full response
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut resp = (StatusCode::OK, body).into_response();
    let h = resp.headers_mut();
    h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&file_size.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );
    h.insert(CONTENT_DISPOSITION, disposition);
    resp
}

fn safe_content_disposition(filename: &str, as_attachment: bool) -> HeaderValue {
    let disp_type = if as_attachment {
        "attachment"
    } else {
        "inline"
    };
    let encoded = encode_url_path(filename);
    let ascii_fallback: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let header_str =
        format!("{disp_type}; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}");
    HeaderValue::from_str(&header_str)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"file\""))
}

fn parse_range(range: &str, total: u64) -> Option<(u64, u64)> {
    let bytes_prefix = "bytes=";
    if !range.starts_with(bytes_prefix) {
        return None;
    }
    let s = &range[bytes_prefix.len()..];
    let mut parts = s.split('-');
    let start_str = parts.next()?.trim();
    let end_str = parts.next()?.trim();

    if start_str.is_empty() {
        // Suffix range: -N means last N bytes
        let len: u64 = end_str.parse().ok()?;
        let start = total.saturating_sub(len);
        Some((start, total.saturating_sub(1)))
    } else {
        let start: u64 = start_str.parse().ok()?;
        let end = if end_str.is_empty() {
            total.saturating_sub(1)
        } else {
            end_str.parse().ok()?
        };
        if start <= end && start < total {
            Some((start, end.min(total - 1)))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Admin User Management Handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct IdQuery {
    id: Option<i64>,
}

async fn admin_user_list_handler(headers: HeaderMap, State(state): State<SharedState>) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };
    if !user.is_admin() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(403, "Permission denied")),
        )
            .into_response();
    }

    let users = match crate::db::get_all_users(&state.pool).await {
        Ok(u) => u,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };
    let storages = crate::db::get_storages(&state.pool)
        .await
        .unwrap_or_default();

    let content: Vec<UserWithMount> = users
        .into_iter()
        .map(|u| {
            let local_path = crate::db::compute_local_path(&u.base_path, &storages);
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
    Json(ApiResponse::success(serde_json::json!({
        "content": content,
        "total": total
    })))
    .into_response()
}

async fn admin_user_get_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };
    if !user.is_admin() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(403, "Permission denied")),
        )
            .into_response();
    }

    let id = match query.id {
        Some(id) => id,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "missing id")),
            )
                .into_response();
        }
    };

    let target_user = match crate::db::get_user_by_id(&state.pool, id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(404, "user not found")),
            )
                .into_response();
        }
        Err(err) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response();
        }
    };

    let storages = crate::db::get_storages(&state.pool)
        .await
        .unwrap_or_default();
    let local_path = crate::db::compute_local_path(&target_user.base_path, &storages);

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

    Json(ApiResponse::success(res)).into_response()
}

async fn admin_user_create_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };
    if !user.is_admin() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(403, "Permission denied")),
        )
            .into_response();
    }

    let raw_pwd = req.password.as_deref().unwrap_or("").trim();
    if raw_pwd.is_empty() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "password is required")),
        )
            .into_response();
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
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
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
    .bind(req.role.unwrap_or(0))
    .bind(if req.disabled.unwrap_or(false) { 1 } else { 0 })
    .bind(req.permission.unwrap_or(0))
    .execute(&mut *tx)
    .await;

    let new_id = match res {
        Ok(r) => r.last_insert_rowid(),
        Err(err) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, err.to_string())),
            )
                .into_response();
        }
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
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
        }

        if let Err(e) = sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `order`, `driver`, `addition`, `status`, `disabled`) VALUES (?, 0, 'Local', ?, 'work', 0)"
        )
        .bind(&mount_path)
        .bind(&addition)
        .execute(&mut *tx)
        .await
        {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
    }

    let _ = state.storage.reload_from_db(&state.pool).await;

    Json(ApiResponse::success(())).into_response()
}

async fn admin_user_update_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<AdminUserSaveReq>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };
    if !user.is_admin() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(403, "Permission denied")),
        )
            .into_response();
    }

    let target_id = match req.id {
        Some(id) => id,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "missing id")),
            )
                .into_response();
        }
    };

    let mut target_user = match crate::db::get_user_by_id(&state.pool, target_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(404, "user not found")),
            )
                .into_response();
        }
    };

    if target_user.is_admin() && req.disabled.unwrap_or(false) {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(
                400,
                "admin user can not be disabled",
            )),
        )
            .into_response();
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

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
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
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
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
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error(500, e.to_string())),
                )
                    .into_response();
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
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error(500, e.to_string())),
                )
                    .into_response();
            }

            if let Err(e) = sqlx::query("UPDATE `x_users` SET `base_path` = ? WHERE `id` = ?")
                .bind(&user_mount)
                .bind(target_id)
                .execute(&mut *tx)
                .await
            {
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error(500, e.to_string())),
                )
                    .into_response();
            }
        }
    }

    if let Err(e) = tx.commit().await {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
    }

    let _ = state.storage.reload_from_db(&state.pool).await;

    Json(ApiResponse::success(())).into_response()
}

async fn admin_user_delete_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };
    if !user.is_admin() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(403, "Permission denied")),
        )
            .into_response();
    }

    let id = match query.id {
        Some(id) => id,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "missing id")),
            )
                .into_response();
        }
    };

    if id == 1 {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "cannot delete initial admin")),
        )
            .into_response();
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
    };

    let target_user = match sqlx::query_as::<_, User>("SELECT * FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(404, "user not found")),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
    };

    if target_user.is_admin() {
        let admin_count: i64 =
            match sqlx::query_scalar("SELECT count(*) FROM `x_users` WHERE `role` = ?")
                .bind(crate::model::ROLE_ADMIN)
                .fetch_one(&mut *tx)
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    return (
                        StatusCode::OK,
                        Json(ApiResponse::<()>::error(500, e.to_string())),
                    )
                        .into_response();
                }
            };
        if admin_count <= 1 {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(
                    400,
                    "cannot delete last admin user",
                )),
            )
                .into_response();
        }
    }

    if let Err(e) = sqlx::query("DELETE FROM `x_otp_pending` WHERE `user_id` = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
    }

    if let Err(e) = sqlx::query("DELETE FROM `x_users` WHERE `id` = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
    }

    let user_mount = format!("/.users/{}", id);
    if let Err(e) = sqlx::query("DELETE FROM `x_storages` WHERE `mount_path` = ?")
        .bind(&user_mount)
        .execute(&mut *tx)
        .await
    {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(500, e.to_string())),
        )
            .into_response();
    }

    let _ = state.storage.reload_from_db(&state.pool).await;

    Json(ApiResponse::success(())).into_response()
}

async fn admin_user_cancel_2fa_handler(
    headers: HeaderMap,
    Query(query): Query<IdQuery>,
    State(state): State<SharedState>,
) -> Response {
    let user = match authenticate_user(&headers, &state).await {
        Some(u) => u,
        None => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(401, "Authentication required")),
            )
                .into_response();
        }
    };
    if !user.is_admin() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(403, "Permission denied")),
        )
            .into_response();
    }

    if let Some(id) = query.id {
        let _ = sqlx::query("UPDATE `x_users` SET `otp_secret` = '' WHERE `id` = ?")
            .bind(id)
            .execute(&state.pool)
            .await;
    }
    Json(ApiResponse::success(())).into_response()
}

// ---------------------------------------------------------------------------
// File System Batch & Link Handlers
// ---------------------------------------------------------------------------

async fn fs_batch_rename_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<BatchRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(401, "Authentication required")),
        )
            .into_response();
    };
    if !permitted(&user, 4)
        || req
            .rename_objects
            .iter()
            .any(|item| !valid_name(&item.src_name) || !valid_name(&item.new_name))
    {
        return permission_denied();
    }
    let src_dir = match user_path(&user, &req.src_dir) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };

    for item in req.rename_objects {
        let src_path = format!("{}/{}", src_dir.trim_end_matches('/'), item.src_name);
        if let Err(err) = state.storage.rename(&src_path, &item.new_name).await {
            return Json(ApiResponse::<()>::error(500, err.to_string())).into_response();
        }
    }
    Json(ApiResponse::success(())).into_response()
}

async fn fs_link_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsLinkReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(401, "Authentication required")),
        )
            .into_response();
    };

    let token = get_setting(&state.pool, "token")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let clean_path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let sign = sign_path(&token, &clean_path);
    let url = format!("/d{}?sign={}", encode_url_path(&clean_path), sign);
    Json(ApiResponse::success(FsLinkResp { url })).into_response()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutting down gracefully...");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_paths_stay_under_base_and_permissions_are_enforced() {
        let user = User {
            id: 2,
            username: "alice".into(),
            pwd_hash: String::new(),
            pwd_ts: 0,
            salt: String::new(),
            password: None,
            base_path: "/.users/2".into(),
            role: 0,
            disabled: false,
            permission: 0,
            otp_secret: None,
            sso_id: None,
            otp: false,
        };
        assert_eq!(
            user_path(&user, "/secret.txt").unwrap(),
            "/.users/2/secret.txt"
        );
        assert_eq!(user_path(&user, "/").unwrap(), "/.users/2");
        assert!(user_path(&user, "../Local/secret.txt").is_err());
        assert!(user_path(&user, "/foo/../secret.txt").is_err());
        assert!(user_path(&user, "/foo/./secret.txt").is_err());
        assert!(!permitted(&user, 3));
        assert!(!permitted(&user, 7));
        assert!(!valid_name(""));
        assert!(!valid_name("../Local"));
        assert_eq!(encode_url_path("/a/hello #%.txt"), "/a/hello%20%23%25.txt");
    }

    #[tokio::test]
    async fn test_2fa_lifecycle_and_login_enforcement() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        let config = Config::default();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // 1. Get initial admin user and admin token
        crate::db::set_admin_password(&pool, "TestPass123!")
            .await
            .unwrap();
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(!admin.otp);

        let admin_token = crate::db::get_setting(&pool, "token")
            .await
            .unwrap()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // 2. Request 2FA generation with wrong password -> should fail
        let bad_gen_req = TwoFaGenerateReq {
            current_password: "WrongPassword!".to_string(),
        };
        let resp =
            two_factor_generate_handler(headers.clone(), State(state.clone()), Json(bad_gen_req))
                .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 403);

        // 2b. Request 2FA generation with correct password -> succeeds
        let gen_req = TwoFaGenerateReq {
            current_password: "TestPass123!".to_string(),
        };
        let resp =
            two_factor_generate_handler(headers.clone(), State(state.clone()), Json(gen_req)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let secret = json["data"]["secret"].as_str().unwrap().to_string();
        let qr = json["data"]["qr"].as_str().unwrap().to_string();
        assert!(qr.starts_with("data:image/svg+xml;base64,"));

        // 3. Verify with invalid code -> should fail
        let verify_req = TwoFaVerifyReq {
            code: "999999".to_string(),
        };
        let resp =
            two_factor_verify_handler(headers.clone(), State(state.clone()), Json(verify_req))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 4. Verify with valid TOTP code -> should succeed
        let now_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;
        let valid_code = crate::auth::compute_totp(&secret, now_step).unwrap();
        let verify_req = TwoFaVerifyReq { code: valid_code };
        let resp =
            two_factor_verify_handler(headers.clone(), State(state.clone()), Json(verify_req))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // 5. Check user in DB now has otp = true
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(admin.otp);

        // 6. Generating again should fail as 2FA is already enabled
        let gen_again = TwoFaGenerateReq {
            current_password: "TestPass123!".to_string(),
        };
        let resp =
            two_factor_generate_handler(headers.clone(), State(state.clone()), Json(gen_again))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 6b. Login with wrong password -> should return code 400
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "WrongPassword!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 7. Login without OTP code -> should return code 402 (OTP required)
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 402);

        // 8. Login with invalid OTP code -> should return code 400
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: Some("000000".to_string()),
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 9. Login with valid OTP code -> should succeed (code 200)
        let cur_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;
        let valid_code = crate::auth::compute_totp(&secret, cur_step).unwrap();
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: Some(valid_code),
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(json["data"]["token"].is_string());

        // 10. Admin cancel 2FA
        let cancel_query = IdQuery { id: Some(admin.id) };
        let resp = admin_user_cancel_2fa_handler(
            headers.clone(),
            Query(cancel_query),
            State(state.clone()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify 2FA is now cancelled
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(!admin.otp);

        // Login without OTP code succeeds again
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
    }

    #[tokio::test]
    async fn client_cannot_choose_otp_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "TestPass123!")
            .await
            .unwrap();
        let config = Config::default();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        let admin_token = crate::db::get_setting(&pool, "token")
            .await
            .unwrap()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // Attacker has their own secret A
        let secret_a = crate::auth::generate_otp_secret();

        // Server generates secret B and saves to x_otp_pending
        let gen_req = TwoFaGenerateReq {
            current_password: "TestPass123!".to_string(),
        };
        let resp =
            two_factor_generate_handler(headers.clone(), State(state.clone()), Json(gen_req)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let secret_b = json["data"]["secret"].as_str().unwrap().to_string();
        assert_ne!(secret_a, secret_b);

        let now_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;

        // Attacker computes code from their chosen secret A and calls verify -> MUST FAIL
        let code_a = crate::auth::compute_totp(&secret_a, now_step).unwrap();
        let verify_req_a = TwoFaVerifyReq { code: code_a };
        let resp =
            two_factor_verify_handler(headers.clone(), State(state.clone()), Json(verify_req_a))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // Verification with valid code from secret B succeeds
        let code_b = crate::auth::compute_totp(&secret_b, now_step).unwrap();
        let verify_req_b = TwoFaVerifyReq { code: code_b };
        let resp =
            two_factor_verify_handler(headers.clone(), State(state.clone()), Json(verify_req_b))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert_eq!(admin.otp_secret.as_deref(), Some(secret_b.as_str()));
    }

    #[tokio::test]
    async fn test_sensitive_account_changes_require_current_password() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "OldPassword123!")
            .await
            .unwrap();
        let config = Config::default();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // Login to get valid JWT token
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "OldPassword123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let token = json["data"]["token"].as_str().unwrap().to_string();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );

        // 1. JWT + 无 current_password 修改用户名 → 失败 (400)
        let req1 = UpdateCurrentReq {
            username: Some("newadmin".to_string()),
            password: None,
            current_password: None,
        };
        let resp = update_current_handler(headers.clone(), State(state.clone()), Json(req1)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 2. JWT + 错 current_password 修改用户名 → 失败 (403)
        let req2 = UpdateCurrentReq {
            username: Some("newadmin".to_string()),
            password: None,
            current_password: Some("WrongPass123!".to_string()),
        };
        let resp = update_current_handler(headers.clone(), State(state.clone()), Json(req2)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 403);

        // 3. JWT + 正确 current_password 修改用户名 → 成功 (200)
        let req3 = UpdateCurrentReq {
            username: Some("newadmin".to_string()),
            password: None,
            current_password: Some("OldPassword123!".to_string()),
        };
        let resp = update_current_handler(headers.clone(), State(state.clone()), Json(req3)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // Verify username updated in DB
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert_eq!(admin.username, "newadmin");

        // Obtain new token for newadmin
        let login_req = LoginReq {
            username: "newadmin".to_string(),
            password: "OldPassword123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let new_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut new_headers = HeaderMap::new();
        new_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", new_token)).unwrap(),
        );

        // 4. JWT + 正确旧密码修改密码 → 成功 (200)
        let req4 = UpdateCurrentReq {
            username: None,
            password: Some("NewPassword123!".to_string()),
            current_password: Some("OldPassword123!".to_string()),
        };
        let resp =
            update_current_handler(new_headers.clone(), State(state.clone()), Json(req4)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // 5. 修改密码后旧 JWT → 无效 (authenticate_user returns None)
        assert!(authenticate_user(&new_headers, &state).await.is_none());

        // Login with new password succeeds
        let login_req = LoginReq {
            username: "newadmin".to_string(),
            password: "NewPassword123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
    }

    #[tokio::test]
    async fn test_fs_conflict_policies_and_recursive_move_hierarchy() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        // Create storage directory structure
        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

        // Insert storage record in DB
        sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `driver`, `addition`) VALUES (?, ?, ?)",
        )
        .bind("/local")
        .bind("Local")
        .bind(
            serde_json::json!({
                "root_folder_path": storage_root.to_str().unwrap()
            })
            .to_string(),
        )
        .execute(&pool)
        .await
        .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let config = Config::default();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // Get admin token
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token = json["data"]["token"].as_str().unwrap().to_string();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );

        // Setup directories:
        // /local/src/sub/file1.txt ("file1 v1")
        // /local/src/sub/file2.txt ("file2 v1")
        // /local/src/root.txt ("root v1")
        // /local/dst/sub/file1.txt ("existing dst file1")
        let src_dir = storage_root.join("src/sub");
        tokio::fs::create_dir_all(&src_dir).await.unwrap();
        tokio::fs::write(src_dir.join("file1.txt"), b"file1 v1")
            .await
            .unwrap();
        tokio::fs::write(src_dir.join("file2.txt"), b"file2 v1")
            .await
            .unwrap();
        tokio::fs::write(storage_root.join("src/root.txt"), b"root v1")
            .await
            .unwrap();

        let dst_sub = storage_root.join("dst/sub");
        tokio::fs::create_dir_all(&dst_sub).await.unwrap();
        tokio::fs::write(dst_sub.join("file1.txt"), b"existing dst file1")
            .await
            .unwrap();

        // 1. Test recursive_move with ConflictPolicy::Cancel
        // Because /local/dst/sub/file1.txt already exists, it must return 403, and move NO files!
        let req_cancel = FsRecursiveMoveReq {
            src_dir: "/local/src".to_string(),
            dst_dir: "/local/dst".to_string(),
            conflict_policy: ConflictPolicy::Cancel,
        };
        let resp =
            fs_recursive_move_handler(State(state.clone()), headers.clone(), Json(req_cancel))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 403);

        // Verify NOTHING was moved
        assert!(src_dir.join("file1.txt").exists());
        assert!(src_dir.join("file2.txt").exists());
        assert!(storage_root.join("src/root.txt").exists());
        assert_eq!(
            tokio::fs::read(dst_sub.join("file1.txt")).await.unwrap(),
            b"existing dst file1"
        );
        assert!(!dst_sub.join("file2.txt").exists());
        assert!(!storage_root.join("dst/root.txt").exists());

        // 2. Test recursive_move with ConflictPolicy::Skip
        // Skips file1.txt, moves file2.txt and root.txt, PRESERVING nested sub/ directory structure!
        let req_skip = FsRecursiveMoveReq {
            src_dir: "/local/src".to_string(),
            dst_dir: "/local/dst".to_string(),
            conflict_policy: ConflictPolicy::Skip,
        };
        let resp =
            fs_recursive_move_handler(State(state.clone()), headers.clone(), Json(req_skip)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // Existing dst file1 was kept intact
        assert_eq!(
            tokio::fs::read(dst_sub.join("file1.txt")).await.unwrap(),
            b"existing dst file1"
        );
        // Skipped source file1 remains in src
        assert!(src_dir.join("file1.txt").exists());
        // Moved files are now in dst with preserved hierarchy
        assert!(!src_dir.join("file2.txt").exists());
        assert_eq!(
            tokio::fs::read(dst_sub.join("file2.txt")).await.unwrap(),
            b"file2 v1"
        );
        assert!(!storage_root.join("src/root.txt").exists());
        assert_eq!(
            tokio::fs::read(storage_root.join("dst/root.txt"))
                .await
                .unwrap(),
            b"root v1"
        );

        // 3. Test recursive_move with ConflictPolicy::Overwrite
        // Overwrites dst/sub/file1.txt with src/sub/file1.txt
        let req_overwrite = FsRecursiveMoveReq {
            src_dir: "/local/src".to_string(),
            dst_dir: "/local/dst".to_string(),
            conflict_policy: ConflictPolicy::Overwrite,
        };
        let resp =
            fs_recursive_move_handler(State(state.clone()), headers.clone(), Json(req_overwrite))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // dst/sub/file1.txt is now overwritten with file1 v1
        assert_eq!(
            tokio::fs::read(dst_sub.join("file1.txt")).await.unwrap(),
            b"file1 v1"
        );
        assert!(!src_dir.join("file1.txt").exists());
    }

    #[tokio::test]
    async fn test_user_administration_transactional_and_last_admin_protection() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let config = Config::default();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // Get admin token
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token = json["data"]["token"].as_str().unwrap().to_string();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );

        // 1. 普通用户创建：user + permissions + base_path 均成功
        let user_home = tmp.path().join("user_home");
        tokio::fs::create_dir_all(&user_home).await.unwrap();
        let create_req = AdminUserSaveReq {
            id: None,
            username: "regular_user".to_string(),
            password: Some("UserPass123!".to_string()),
            role: Some(0),
            permission: Some(255),
            disabled: Some(false),
            local_path: Some(user_home.to_str().unwrap().to_string()),
        };
        let resp =
            admin_user_create_handler(headers.clone(), State(state.clone()), Json(create_req))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        let created_user = crate::db::get_user_by_name(&pool, "regular_user")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(created_user.permission, 255);
        assert_eq!(created_user.role, 0);
        assert_eq!(
            created_user.base_path,
            format!("/.users/{}", created_user.id)
        );
        let storage_row: Option<i64> =
            sqlx::query_scalar("SELECT id FROM x_storages WHERE mount_path = ?")
                .bind(&created_user.base_path)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(storage_row.is_some());

        // 2. 中途失败：事务回滚，无孤立 user 记录
        // Trying to create a user with duplicate username
        let dup_req = AdminUserSaveReq {
            id: None,
            username: "regular_user".to_string(),
            password: Some("AnotherPass123!".to_string()),
            role: Some(0),
            permission: Some(10),
            disabled: Some(false),
            local_path: None,
        };
        let resp =
            admin_user_create_handler(headers.clone(), State(state.clone()), Json(dup_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 3. 最后一个 admin 删除保护：必须返回错误，不能删空管理员
        // Trying to delete admin (id=1)
        let resp = admin_user_delete_handler(
            headers.clone(),
            Query(IdQuery { id: Some(1) }),
            State(state.clone()),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // Create a second admin user
        let second_admin_req = AdminUserSaveReq {
            id: None,
            username: "second_admin".to_string(),
            password: Some("Admin2Pass123!".to_string()),
            role: Some(crate::model::ROLE_ADMIN),
            permission: Some(0),
            disabled: Some(false),
            local_path: None,
        };
        let resp = admin_user_create_handler(
            headers.clone(),
            State(state.clone()),
            Json(second_admin_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        let second_admin = crate::db::get_user_by_name(&pool, "second_admin")
            .await
            .unwrap()
            .unwrap();
        assert!(second_admin.is_admin());

        // Deleting second admin succeeds because admin (id=1) still exists
        let resp = admin_user_delete_handler(
            headers.clone(),
            Query(IdQuery {
                id: Some(second_admin.id),
            }),
            State(state.clone()),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(
            crate::db::get_user_by_id(&pool, second_admin.id)
                .await
                .unwrap()
                .is_none()
        );

        // Now delete regular_user and verify user + storage cleanup
        let resp = admin_user_delete_handler(
            headers.clone(),
            Query(IdQuery {
                id: Some(created_user.id),
            }),
            State(state.clone()),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(
            crate::db::get_user_by_id(&pool, created_user.id)
                .await
                .unwrap()
                .is_none()
        );
        let storage_row_after: Option<i64> =
            sqlx::query_scalar("SELECT id FROM x_storages WHERE mount_path = ?")
                .bind(&created_user.base_path)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(storage_row_after.is_none());
    }
}
