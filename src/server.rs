use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::header::{
    ACCEPT_RANGES, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post, put};
use axum::Router;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::auth::{generate_jwt, parse_jwt, verify_password, verify_password_static_hash};
use crate::config::Config;
use crate::db::{get_admin, get_public_settings, get_setting, get_user_by_name, DbPool};
use crate::driver::{SharedStorageManager, StorageManager};
use crate::model::{
    sort_files, ApiResponse, DirItem, FsDirNamesReq, FsDirsReq, FsGetReq, FsListReq,
    FsListResp, FsMoveCopyReq, FsRenameReq, LoginReq, UpdateCurrentReq, User,
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
        .route("/tinylist.svg", get(crate::static_files::dist_assets_handler))
        .route("/tinylist.png", get(crate::static_files::dist_assets_handler))
        .route("/rulist.svg", get(crate::static_files::dist_assets_handler))
        .route("/rulist.png", get(crate::static_files::dist_assets_handler))
        // Static assets from frontend dist
        .route("/assets/{*path}", get(crate::static_files::dist_assets_handler))
        .route("/static/{*path}", get(crate::static_files::dist_assets_handler))
        .route("/streamer/{*path}", get(crate::static_files::dist_assets_handler))
        // Settings
        .route("/api/public/settings", get(public_settings_handler).post(public_settings_handler))
        // Authentication
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/login/hash", post(login_hash_handler))
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/me", get(current_user_handler))
        .route("/api/me/update", post(update_current_handler))
        .route("/api/auth/logout", get(logout_handler).post(logout_handler))
        // File system read
        .route("/api/fs/list", post(fs_list_handler).get(fs_list_handler))
        .route("/api/fs/get", post(fs_get_handler).get(fs_get_handler))
        .route("/api/fs/dirs", post(fs_dirs_handler).get(fs_dirs_handler))
        // File system write
        .route("/api/fs/mkdir", post(fs_mkdir_handler))
        .route("/api/fs/rename", post(fs_rename_handler))
        .route("/api/fs/move", post(fs_move_handler))
        .route("/api/fs/recursive_move", post(fs_move_handler))
        .route("/api/fs/copy", post(fs_copy_handler))
        .route("/api/fs/remove", post(fs_remove_handler))
        .route("/api/fs/remove_empty_directory", post(fs_remove_handler))
        .route("/api/fs/put", put(fs_put_handler))
        // Direct download & streaming
        .route("/d/{*path}", get(raw_download_handler).head(raw_download_handler))
        .route("/p/{*path}", get(raw_preview_handler).head(raw_preview_handler))
        // SPA Fallback for all other routes
        .fallback(crate::static_files::spa_fallback_handler)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.scheme.address, config.scheme.http_port).parse()?;
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

async fn login_handler(
    State(state): State<SharedState>,
    Json(req): Json<LoginReq>,
) -> Response {
    let user = match get_user_by_name(&state.pool, &req.username).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "user not found")),
            )
                .into_response()
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response()
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
            Json(ApiResponse::<()>::error(400, "invalid username or password")),
        )
            .into_response();
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

async fn login_hash_handler(
    State(state): State<SharedState>,
    Json(req): Json<LoginReq>,
) -> Response {
    let user = match get_user_by_name(&state.pool, &req.username).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(ApiResponse::<()>::error(400, "user not found")),
            )
                .into_response()
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(500, err.to_string())),
            )
                .into_response()
        }
    };

    if user.disabled {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "user is disabled")),
        )
            .into_response();
    }

    if !verify_password_static_hash(&req.password, &user.pwd_hash, &user.salt) {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(400, "invalid username or password")),
        )
            .into_response();
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

    if let Some(new_pwd) = &req.password {
        if !new_pwd.is_empty() {
            let cur_pwd = req.current_password.as_deref().unwrap_or("");
            if cur_pwd.is_empty() {
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error(400, "Current password is required")),
                )
                    .into_response();
            }
            if !verify_password(cur_pwd, &user.pwd_hash, &user.salt) {
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<()>::error(403, "Current password is incorrect")),
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
                "UPDATE `x_users` SET `pwd_hash` = ?, `salt` = ?, `pwd_ts` = ? WHERE `id` = ?"
            )
            .bind(&encoded_pwd)
            .bind(&salt)
            .bind(now_ts)
            .bind(user.id)
            .execute(&state.pool)
            .await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(500, e.to_string())),
                )
                    .into_response();
            }
            user.pwd_ts = now_ts;
        }
    }

    if let Some(new_name) = &req.username {
        if !new_name.is_empty() && new_name != &user.username {
            if let Err(e) = sqlx::query("UPDATE `x_users` SET `username` = ? WHERE `id` = ?")
                .bind(new_name)
                .bind(user.id)
                .execute(&state.pool)
                .await {
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
    if let Ok(Some(admin_token)) = get_setting(&state.pool, "token").await {
        if !admin_token.is_empty() && admin_token == token {
            return get_admin(&state.pool).await.ok().flatten();
        }
    }

    // Parse JWT
    let claims = parse_jwt(token, &state.config.jwt_secret).ok()?;
    let user = get_user_by_name(&state.pool, &claims.username).await.ok().flatten()?;

    if user.disabled || user.pwd_ts != claims.pwd_ts {
        return None;
    }

    Some(user)
}

async fn current_user_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Response {
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
    if authenticate_user(&headers, &state).await.is_none() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(401, "Authentication required")),
        )
            .into_response();
    }

    match state.storage.list(&req.path).await {
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
                    let item_path = format!("{}/{}", req.path.trim_end_matches('/'), item.name);
                    let sign = sign_path(&token, &item_path);
                    item.sign = sign.clone();
                    item.raw_url = format!("/p{}?sign={}", item_path, sign);
                }
            }

            let total = content.len() as i64;
            let resp = FsListResp {
                content,
                total,
                readme: String::new(),
                header: String::new(),
                write: true,
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
    if authenticate_user(&headers, &state).await.is_none() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(401, "Authentication required")),
        )
            .into_response();
    }

    match state.storage.get(&req.path).await {
        Ok(mut file) => {
            let token = get_setting(&state.pool, "token")
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

            if !file.is_dir {
                let s = sign_path(&token, &req.path);
                file.sign = s.clone();
                file.raw_url = format!("/p{}?sign={}", req.path, s);
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
    if authenticate_user(&headers, &state).await.is_none() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<()>::error(401, "Authentication required")),
        )
            .into_response();
    }

    let path = if req.path.is_empty() { "/" } else { &req.path };
    let files = match state.storage.list(path).await {
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
    if authenticate_user(&headers, &state).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::error(401, "unauthorized"))).into_response();
    }

    let path = req.get("path").and_then(|v| v.as_str()).unwrap_or("");
    match state.storage.mkdir(path).await {
        Ok(_) => Json(ApiResponse::success(serde_json::Value::Null)).into_response(),
        Err(err) => (StatusCode::OK, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response(),
    }
}

async fn fs_rename_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRenameReq>,
) -> Response {
    if authenticate_user(&headers, &state).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::error(401, "unauthorized"))).into_response();
    }

    match state.storage.rename(&req.path, &req.name).await {
        Ok(_) => Json(ApiResponse::success(serde_json::Value::Null)).into_response(),
        Err(err) => (StatusCode::OK, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response(),
    }
}

async fn fs_move_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    if authenticate_user(&headers, &state).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::error(401, "unauthorized"))).into_response();
    }

    for name in req.names {
        let src = format!("{}/{}", req.src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", req.dst_dir.trim_end_matches('/'), name);
        if let Err(err) = state.storage.move_to(&src, &dst).await {
            return (StatusCode::OK, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response();
        }
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

async fn fs_copy_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    if authenticate_user(&headers, &state).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::error(401, "unauthorized"))).into_response();
    }

    for name in req.names {
        let src = format!("{}/{}", req.src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", req.dst_dir.trim_end_matches('/'), name);
        if let Err(err) = state.storage.copy_to(&src, &dst).await {
            return (StatusCode::OK, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response();
        }
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

async fn fs_remove_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsDirNamesReq>,
) -> Response {
    if authenticate_user(&headers, &state).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::error(401, "unauthorized"))).into_response();
    }

    for name in req.names {
        let target = format!("{}/{}", req.dir.trim_end_matches('/'), name);
        if let Err(err) = state.storage.remove(&target).await {
            return (StatusCode::OK, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response();
        }
    }

    Json(ApiResponse::success(serde_json::Value::Null)).into_response()
}

async fn fs_put_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    if authenticate_user(&headers, &state).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::error(401, "unauthorized"))).into_response();
    }

    let file_path = match headers.get("File-Path").and_then(|h| h.to_str().ok()) {
        Some(p) => p.to_string(),
        None => return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::error(400, "missing File-Path header"))).into_response(),
    };

    let body = request.into_body();
    // Stream body to file
    let (ms, sub) = match state.storage.find_storage(&file_path) {
        Some(m) => m,
        None => return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::error(404, "storage not found"))).into_response(),
    };

    let target = match ms.driver.safe_resolve(&sub) {
        Ok(t) => t,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::error(400, err.to_string()))).into_response(),
    };

    if let Some(parent) = target.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let mut file = match tokio::fs::File::create(&target).await {
        Ok(f) => f,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response(),
    };

    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                if let Err(err) = file.write_all(&bytes).await {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::error(500, err.to_string()))).into_response();
                }
            }
            Err(err) => return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::error(400, err.to_string()))).into_response(),
        }
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
    let mut chars = s.as_bytes().iter();
    while let Some(&b) = chars.next() {
        if b == b'%' {
            if let (Some(&h1), Some(&h2)) = (chars.next(), chars.next()) {
                let hex_str = [h1, h2];
                if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&hex_str).unwrap_or(""), 16) {
                    bytes.push(byte);
                    continue;
                }
            }
        }
        bytes.push(b);
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
    let decoded = percent_decode(&raw_path);
    let clean_path = format!("/{}", decoded.trim_start_matches('/'));

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
        if let Err(_) = verify_sign(&token, &clean_path, &s) {
            return (StatusCode::FORBIDDEN, "Invalid or expired download link signature").into_response();
        }
    }

    let mut file = match state.storage.open(&clean_path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    let meta = match file.metadata().await {
        Ok(m) => m,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file metadata").into_response(),
    };

    if meta.is_dir() {
        return (StatusCode::BAD_REQUEST, "Cannot download directory directly").into_response();
    }

    let file_size = meta.len();
    let content_type = mime_guess::from_path(&clean_path)
        .first_or_octet_stream()
        .to_string();

    let filename = Path::new(&clean_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");

    let disposition = if as_attachment {
        format!("attachment; filename=\"{}\"", filename)
    } else {
        format!("inline; filename=\"{}\"", filename)
    };

    // Range header handling
    let range_header = headers.get("range").and_then(|r| r.to_str().ok());

    if let Some(range_val) = range_header {
        if let Some((start, end)) = parse_range(range_val, file_size) {
            let part_len = end - start + 1;
            if file.seek(SeekFrom::Start(start)).await.is_err() {
                return (StatusCode::RANGE_NOT_SATISFIABLE, "Range Not Satisfiable").into_response();
            }

            let stream = ReaderStream::new(file.take(part_len));
            let body = Body::from_stream(stream);

            let mut resp = (StatusCode::PARTIAL_CONTENT, body).into_response();
            let h = resp.headers_mut();
            h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            h.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type).unwrap_or(HeaderValue::from_static("application/octet-stream")));
            h.insert(CONTENT_LENGTH, HeaderValue::from_str(&part_len.to_string()).unwrap());
            h.insert(
                CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size)).unwrap(),
            );
            h.insert(CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap());
            return resp;
        }
    }

    // Full response
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut resp = (StatusCode::OK, body).into_response();
    let h = resp.headers_mut();
    h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    h.insert(CONTENT_LENGTH, HeaderValue::from_str(&file_size.to_string()).unwrap());
    h.insert(CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap());
    resp
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
