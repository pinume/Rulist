use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::filesystem::local::RenameError;
use crate::filesystem::valid_name;
use crate::permissions::{COPY, DELETE, MOVE, OVERWRITE, RENAME, WRITE_CONTENT};
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, permission_denied, permitted, user_path,
};

use super::{filesystem_error_details, filesystem_error_response, user_fs};

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct FsRenameReq {
    path: String,
    name: String,
    #[serde(default)]
    overwrite: bool,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub(crate) struct FsDirNamesReq {
    #[serde(default)]
    dir: String,
    #[serde(default)]
    names: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ConflictPolicy {
    #[default]
    Cancel,
    Overwrite,
    Skip,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub(crate) struct FsMoveCopyReq {
    #[serde(default)]
    src_dir: String,
    #[serde(default)]
    dst_dir: String,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    conflict_policy: ConflictPolicy,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct FsMkdirReq {
    path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct BatchRenameItem {
    src_name: String,
    new_name: String,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct BatchRenameReq {
    src_dir: String,
    rename_objects: Vec<BatchRenameItem>,
}

pub(crate) async fn mkdir_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMkdirReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, WRITE_CONTENT) {
        return permission_denied();
    }
    if req.path.trim_matches('/').is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            400,
            "directory path cannot be empty",
        );
    }
    let path = match user_path(&user, &req.path) {
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

pub(crate) async fn rename_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, RENAME)
        || (req.overwrite && !permitted(&user, OVERWRITE))
        || !valid_name(&req.name)
    {
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

pub(crate) async fn move_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, MOVE)
        || (req.conflict_policy == ConflictPolicy::Overwrite && !permitted(&user, OVERWRITE))
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

pub(crate) async fn copy_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsMoveCopyReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, COPY)
        || (req.conflict_policy == ConflictPolicy::Overwrite && !permitted(&user, OVERWRITE))
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

pub(crate) async fn remove_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsDirNamesReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, DELETE) || req.names.iter().any(|name| !valid_name(name)) {
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

pub(crate) async fn batch_rename_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<BatchRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    if !permitted(&user, RENAME) {
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
