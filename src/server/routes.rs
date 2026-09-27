use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post, put};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::db::get_public_settings;

use super::{SharedState, api_error, api_success, auth, fs, preview, stream, users};

pub fn build_app(state: SharedState) -> Router {
    let mut cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);
    if state.config.scheme.allow_cors {
        cors = cors.allow_origin(Any);
    }

    Router::new()
        .route("/ping", get(crate::static_files::ping_handler))
        .route("/favicon.ico", get(crate::static_files::favicon_handler))
        .route("/robots.txt", get(crate::static_files::robots_handler))
        .route("/manifest.json", get(crate::static_files::manifest_handler))
        .route(
            "/assets/{*path}",
            get(crate::static_files::dist_assets_handler),
        )
        .route(
            "/static/{*path}",
            get(crate::static_files::dist_assets_handler),
        )
        .route(
            "/streamer/{*path}",
            get(crate::static_files::dist_assets_handler),
        )
        .route("/api/public/settings", get(public_settings_handler))
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/me", get(auth::current_user_handler))
        .route("/api/me/update", post(auth::update_current_handler))
        .route("/api/auth/logout", get(auth::logout_handler))
        .route("/api/fs/list", post(fs::fs_list_handler))
        .route("/api/fs/get", post(fs::fs_get_handler))
        .route("/api/fs/dirs", post(fs::fs_dirs_handler))
        .route("/api/fs/mkdir", post(fs::fs_mkdir_handler))
        .route("/api/fs/rename", post(fs::fs_rename_handler))
        .route("/api/fs/move", post(fs::fs_move_handler))
        .route(
            "/api/fs/recursive_move",
            post(fs::fs_recursive_move_handler),
        )
        .route("/api/fs/copy", post(fs::fs_copy_handler))
        .route("/api/fs/remove", post(fs::fs_remove_handler))
        .route(
            "/api/fs/remove_empty_directory",
            post(fs::fs_remove_empty_dirs_handler),
        )
        .route("/api/fs/put", put(fs::fs_put_handler))
        .route("/api/fs/batch_rename", post(fs::fs_batch_rename_handler))
        .route("/api/fs/link", post(fs::fs_link_handler))
        .route("/api/fs/preview", post(preview::preview_handler))
        .route("/api/admin/user/list", get(users::admin_user_list_handler))
        .route("/api/admin/user/get", get(users::admin_user_get_handler))
        .route(
            "/api/admin/user/create",
            post(users::admin_user_create_handler),
        )
        .route(
            "/api/admin/user/update",
            post(users::admin_user_update_handler),
        )
        .route(
            "/api/admin/user/delete",
            post(users::admin_user_delete_handler),
        )
        .route(
            "/api/admin/user/cancel_2fa",
            post(users::admin_user_cancel_2fa_handler),
        )
        .route(
            "/d/{*path}",
            get(stream::raw_download_handler).head(stream::raw_download_handler),
        )
        .route(
            "/p/{*path}",
            get(stream::raw_preview_handler).head(stream::raw_preview_handler),
        )
        .fallback(crate::static_files::spa_fallback_handler)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn public_settings_handler(State(state): State<SharedState>) -> Response {
    match get_public_settings(&state.pool).await {
        Ok(settings) => api_success(settings),
        Err(err) => {
            tracing::error!(error = %err, "failed to get public settings");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                500,
                "Internal server error",
            )
        }
    }
}
