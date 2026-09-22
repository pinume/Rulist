use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::Config;
use crate::db::{get_public_settings, DbPool};
use crate::model::ApiResponse;

pub struct AppState {
    pub pool: DbPool,
    #[allow(dead_code)]
    pub config: Config,
}

pub type SharedState = Arc<AppState>;

pub async fn run_server(config: Config, pool: DbPool) -> Result<(), anyhow::Error> {
    let state = Arc::new(AppState { pool, config: config.clone() });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ping", get(ping_handler))
        .route("/api/public/settings", get(public_settings_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.scheme.address, config.scheme.http_port).parse()?;
    info!("start HTTP server @ {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn ping_handler() -> impl IntoResponse {
    (StatusCode::OK, "pong")
}

async fn public_settings_handler(
    State(state): State<SharedState>,
) -> Response {
    match get_public_settings(&state.pool).await {
        Ok(settings) => Json(ApiResponse::success(settings)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(500, err.to_string())),
        )
            .into_response(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutting down gracefully...");
}
