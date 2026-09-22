use axum::extract::{Json, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::db::get_setting;
use crate::model::{
    BatchRenameReq, ConflictPolicy, DirItem, FsDirNamesReq, FsDirsReq, FsGetReq, FsLinkReq,
    FsLinkResp, FsListReq, FsListResp, FsMoveCopyReq, FsRecursiveMoveReq, FsRemoveEmptyDirsReq,
    FsRenameReq, sort_files,
};
use crate::server::stream::percent_decode;
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, encode_url_path, permission_denied,
    permitted, user_path, valid_name,
};
use crate::sign::sign_path;

pub async fn fs_list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsListReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::OK, 401, "Authentication required");
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
            api_success(resp)
        }
        Err(err) => api_error(StatusCode::OK, 500, err.to_string()),
    }
}

pub async fn fs_get_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsGetReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::OK, 401, "Authentication required");
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

            api_success(file)
        }
        Err(err) => api_error(StatusCode::OK, 500, err.to_string()),
    }
}

pub async fn fs_dirs_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsDirsReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::OK, 401, "Authentication required");
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
        Err(err) => return api_error(StatusCode::OK, 500, err.to_string()),
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
        Err(err) => api_error(StatusCode::OK, 500, err.to_string()),
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
    if !permitted(&user, 4) || !valid_name(&req.name) {
        return permission_denied();
    }
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };
    match state.storage.rename(&path, &req.name).await {
        Ok(_) => api_success(serde_json::Value::Null),
        Err(err) => api_error(StatusCode::OK, 500, err.to_string()),
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
        if let Err(msg) = validate_transfer_paths(&src, &dst) {
            return api_error(StatusCode::BAD_REQUEST, 400, msg);
        }
        let dst_exists = state.storage.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(StatusCode::OK, 403, format!("file [{name}] exists"));
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
            match state.storage.remove(&dst).await {
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(error = %err, path = %dst, "failed to remove destination");
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Failed to overwrite destination",
                    );
                }
            }
        }
        if let Err(err) = state.storage.move_to(&src, &dst).await {
            return api_error(StatusCode::OK, 500, err.to_string());
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
        return api_error(StatusCode::OK, 401, "Authentication required");
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
    if let Err(msg) = validate_transfer_paths(&src_dir, &dst_dir) {
        return api_error(StatusCode::BAD_REQUEST, 400, msg);
    }

    let mut dirs_to_visit = vec![src_dir.clone()];
    let mut dirs_to_create = Vec::new();
    let mut files = Vec::new();

    let src_prefix = src_dir.trim_end_matches('/');
    let dst_prefix = dst_dir.trim_end_matches('/');

    while let Some(current_dir) = dirs_to_visit.pop() {
        match state.storage.list(&current_dir).await {
            Ok(entries) => {
                for entry in entries {
                    let full_path = format!("{}/{}", current_dir.trim_end_matches('/'), entry.name);
                    let rel_path = full_path
                        .strip_prefix(src_prefix)
                        .unwrap_or(&full_path)
                        .trim_start_matches('/')
                        .to_string();

                    if entry.is_dir {
                        dirs_to_create.push(rel_path);
                        dirs_to_visit.push(full_path);
                    } else {
                        files.push((full_path, rel_path));
                    }
                }
            }
            Err(err) => return api_error(StatusCode::OK, 500, err.to_string()),
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
                        StatusCode::OK,
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
    for rel_dir in &dirs_to_create {
        let target_dir = format!("{}/{}", dst_prefix, rel_dir);
        let _ = state.storage.mkdir(&target_dir).await;
    }

    for (src, dst) in moves {
        if policy == ConflictPolicy::Overwrite && state.storage.get(&dst).await.is_ok() {
            match state.storage.remove(&dst).await {
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(error = %err, path = %dst, "failed to remove destination");
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Failed to overwrite destination",
                    );
                }
            }
        }
        if let Err(err) = state.storage.move_to(&src, &dst).await {
            return api_error(StatusCode::OK, 500, err.to_string());
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
        if let Err(msg) = validate_transfer_paths(&src, &dst) {
            return api_error(StatusCode::BAD_REQUEST, 400, msg);
        }
        let dst_exists = state.storage.get(&dst).await.is_ok();
        if dst_exists {
            match policy {
                ConflictPolicy::Cancel => {
                    return api_error(StatusCode::OK, 403, format!("file [{name}] exists"));
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
            match state.storage.remove(&dst).await {
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(error = %err, path = %dst, "failed to remove destination");
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Failed to overwrite destination",
                    );
                }
            }
        }
        if let Err(err) = state.storage.copy_to(&src, &dst).await {
            return api_error(StatusCode::OK, 500, err.to_string());
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
    for name in req.names {
        let target = format!("{}/{}", dir.trim_end_matches('/'), name);
        if let Err(err) = state.storage.remove(&target).await {
            return api_error(StatusCode::OK, 500, err.to_string());
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
                tracing::error!(error = %err, path = %dir, "failed to list directory during empty dir removal");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
                );
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
        match state.storage.list(&dir).await {
            Ok(entries) => {
                if entries.is_empty() {
                    match state.storage.remove(&dir).await {
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!(error = %err, path = %dir, "failed to remove empty directory");
                            return api_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                500,
                                "Internal server error",
                            );
                        }
                    }
                }
            }
            Err(err) => {
                tracing::error!(error = %err, path = %dir, "failed to check directory contents");
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    500,
                    "Internal server error",
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

    let body = request.into_body();
    // Stream body to file
    let (ms, sub) = match state.storage.find_storage(&file_path) {
        Some(m) => m,
        None => return api_error(StatusCode::NOT_FOUND, 404, "storage not found"),
    };

    let target = match ms.driver.safe_resolve(&sub) {
        Ok(t) => t,
        Err(err) => return api_error(StatusCode::BAD_REQUEST, 400, err.to_string()),
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
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string());
    }

    let temp = target.with_file_name(format!(".rulist-upload-{}", crate::auth::rand_string(24)));
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
    {
        Ok(f) => f,
        Err(err) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string()),
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
                    return api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string());
                }
            }
            Err(err) => {
                let _ = tokio::fs::remove_file(&temp).await;
                return api_error(StatusCode::BAD_REQUEST, 400, err.to_string());
            }
        }
    }

    if let Err(err) = file.flush().await {
        let _ = tokio::fs::remove_file(&temp).await;
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string());
    }
    drop(file);
    if !overwrite && target.exists() {
        let _ = tokio::fs::remove_file(&temp).await;
        return api_error(StatusCode::CONFLICT, 409, "file already exists");
    }
    if let Err(err) = tokio::fs::rename(&temp, &target).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string());
    }

    api_success(serde_json::Value::Null)
}

pub async fn fs_batch_rename_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<BatchRenameReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::OK, 401, "Authentication required");
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
            return api_error(StatusCode::OK, 500, err.to_string());
        }
    }
    api_success(())
}

pub async fn fs_link_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsLinkReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::OK, 401, "Authentication required");
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
    api_success(FsLinkResp { url })
}
