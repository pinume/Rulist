use axum::Router;
use axum::extract::{Request, State};
use axum::http::header::{HeaderName, HeaderValue, X_CONTENT_TYPE_OPTIONS};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post, put};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use super::{SharedState, api_success, auth, fs, preview, stream, users};

pub fn build_app(state: SharedState) -> Router {
    let mut cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);
    if state.config.server.allow_cors {
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
        .route("/api/fs/copy", post(fs::fs_copy_handler))
        .route("/api/fs/remove", post(fs::fs_remove_handler))
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
        .layer(middleware::from_fn(security_headers_middleware))
        .with_state(state)
}

async fn public_settings_handler(State(state): State<SharedState>) -> Response {
    let ui = &state.config.ui;
    api_success(std::collections::HashMap::from([
        ("site_title", ui.site_title.clone()),
        ("logo", ui.logo.clone()),
        ("favicon", ui.favicon.clone()),
        ("main_color", ui.main_color.clone()),
        ("hide_files", ui.hide_files.join("\n")),
        ("package_download", ui.package_download.to_string()),
        ("announcement", ui.announcement.clone()),
        ("version", format!("v{}-rust", env!("CARGO_PKG_VERSION"))),
    ]))
}

async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("SAMEORIGIN"),
    );
    response
}
