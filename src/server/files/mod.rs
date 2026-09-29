use axum::http::StatusCode;
use axum::response::Response;

use crate::filesystem::local::LocalFs;
use crate::server::api_error;

mod browse;
mod mutate;
mod upload;

pub(crate) use browse::{dirs_handler, get_handler, link_handler, list_handler, sign_context};
pub(crate) use mutate::{
    batch_rename_handler, copy_handler, mkdir_handler, move_handler, remove_handler, rename_handler,
};
pub(crate) use upload::upload_handler;

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
