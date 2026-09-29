use axum::Router;
use axum::extract::{Request, State};
use axum::http::header::{HeaderName, HeaderValue, X_CONTENT_TYPE_OPTIONS};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post, put};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use super::{SharedState, api_success, auth, files, preview, stream};

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
        .route("/api/auth/logout", get(auth::logout_handler))
        .route("/api/fs/list", post(files::list_handler))
        .route("/api/fs/get", post(files::get_handler))
        .route("/api/fs/dirs", post(files::dirs_handler))
        .route("/api/fs/mkdir", post(files::mkdir_handler))
        .route("/api/fs/rename", post(files::rename_handler))
        .route("/api/fs/move", post(files::move_handler))
        .route("/api/fs/copy", post(files::copy_handler))
        .route("/api/fs/remove", post(files::remove_handler))
        .route("/api/fs/put", put(files::upload_handler))
        .route("/api/fs/batch_rename", post(files::batch_rename_handler))
        .route("/api/fs/link", post(files::link_handler))
        .route("/api/fs/preview", post(preview::preview_handler))
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

#[derive(Serialize)]
struct PublicConfig {
    site_title: String,
    logo: String,
    favicon: String,
    main_color: String,
    hide_files: Vec<String>,
    package_download: bool,
    announcement: String,
    version: String,
}

async fn public_settings_handler(State(state): State<SharedState>) -> Response {
    let site = &state.config.site;
    api_success(PublicConfig {
        site_title: site.site_title.clone(),
        logo: site.logo.clone(),
        favicon: site.favicon.clone(),
        main_color: site.main_color.clone(),
        hide_files: site.hide_files.clone(),
        package_download: site.package_download,
        announcement: site.announcement.clone(),
        version: format!("v{}-rust", env!("CARGO_PKG_VERSION")),
    })
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
