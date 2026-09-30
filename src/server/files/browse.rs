use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::filesystem::{FileEntry, sort_files_by, sorted_file_page};
use crate::server::{
    SharedState, api_error, api_success, authenticate_user, normalize_request_path,
};

use super::{filesystem_error_response, signed_download_url, signed_preview_url, user_fs};

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub(crate) struct FsListReq {
    #[serde(default)]
    path: String,
    #[serde(default)]
    page: Option<usize>,
    #[serde(default)]
    per_page: Option<usize>,
    #[serde(default)]
    order_by: Option<String>,
    #[serde(default)]
    reverse: Option<bool>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FsListResp {
    content: Vec<FileEntry>,
    total: i64,
}
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub(crate) struct FsGetReq {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub(crate) struct FsDirsReq {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct DirItem {
    name: String,
    modified: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct FsLinkReq {
    path: String,
}
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct FsLinkResp {
    url: String,
}

pub(crate) fn sign_context(user: &crate::db::User) -> String {
    format!(
        "uid={}:pwd_ts={}:root={}",
        user.id, user.pwd_ts, user.local_path
    )
}

pub(crate) async fn list_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsListReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = match normalize_request_path(&req.path) {
        Ok(path) => path,
        Err(err) => return filesystem_error_response(&err, "normalize path"),
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
                    (item.sign, item.raw_url) = match signed_preview_url(&state, &user, &item_path)
                    {
                        Ok((sign, url)) => (sign, url),
                        Err(err) => {
                            tracing::error!(error = %err, "failed to sign file path");
                            return api_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                500,
                                "Signing token is unavailable",
                            );
                        }
                    };
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

pub(crate) async fn get_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsGetReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = match normalize_request_path(&req.path) {
        Ok(path) => path,
        Err(err) => return filesystem_error_response(&err, "normalize path"),
    };
    let fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };

    match fs.get(&path).await {
        Ok(mut file) => {
            if !file.is_dir {
                let (sign, raw_url) = match signed_preview_url(&state, &user, &path) {
                    Ok(signed) => signed,
                    Err(err) => {
                        tracing::error!(error = %err, "failed to sign file path");
                        return api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            500,
                            "Signing token is unavailable",
                        );
                    }
                };
                file.sign = sign;
                file.raw_url = raw_url;
            }

            api_success(file)
        }
        Err(err) => {
            tracing::debug!(path = %path, "failed to get file");
            filesystem_error_response(&err, "get")
        }
    }
}

pub(crate) async fn dirs_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsDirsReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };
    let path = normalize_request_path(&req.path);
    let path = match path {
        Ok(path) => path,
        Err(err) => return filesystem_error_response(&err, "normalize path"),
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

pub(crate) async fn link_handler(
    headers: HeaderMap,
    State(state): State<SharedState>,
    Json(req): Json<FsLinkReq>,
) -> Response {
    let Some(user) = authenticate_user(&headers, &state).await else {
        return api_error(StatusCode::UNAUTHORIZED, 401, "Authentication required");
    };

    let clean_path = match normalize_request_path(&req.path) {
        Ok(path) => path,
        Err(err) => return filesystem_error_response(&err, "normalize path"),
    };
    let _fs = match user_fs(&user) {
        Ok(fs) => fs,
        Err(_) => return api_error(StatusCode::NOT_FOUND, 404, "File root not found"),
    };
    let url = match signed_download_url(&state, &user, &clean_path) {
        Ok(url) => url,
        Err(err) => {
            tracing::error!(error = %err, "failed to sign file path");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Signing token is unavailable",
            );
        }
    };
    api_success(FsLinkResp { url })
}
