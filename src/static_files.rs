use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
};
use rust_embed::RustEmbed;
use std::sync::Arc;

use crate::server::AppState;

#[derive(RustEmbed)]
#[folder = "public/dist/"]
pub struct DistAssets;

fn dist_mime(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        crate::preview::detector::detect_from_path(path).1
    }
}

pub fn serve_dist_asset(path: &str) -> Option<Response<Body>> {
    let clean_path = path.trim_start_matches('/');
    let file = DistAssets::get(clean_path)?;
    let mime = dist_mime(clean_path);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=15552000"),
        )
        .body(Body::from(file.data.into_owned()))
        .ok()?;

    Some(response)
}

pub async fn render_html(pool: &crate::db::DbPool) -> String {
    let raw_html = match DistAssets::get("index.html") {
        Some(file) => String::from_utf8_lossy(&file.data).to_string(),
        None => return "Rulist frontend not found".to_string(),
    };

    let settings = crate::db::get_public_settings(pool)
        .await
        .unwrap_or_default();
    let site_title = settings
        .get("site_title")
        .map(String::as_str)
        .unwrap_or("Rulist");
    let safe_site_title = escape_html(site_title);

    raw_html
        .replace(
            "<title>Rulist</title>",
            &format!("<title>{safe_site_title}</title>"),
        )
        .replace("Loading...", &safe_site_title)
}

pub async fn manifest_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let settings = crate::db::get_public_settings(&state.pool)
        .await
        .unwrap_or_default();
    let site_title = settings
        .get("site_title")
        .cloned()
        .unwrap_or_else(|| "Rulist".to_string());
    let logo = settings.get("logo").cloned().unwrap_or_default();
    let logo_first = logo.lines().next().unwrap_or(&logo).trim();
    let icons = if logo.trim().is_empty()
        || logo.trim() == "favicon.ico"
        || logo.trim() == "rulist.svg\nrulist-dark.svg"
    {
        serde_json::json!([
            { "src": "icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "icon-512.png", "sizes": "512x512", "type": "image/png" }
        ])
    } else {
        serde_json::json!([{ "src": logo_first }])
    };

    let manifest = serde_json::json!({
        "display": "standalone",
        "scope": "/",
        "start_url": "/",
        "name": site_title,
        "icons": icons
    });

    let mut response = axum::Json(manifest).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    response
}

pub async fn favicon_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let favicon_url = crate::db::get_setting(&state.pool, "favicon")
        .await
        .unwrap_or_default();
    if let Some(favicon) = favicon_url
        && !favicon.trim().is_empty()
    {
        return Redirect::temporary(&favicon).into_response();
    }

    if let Some(mut response) = serve_dist_asset("favicon.ico") {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return response;
    }
    StatusCode::NOT_FOUND.into_response()
}

pub async fn robots_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let robots = crate::db::get_setting(&state.pool, "robots_txt")
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "User-agent: *\nAllow: /\n".to_string());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(robots))
        .unwrap()
        .into_response()
}

pub async fn ping_handler() -> impl IntoResponse {
    (StatusCode::OK, "pong")
}

pub async fn dist_assets_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path();
    if let Some(response) = serve_dist_asset(path) {
        response
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub async fn spa_fallback_handler(
    State(state): State<Arc<AppState>>,
    uri: Uri,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let method = req.method();
    if method != axum::http::Method::GET && method != axum::http::Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = uri.path().trim_start_matches('/');
    if let Some(response) = serve_dist_asset(path) {
        return response;
    }

    let html = render_html(&state.pool).await;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(Body::from(html))
        .unwrap()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
