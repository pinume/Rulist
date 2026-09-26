use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::preview::{
    PreviewMeta, PreviewReq, PreviewResponse, PreviewStrategy, detect_from_path, processor,
};
use crate::server::fs::signing_token;
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, encode_url_path, permission_denied,
    user_path,
};
use crate::sign::sign_path;

pub async fn preview_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<PreviewReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = match user_path(&user, &req.path) {
        Ok(path) => path,
        Err(_) => return permission_denied(),
    };

    match state.storage.get(&path).await {
        Ok(file) => {
            if file.is_dir {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    400,
                    "Directory cannot be previewed",
                );
            }

            let token = match signing_token(&state).await {
                Ok(token) => token,
                Err(response) => return response,
            };

            let sign = match sign_path(
                &token,
                &path,
                &state.storage.storage_context_for_path(&path),
            ) {
                Ok(s) => s,
                Err(err) => {
                    tracing::error!(error = %err, "failed to sign file path for preview");
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Signing token is unavailable",
                    );
                }
            };

            let raw_url = format!("/p{}?sign={}", encode_url_path(&path), sign);
            let (preview_type, mime_type) = detect_from_path(&file.name);
            let strategy = preview_type.strategy();

            let meta = PreviewMeta {
                name: file.name,
                path: path.clone(),
                size: file.size.max(0) as u64,
                modified: file.modified,
                mime_type: mime_type.to_string(),
                preview_type,
                strategy,
                raw_url,
                permissions: file.permissions,
            };

            let (content, error) = if strategy == PreviewStrategy::Processed {
                match processor::process_file(&state.storage, &path, preview_type, file.size).await
                {
                    Ok(content) => (Some(content), None),
                    Err(err_code) => (None, Some(err_code.to_string())),
                }
            } else {
                (None, None)
            };

            api_success(PreviewResponse {
                meta,
                content,
                error,
            })
        }
        Err(err) => {
            tracing::error!(error = %err, path = %path, "failed to get file for preview");
            api_error(StatusCode::NOT_FOUND, 404, "File not found")
        }
    }
}
