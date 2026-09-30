use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use tokio::io::AsyncWriteExt;

use crate::permissions::{OVERWRITE, WRITE_CONTENT};
use crate::server::stream::percent_decode;
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, filesystem_error_response,
    normalize_request_path, permission_denied, permitted,
};

use super::user_fs;

pub(crate) async fn upload_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "unauthorized");
    };
    if !permitted(&user, WRITE_CONTENT) {
        return permission_denied();
    }

    let file_path = match headers.get("File-Path").and_then(|h| h.to_str().ok()) {
        Some(p) => percent_decode(p),
        None => return api_error(StatusCode::BAD_REQUEST, 400, "missing File-Path header"),
    };
    let file_path = match normalize_request_path(&file_path) {
        Ok(path) => path,
        Err(err) => return filesystem_error_response(&err, "normalize upload path"),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    let overwrite = headers.get("Overwrite").and_then(|h| h.to_str().ok()) == Some("true");
    if overwrite && !permitted(&user, OVERWRITE) {
        return permission_denied();
    }

    let mut body = request.into_body();
    // Stream body to file
    let target = match fs.safe_resolve(&file_path) {
        Ok(t) => t,
        Err(err) => {
            return filesystem_error_response(&err, "resolve upload path");
        }
    };

    if file_path.trim_matches('/').is_empty() {
        return filesystem_error_response(
            &crate::filesystem::FsError::InvalidPath.into(),
            "resolve upload path",
        );
    }
    if !overwrite {
        match fs.entry_exists(&file_path).await {
            Ok(false) => {}
            Ok(true) => return api_error(StatusCode::CONFLICT, 409, "file already exists"),
            Err(err) => return filesystem_error_response(&err, "check upload target"),
        }
    }
    if let Some(parent) = target.parent() {
        if let Err(err) = tokio::fs::create_dir_all(parent).await {
            tracing::error!(error = %err, "failed to create parent dir for upload");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            );
        }
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
        match fs.entry_exists(&file_path).await {
            Ok(false) => {}
            Ok(true) => {
                let _ = tokio::fs::remove_file(&temp).await;
                return api_error(StatusCode::CONFLICT, 409, "file already exists");
            }
            Err(err) => {
                let _ = tokio::fs::remove_file(&temp).await;
                return filesystem_error_response(&err, "check upload target");
            }
        }
        if let Err(err) = tokio::fs::hard_link(&temp, &target).await {
            let _ = tokio::fs::remove_file(&temp).await;
            if err.kind() == std::io::ErrorKind::AlreadyExists {
                return api_error(StatusCode::CONFLICT, 409, "file already exists");
            }
            return filesystem_error_response(&err.into(), "finalize upload");
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
