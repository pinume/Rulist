mod auth;
mod files;
mod preview;
mod routes;
mod security;
mod stream;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use tracing::info;

use crate::config::Config;
use crate::db::DbPool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            code: 200,
            message: "success".to_string(),
            data: Some(data),
        }
    }
    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

pub use routes::build_app;
pub(crate) use security::{authenticate_user, encode_url_path, permitted, user_path};

pub struct AppState {
    pub pool: DbPool,
    pub config: Config,
}

pub type SharedState = Arc<AppState>;

pub async fn run_server(config: Config, pool: DbPool) -> Result<(), anyhow::Error> {
    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
    });
    let app = build_app(state);

    let ip: IpAddr = config.scheme.address.parse()?;
    let addr = SocketAddr::new(ip, config.scheme.http_port);
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
