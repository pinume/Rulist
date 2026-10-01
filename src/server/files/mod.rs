use axum::http::StatusCode;
use axum::response::Response;

use crate::filesystem::FsError;
use crate::filesystem::local::LocalFs;
use crate::server::api_error;
use crate::server::{SharedState, encode_url_path};
use crate::sign::sign_path;

mod browse;
mod mutate;
mod upload;

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub(crate) struct PathReq {
    #[serde(default)]
    pub path: String,
}

pub(crate) use browse::{dirs_handler, get_handler, link_handler, list_handler, sign_context};
pub(crate) use mutate::{
    batch_rename_handler, copy_handler, mkdir_handler, move_handler, remove_handler, rename_handler,
};
pub(crate) use upload::upload_handler;

pub(crate) fn signed_preview_url(
    state: &SharedState,
    user: &crate::db::User,
    path: &str,
) -> anyhow::Result<(String, String)> {
    signed_url(state, user, path, "/p")
}

pub(crate) fn signed_download_url(
    state: &SharedState,
    user: &crate::db::User,
    path: &str,
) -> anyhow::Result<String> {
    signed_url(state, user, path, "/d").map(|(_, url)| url)
}

fn signed_url(
    state: &SharedState,
    user: &crate::db::User,
    path: &str,
    prefix: &str,
) -> anyhow::Result<(String, String)> {
    let sign = sign_path(&state.config.jwt_secret, path, &sign_context(user))?;
    let url = format!(
        "{prefix}{}?sign={sign}&uid={}",
        encode_url_path(path),
        user.id
    );
    Ok((sign, url))
}

fn user_fs(user: &crate::db::User) -> Result<LocalFs, anyhow::Error> {
    LocalFs::new(&user.local_path, false)
}

pub(crate) fn filesystem_error_details(
    err: &anyhow::Error,
    action: &'static str,
) -> (StatusCode, i32, &'static str) {
    let fs_error = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<FsError>())
        .copied();
    let kind = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .map(std::io::Error::kind);
    let result = match fs_error {
        Some(FsError::InvalidPath) => (StatusCode::BAD_REQUEST, 400, "Invalid filesystem request"),
        Some(FsError::Forbidden) => (StatusCode::FORBIDDEN, 403, "Permission denied"),
        Some(FsError::NotFound) => (StatusCode::NOT_FOUND, 404, "File not found"),
        Some(FsError::Conflict) => (StatusCode::CONFLICT, 409, "File already exists"),
        None => match kind {
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
        },
    };

    if result.0.is_server_error() {
        tracing::error!(error = %err, action, "filesystem operation failed");
    } else {
        tracing::debug!(error = %err, action, "filesystem operation failed");
    }

    result
}

pub(crate) fn filesystem_error_response(err: &anyhow::Error, action: &'static str) -> Response {
    let (status, code, message) = filesystem_error_details(err, action);
    api_error(status, code, message)
}
