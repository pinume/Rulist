use axum::extract::{Json, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::db::get_setting;
use crate::driver::local::RenameError;
use crate::model::{
    BatchRenameReq, ConflictPolicy, DirItem, FsDirNamesReq, FsDirsReq, FsGetReq, FsLinkReq,
    FsLinkResp, FsListReq, FsListResp, FsMoveCopyReq, FsRecursiveMoveReq, FsRemoveEmptyDirsReq,
    FsRenameReq, sort_files_by, sorted_file_page,
};
use crate::server::stream::percent_decode;
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, encode_url_path, permission_denied,
    permitted, user_path, valid_name,
};
use crate::sign::sign_path;

pub(crate) async fn signing_token(state: &SharedState) -> Result<String, Response> {
    match get_setting(&state.pool, "token").await {
        Ok(Some(token)) if !token.trim().is_empty() => Ok(token),
        Ok(_) => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            500,
            "Signing token is unavailable",
        )),
        Err(err) => {
            tracing::error!(error = %err, "failed to load signing token");
            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Signing token is unavailable",
            ))
        }
    }
}

pub async fn fs_list_handler(
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

    match state.storage.list(&path).await {
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
            let token = match signing_token(&state).await {
                Ok(token) => token,
                Err(response) => return response,
            };

            for item in &mut content {
                if !item.is_dir {
                    let item_path = format!("{}/{}", path.trim_end_matches('/'), item.name);
                    let sign = match sign_path(
                        &token,
                        &item_path,
                        &state.storage.storage_context_for_path(&item_path),
                    ) {
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
                    item.raw_url = format!("/p{}?sign={}", encode_url_path(&item_path), sign);
                }
            }

            let resp = FsListResp {
                content,
                total,
                readme: String::new(),
                header: String::new(),
                write: permitted(&user, 3),
                provider: "Local".to_string(),
            };
            api_success(resp)
        }
        Err(err) => {
            tracing::error!(error = %err, path = %path, "failed to list directory");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn fs_get_handler(
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

    match state.storage.get(&path).await {
        Ok(mut file) => {
            let token = match signing_token(&state).await {
                Ok(token) => token,
                Err(response) => return response,
            };

            if !file.is_dir {
                let s = match sign_path(
                    &token,
                    &path,
                    &state.storage.storage_context_for_path(&path),
                ) {
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
                file.raw_url = format!("/p{}?sign={}", encode_url_path(&path), s);
            }

            api_success(file)
        }
        Err(err) => {
            tracing::error!(error = %err, path = %path, "failed to get file");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn fs_dirs_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsDirsReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
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
            tracing::error!(error = %err, path = %path, "failed to list dirs");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
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

pub async fn fs_mkdir_handler(
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
    match state.storage.mkdir(&path).await {
        Ok(_) => api_success(serde_json::Value::Null),
        Err(err) => {
            tracing::error!(error = %err, path = %path, "failed to create directory");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}

pub async fn fs_rename_handler(
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

    match state
        .storage
        .rename_safe(&path, &req.name, req.overwrite)
        .await
    {
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

pub async fn fs_move_handler(
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

    let policy = req.conflict_policy;
    let mut moves = Vec::new();
    for name in &req.names {
        let src = format!("{}/{}", src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", dst_dir.trim_end_matches('/'), name);
        if let Err(msg) = validate_transfer_paths(&src, &dst) {
            return api_error(StatusCode::BAD_REQUEST, 400, msg);
        }
        let dst_exists = state.storage.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(StatusCode::FORBIDDEN, 403, format!("file [{name}] exists"));
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
        if let Err(err) = state.storage.move_to_safe(&src, &dst, overwrite).await {
            tracing::error!(error = %err, src = %src, dst = %dst, "failed to move file");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                format!("Move failed for {src}; {completed} item(s) already moved"),
            );
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn fs_recursive_move_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRecursiveMoveReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    if !permitted(&user, 5)
        || (req.conflict_policy == ConflictPolicy::Overwrite && !permitted(&user, 8))
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
    if let Err(msg) = validate_transfer_paths(&src_dir, &dst_dir) {
        return api_error(StatusCode::BAD_REQUEST, 400, msg);
    }

    let mut dirs_to_visit = vec![src_dir.clone()];
    let mut dirs_to_create = Vec::new();
    let mut files = Vec::new();

    let src_prefix = src_dir.trim_end_matches('/');
    let dst_prefix = dst_dir.trim_end_matches('/');

    while let Some(current_dir) = dirs_to_visit.pop() {
        match state.storage.read_dir_physical(&current_dir).await {
            Ok(entries) => {
                for (name, is_dir) in entries {
                    let full_path = format!("{}/{}", current_dir.trim_end_matches('/'), name);
                    let rel_path = full_path
                        .strip_prefix(src_prefix)
                        .unwrap_or(&full_path)
                        .trim_start_matches('/')
                        .to_string();

                    if is_dir {
                        dirs_to_create.push(rel_path);
                        dirs_to_visit.push(full_path);
                    } else {
                        files.push((full_path, rel_path));
                    }
                }
            }
            Err(err) => {
                tracing::error!(error = %err, path = %current_dir, "failed to list directory in recursive move");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
            }
        }
    }

    // Sort directories by length so parents are created first
    dirs_to_create.sort_by_key(|a| a.len());

    let policy = req.conflict_policy;
    let mut moves = Vec::new();

    for (src_file, rel_path) in files {
        let dst_file = format!("{}/{}", dst_prefix, rel_path);
        let dst_exists = state.storage.get(&dst_file).await.is_ok();

        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(
                        StatusCode::FORBIDDEN,
                        403,
                        format!("destination path already exists: {dst_file}"),
                    );
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
    let mut created_dirs = 0;
    for rel_dir in &dirs_to_create {
        let target_dir = format!("{}/{}", dst_prefix, rel_dir);
        if let Err(err) = state.storage.mkdir(&target_dir).await {
            tracing::error!(
                error = %err,
                path = %target_dir,
                "failed to create recursive move target directory"
            );

            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                format!(
                    "Recursive move failed creating {target_dir}; {created_dirs} destination director(ies) may already exist"
                ),
            );
        }
        created_dirs += 1;
    }

    for (completed, (src, dst)) in moves.into_iter().enumerate() {
        let overwrite = policy == ConflictPolicy::Overwrite;
        if let Err(err) = state.storage.move_to_safe(&src, &dst, overwrite).await {
            tracing::error!(
                error = %err,
                src = %src,
                dst = %dst,
                "failed to move file in recursive move"
            );
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                format!(
                    "Recursive move failed for {src}; {created_dirs} destination director(ies) and {completed} file(s) were completed"
                ),
            );
        }
    }

    api_success(())
}

pub async fn fs_copy_handler(
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

    let policy = req.conflict_policy;
    let mut copies = Vec::new();
    for name in &req.names {
        let src = format!("{}/{}", src_dir.trim_end_matches('/'), name);
        let dst = format!("{}/{}", dst_dir.trim_end_matches('/'), name);
        if let Err(msg) = validate_transfer_paths(&src, &dst) {
            return api_error(StatusCode::BAD_REQUEST, 400, msg);
        }
        let dst_exists = state.storage.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(StatusCode::FORBIDDEN, 403, format!("file [{name}] exists"));
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
        if let Err(err) = state.storage.copy_to_safe(&src, &dst, overwrite).await {
            tracing::error!(error = %err, src = %src, dst = %dst, "failed to copy file");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                format!("Copy failed for {src}; {completed} item(s) already copied"),
            );
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn fs_remove_handler(
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
    for (completed, name) in req.names.into_iter().enumerate() {
        let target = format!("{}/{}", dir.trim_end_matches('/'), name);
        if let Err(err) = state.storage.remove(&target).await {
            tracing::error!(error = %err, target = %target, "failed to remove target");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                format!("Delete failed for {target}; {completed} item(s) already deleted"),
            );
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn fs_remove_empty_dirs_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<FsRemoveEmptyDirsReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, 7) {
        return permission_denied();
    }
    let root = match user_path(&user, &req.src_dir) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };

    let mut stack = vec![root.clone()];
    let mut dirs = Vec::new();

    while let Some(dir) = stack.pop() {
        let entries = match state.storage.list(&dir).await {
            Ok(entries) => entries,
            Err(err) => {
                if dir == root {
                    tracing::error!(
                        error = %err,
                        path = %dir,
                        "failed to list directory during empty dir removal"
                    );
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Internal server error",
                    );
                } else {
                    tracing::warn!(
                        error = %err,
                        path = %dir,
                        "failed to list child directory during empty dir removal, skipping"
                    );
                    continue;
                }
            }
        };

        for entry in entries {
            if entry.is_dir {
                let child = format!("{}/{}", dir.trim_end_matches('/'), entry.name);
                dirs.push(child.clone());
                stack.push(child);
            }
        }
    }

    // Sort deepest first
    dirs.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count()));

    for dir in dirs {
        match state.storage.is_physically_empty(&dir).await {
            Ok(true) => {
                if let Err(err) = state.storage.remove(&dir).await {
                    tracing::warn!(
                        error = %err,
                        path = %dir,
                        "failed to remove empty directory, skipping"
                    );
                }
            }

            Ok(false) => {}

            Err(err) => {
                tracing::warn!(
                    error = %err,
                    path = %dir,
                    "failed to verify directory emptiness, skipping"
                );
            }
        }
    }

    api_success(serde_json::Value::Null)
}

pub async fn fs_put_handler(
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
    let overwrite = headers.get("Overwrite").and_then(|h| h.to_str().ok()) == Some("true");
    if overwrite && !permitted(&user, 8) {
        return permission_denied();
    }

    let body = request.into_body();
    // Stream body to file
    state.storage.ensure_mounted(&file_path).await;
    let (ms, sub) = match state.storage.find_storage(&file_path) {
        Some(m) => m,
        None => return api_error(StatusCode::NOT_FOUND, 404, "storage not found"),
    };

    let target = match ms.driver.safe_resolve(&sub) {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(error = %err, "safe_resolve failed in put");
            return api_error(StatusCode::BAD_REQUEST, 400, "Invalid file path");
        }
    };

    if sub.is_empty() {
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
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
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
            Err(err) => {
                let _ = tokio::fs::remove_file(&temp).await;
                tracing::warn!(error = %err, "failed to read stream chunk during upload");
                return api_error(StatusCode::BAD_REQUEST, 400, "Failed to read upload data");
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

pub async fn fs_batch_rename_handler(
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

    let pairs: Vec<(String, String)> = req
        .rename_objects
        .into_iter()
        .map(|o| (o.src_name, o.new_name))
        .collect();

    match state.storage.batch_rename(&src_dir, &pairs).await {
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

pub async fn fs_link_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsLinkReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };

    let token = match signing_token(&state).await {
        Ok(token) => token,
        Err(response) => return response,
    };

    let clean_path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    let sign = match sign_path(
        &token,
        &clean_path,
        &state.storage.storage_context_for_path(&clean_path),
    ) {
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
    let url = format!("/d{}?sign={}", encode_url_path(&clean_path), sign);
    api_success(FsLinkResp { url })
}
