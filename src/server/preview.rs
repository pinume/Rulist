use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::filesystem::local::LocalFs;
use crate::preview::{
    PreviewMeta, PreviewReq, PreviewResponse, PreviewStrategy, detect_from_path, processor,
};
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, filesystem_error_response,
    normalize_request_path, signed_preview_url,
};

pub async fn preview_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<PreviewReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = match normalize_request_path(&req.path) {
        Ok(path) => path,
        Err(err) => return filesystem_error_response(&err, "normalize path"),
    };
    let fs = match LocalFs::new(&user.local_path, false) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    match fs.get(&path).await {
        Ok(file) => {
            if file.is_dir {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    400,
                    "Directory cannot be previewed",
                );
            }

            let (_, raw_url) = match signed_preview_url(&state, &user, &path) {
                Ok(signed) => signed,
                Err(err) => {
                    tracing::error!(error = %err, "failed to sign file path for preview");
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        500,
                        "Signing token is unavailable",
                    );
                }
            };

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
                match processor::process_file(&fs, &path, preview_type, file.size).await {
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
        Err(err) => filesystem_error_response(&err, "preview"),
    }
}
