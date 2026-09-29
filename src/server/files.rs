use axum::extract::{Json, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use tokio::io::AsyncWriteExt;

use crate::filesystem::local::{LocalFs, RenameError};
use crate::filesystem::{FileEntry, sort_files_by, sorted_file_page, valid_name};
use crate::server::stream::percent_decode;
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, encode_url_path, permission_denied,
    permitted, user_path,
};
use crate::sign::sign_path;

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct FsListReq {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default)]
    pub per_page: Option<usize>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub reverse: Option<bool>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsListResp {
    pub content: Vec<FileEntry>,
    pub total: i64,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct FsGetReq {
    #[serde(default)]
    pub path: String,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FsRenameReq {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub overwrite: bool,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct FsDirNamesReq {
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub names: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    #[default]
    Cancel,
    Overwrite,
    Skip,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct FsMoveCopyReq {
    #[serde(default)]
    pub src_dir: String,
    #[serde(default)]
    pub dst_dir: String,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct FsDirsReq {
    #[serde(default)]
    pub path: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirItem {
    pub name: String,
    pub modified: String,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BatchRenameItem {
    pub src_name: String,
    pub new_name: String,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BatchRenameReq {
    pub src_dir: String,
    pub rename_objects: Vec<BatchRenameItem>,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FsLinkReq {
    pub path: String,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct FsLinkResp {
    pub url: String,
}

fn user_fs(user: &crate::db::User) -> Result<LocalFs, anyhow::Error> {
    LocalFs::new(&user.local_path, false)
}

fn filesystem_error_details(
    err: &anyhow::Error,
    action: &'static str,
) -> (StatusCode, i32, &'static str) {
    let kind = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .map(std::io::Error::kind);
    let result = match kind {
        Some(std::io::ErrorKind::NotFound) => (StatusCode::NOT_FOUND, 404, "File not found"),
        Some(std::io::ErrorKind::PermissionDenied) => {
            (StatusCode::FORBIDDEN, 403, "Permission denied")
        }
        Some(std::io::ErrorKind::InvalidInput | std::io::ErrorKind::NotADirectory) => {
            (StatusCode::BAD_REQUEST, 400, "Invalid filesystem request")
        }
        Some(std::io::ErrorKind::AlreadyExists) => {
            (StatusCode::CONFLICT, 409, "File already exists")
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        ),
    };

    if result.0.is_server_error() {
        tracing::error!(error = %err, action, "filesystem operation failed");
    } else {
        tracing::debug!(error = %err, action, "filesystem operation failed");
    }

    result
}

fn filesystem_error_response(err: &anyhow::Error, action: &'static str) -> Response {
    let (status, code, message) = filesystem_error_details(err, action);
    api_error(status, code, message)
}

pub(crate) fn sign_context(user: &crate::db::User) -> String {
    format!(
        "uid={}:pwd_ts={}:root={}",
        user.id, user.pwd_ts, user.local_path
    )
}

pub async fn list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsListReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    match fs.list(&path).await {
        Ok(mut content) => {
            let total = content.len() as i64;

            if let Some(per_page) = req.per_page.filter(|&size| size > 0) {
                let page = req.page.unwrap_or(1).max(1);
                content = sorted_file_page(
                    &mut content,
                    req.order_by.as_deref(),
                    req.reverse.unwrap_or(false),
                    page,
                    per_page,
                );
            } else {
                sort_files_by(
                    &mut content,
                    req.order_by.as_deref(),
                    req.reverse.unwrap_or(false),
                );
            }

            // Attach signs and raw_urls to files
            for item in &mut content {
                if !item.is_dir {
                    let item_path = format!("{}/{}", path.trim_end_matches('/'), item.name);
                    let sign =
                        match sign_path(&state.config.jwt_secret, &item_path, &sign_context(&user))
                        {
                            Ok(sign) => sign,
                            Err(err) => {
                                tracing::error!(error = %err, "failed to sign file path");
                                return api_error(
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    500,
                                    "Signing token is unavailable",
                                );
                            }
                        };
                    item.sign = sign.clone();
                    item.raw_url = format!(
                        "/p{}?sign={}&uid={}",
                        encode_url_path(&item_path),
                        sign,
                        user.id
                    );
                }
            }

            let resp = FsListResp { content, total };
            api_success(resp)
        }
        Err(err) => {
            tracing::debug!(path = %path, "failed to list directory");
            filesystem_error_response(&err, "list")
        }
    }
}

pub async fn get_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsGetReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    match fs.get(&path).await {
        Ok(mut file) => {
            if !file.is_dir {
                let s = match sign_path(&state.config.jwt_secret, &path, &sign_context(&user)) {
                    Ok(sign) => sign,
                    Err(err) => {
                        tracing::error!(error = %err, "failed to sign file path");
                        return api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            500,
                            "Signing token is unavailable",
                        );
                    }
                };
                file.sign = s.clone();
                file.raw_url = format!("/p{}?sign={}&uid={}", encode_url_path(&path), s, user.id);
            }

            api_success(file)
        }
        Err(err) => {
            tracing::debug!(path = %path, "failed to get file");
            filesystem_error_response(&err, "get")
        }
    }
}

pub async fn dirs_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsDirsReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = user_path(&user, &req.path);
    let path = match path {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    let files = match fs.list(&path).await {
        Ok(f) => f,
        Err(err) => {
            tracing::debug!(path = %path, "failed to list dirs");
            return filesystem_error_response(&err, "list directories");
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

    api_success(dirs)
}

pub async fn mkdir_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<serde_json::Value>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
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
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    match fs.mkdir(&path).await {
        Ok(_) => api_success(serde_json::Value::Null),
        Err(err) => {
            tracing::debug!(path = %path, "failed to create directory");
            filesystem_error_response(&err, "mkdir")
        }
    }
}

pub async fn rename_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, 4) || (req.overwrite && !permitted(&user, 8)) || !valid_name(&req.name) {
        return permission_denied();
    }
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    match fs.rename_safe(&path, &req.name, req.overwrite).await {
        Ok(_) => api_success(serde_json::Value::Null),
        Err(RenameError::Conflict(msg)) => api_error(StatusCode::CONFLICT, 409, msg),
        Err(RenameError::NotFound(msg)) => api_error(StatusCode::NOT_FOUND, 404, msg),
        Err(RenameError::BadRequest(msg)) => api_error(StatusCode::BAD_REQUEST, 400, msg),
        Err(RenameError::Internal(err)) => {
            tracing::error!(error = %err, path = %path, name = %req.name, "failed to rename");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

fn validate_transfer_paths(src: &str, dst: &str) -> Result<(), &'static str> {
    let src = src.trim_end_matches('/');
    let dst = dst.trim_end_matches('/');

    if src == dst {
        return Err("source and destination are identical");
    }

    if dst.starts_with(&format!("{src}/")) {
        return Err("destination cannot be inside source");
    }

    Ok(())
}

pub async fn move_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, 5)
        || (req.conflict_policy == ConflictPolicy::Overwrite && !permitted(&user, 8))
        || req.names.iter().any(|name| !valid_name(name))
    {
        return permission_denied();
    }
    let (src_dir, dst_dir) = match (
        user_path(&user, &req.src_dir),
        user_path(&user, &req.dst_dir),
    ) {
        (Ok(src), Ok(dst)) => (src, dst),
        _ => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    let policy = req.conflict_policy;
    let mut moves = Vec::new();
    for name in &req.names {
        let src = format!("{}/{}", src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", dst_dir.trim_end_matches('/'), name);
        if let Err(msg) = validate_transfer_paths(&src, &dst) {
            return api_error(StatusCode::BAD_REQUEST, 400, msg);
        }
        let dst_exists = fs.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(StatusCode::CONFLICT, 409, format!("file [{name}] exists"));
                }
                ConflictPolicy::Skip => {
                    continue;
                }
                ConflictPolicy::Overwrite => {}
            }
        }
        moves.push((src, dst));
    }

    for (completed, (src, dst)) in moves.into_iter().enumerate() {
        let overwrite = policy == ConflictPolicy::Overwrite;
        if let Err(err) = fs.move_to_safe(&src, &dst, overwrite).await {
            let (status, code, _) = filesystem_error_details(&err, "move");
            tracing::debug!(src = %src, dst = %dst, "failed to move file");
            return api_error(
                status,
                code,
                format!("Move failed for {src}; {completed} item(s) already moved"),
            );
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn copy_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, 6)
        || (req.conflict_policy == ConflictPolicy::Overwrite && !permitted(&user, 8))
        || req.names.iter().any(|name| !valid_name(name))
    {
        return permission_denied();
    }
    let (src_dir, dst_dir) = match (
        user_path(&user, &req.src_dir),
        user_path(&user, &req.dst_dir),
    ) {
        (Ok(src), Ok(dst)) => (src, dst),
        _ => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    let policy = req.conflict_policy;
    let mut copies = Vec::new();
    for name in &req.names {
        let src = format!("{}/{}", src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", dst_dir.trim_end_matches('/'), name);
        if let Err(msg) = validate_transfer_paths(&src, &dst) {
            return api_error(StatusCode::BAD_REQUEST, 400, msg);
        }
        let dst_exists = fs.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(StatusCode::CONFLICT, 409, format!("file [{name}] exists"));
                }
                ConflictPolicy::Skip => {
                    continue;
                }
                ConflictPolicy::Overwrite => {}
            }
        }
        copies.push((src, dst));
    }

    for (completed, (src, dst)) in copies.into_iter().enumerate() {
        let overwrite = policy == ConflictPolicy::Overwrite;
        if let Err(err) = fs.copy_to_safe(&src, &dst, overwrite).await {
            let (status, code, _) = filesystem_error_details(&err, "copy");
            tracing::debug!(src = %src, dst = %dst, "failed to copy file");
            return api_error(
                status,
                code,
                format!("Copy failed for {src}; {completed} item(s) already copied"),
            );
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn remove_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsDirNamesReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, 7) || req.names.iter().any(|name| !valid_name(name)) {
        return permission_denied();
    }
    let dir = match user_path(&user, &req.dir) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    for (completed, name) in req.names.into_iter().enumerate() {
        let target = format!("{}/{}", dir.trim_end_matches('/'), name);
        if let Err(err) = fs.remove(&target).await {
            let (status, code, _) = filesystem_error_details(&err, "delete");
            tracing::debug!(target = %target, "failed to remove target");
            return api_error(
                status,
                code,
                format!("Delete failed for {target}; {completed} item(s) already deleted"),
            );
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn upload_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, 3) {
        return permission_denied();
    }

    let file_path = match headers.get("File-Path").and_then(|h| h.to_str().ok()) {
        Some(p) => percent_decode(p),
        None => return api_error(StatusCode::BAD_REQUEST, 400, "missing File-Path header"),
    };
    let file_path = match user_path(&user, &file_path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    let overwrite = headers.get("Overwrite").and_then(|h| h.to_str().ok()) == Some("true");
    if overwrite && !permitted(&user, 8) {
        return permission_denied();
    }

    let mut body = request.into_body();
    // Stream body to file
    let target = match fs.safe_resolve(&file_path) {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(error = %err, "safe_resolve failed in put");
            return api_error(StatusCode::BAD_REQUEST, 400, "Invalid file path");
        }
    };

    if file_path.trim_matches('/').is_empty() {
        return permission_denied();
    }
    if !overwrite && target.exists() {
        return api_error(StatusCode::CONFLICT, 409, "file already exists");
    }
    if let Some(parent) = target.parent()
        && let Err(err) = tokio::fs::create_dir_all(parent).await
    {
        tracing::error!(error = %err, "failed to create parent dir for upload");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    let temp = target.with_file_name(format!(".rulist-upload-{}", crate::auth::rand_string(24)));
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
    {
        Ok(f) => f,
        Err(err) => {
            tracing::error!(error = %err, "failed to create temp upload file");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
    };

    let max_upload_bytes: u64 = 100 * 1024 * 1024 * 1024; // 100 GB default safety limit
    let mut uploaded_bytes: u64 = 0;
    use axum::body::HttpBody;
    use std::future::poll_fn;
    use std::pin::Pin;

    while let Some(frame) = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
        match frame {
            Ok(frame) => {
                if let Ok(bytes) = frame.into_data() {
                    uploaded_bytes += bytes.len() as u64;
                    if uploaded_bytes > max_upload_bytes {
                        let _ = tokio::fs::remove_file(&temp).await;
                        return api_error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            413,
                            "payload too large: maximum upload size exceeded",
                        );
                    }
                    if let Err(err) = file.write_all(&bytes).await {
                        let _ = tokio::fs::remove_file(&temp).await;
                        tracing::error!(error = %err, "failed to write chunk to upload temp file");
                        return api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            500,
                            "Internal server error",
                        );
                    }
                }
            }
            Err(err) => {
                let _ = tokio::fs::remove_file(&temp).await;
                tracing::error!(error = %err, "failed to read chunk from upload stream");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        }
    }

    if let Err(err) = file.flush().await {
        let _ = tokio::fs::remove_file(&temp).await;
        tracing::error!(error = %err, "failed to flush upload temp file");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }
    drop(file);
    if !overwrite {
        if target.exists() {
            let _ = tokio::fs::remove_file(&temp).await;
            return api_error(StatusCode::CONFLICT, 409, "file already exists");
        }
        if let Err(err) = tokio::fs::hard_link(&temp, &target).await {
            let _ = tokio::fs::remove_file(&temp).await;
            if err.kind() == std::io::ErrorKind::AlreadyExists || target.exists() {
                return api_error(StatusCode::CONFLICT, 409, "file already exists");
            }
            tracing::error!(error = %err, "failed to finalize upload file via hard_link");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
        let _ = tokio::fs::remove_file(&temp).await;
    } else if let Err(err) = tokio::fs::rename(&temp, &target).await {
        let _ = tokio::fs::remove_file(&temp).await;
        tracing::error!(error = %err, "failed to finalize upload file");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Internal server error",
        );
    }

    api_success(serde_json::Value::Null)
}

pub async fn batch_rename_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<BatchRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    if !permitted(&user, 4) {
        return permission_denied();
    }
    let src_dir = match user_path(&user, &req.src_dir) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    let pairs: Vec<(String, String)> = req
        .rename_objects
        .into_iter()
        .map(|o| (o.src_name, o.new_name))
        .collect();

    match fs.batch_rename(&src_dir, &pairs).await {
        Ok(_) => api_success(()),
        Err(RenameError::Conflict(msg)) => api_error(StatusCode::CONFLICT, 409, msg),
        Err(RenameError::NotFound(msg)) => api_error(StatusCode::NOT_FOUND, 404, msg),
        Err(RenameError::BadRequest(msg)) => api_error(StatusCode::BAD_REQUEST, 400, msg),
        Err(RenameError::Internal(err)) => {
            tracing::error!(error = %err, src_dir = %src_dir, "batch rename failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn link_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsLinkReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };

    let clean_path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let _fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    let sign = match sign_path(&state.config.jwt_secret, &clean_path, &sign_context(&user)) {
        Ok(sign) => sign,
        Err(err) => {
            tracing::error!(error = %err, "failed to sign file path");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Signing token is unavailable",
            );
        }
    };
    let url = format!(
        "/d{}?sign={}&uid={}",
        encode_url_path(&clean_path),
        sign,
        user.id
    );
    api_success(FsLinkResp { url })
}
