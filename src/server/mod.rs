pub mod auth;
pub mod fs;
pub mod preview;
mod routes;
mod security;
pub mod stream;
pub mod users;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use tracing::info;

use crate::config::Config;
use crate::db::DbPool;
use crate::driver::StorageManager;
use crate::model::ApiResponse;

pub use routes::build_app;
pub(crate) use security::{authenticate_user, encode_url_path, permitted, user_path, valid_name};

pub struct AppState {
    pub pool: DbPool,
    pub config: Config,
    pub storage: StorageManager,
}

pub type SharedState = Arc<AppState>;

pub async fn run_server(
    config: Config,
    pool: DbPool,
    storage: StorageManager,
) -> Result<(), anyhow::Error> {
    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        storage,
    });
    let app = build_app(state);

    let ip: IpAddr = config.server.address.parse()?;
    let addr = SocketAddr::new(ip, config.server.port);
    if addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) && addr.ip() != IpAddr::V6(Ipv6Addr::LOCALHOST)
    {
        anyhow::bail!(
            "security restriction: only 127.0.0.1 or ::1 is permitted, got {}",
            addr.ip()
        );
    }
    info!("start HTTP server @ {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

pub(crate) fn api_error<T: Into<String>>(status: StatusCode, code: i32, msg: T) -> Response {
    (status, Json(ApiResponse::<()>::error(code, msg))).into_response()
}

pub(crate) fn api_success<T: serde::Serialize>(data: T) -> Response {
    Json(ApiResponse::success(data)).into_response()
}

pub(crate) fn permission_denied() -> Response {
    api_error(StatusCode::FORBIDDEN, 403, "Permission denied")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutting down gracefully...");
}
