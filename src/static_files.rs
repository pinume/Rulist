use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
};
use rust_embed::RustEmbed;
use serde::Serialize;
use std::sync::Arc;

use crate::server::AppState;

#[derive(RustEmbed)]
#[folder = "public/dist/"]
pub struct DistAssets;

pub const RULIST_SVG: &[u8] = include_bytes!("../public/rulist.svg");
pub const RULIST_PNG: &[u8] = include_bytes!("../public/rulist.png");

#[derive(Serialize)]
struct ManifestIcon {
    src: String,
    sizes: String,
    #[serde(rename = "type")]
    icon_type: String,
}

#[derive(Serialize)]
struct Manifest {
    display: String,
    scope: String,
    start_url: String,
    name: String,
    icons: Vec<ManifestIcon>,
}

pub fn serve_dist_asset(path: &str) -> Option<Response<Body>> {
    let clean_path = path.trim_start_matches('/');
    let file = DistAssets::get(clean_path)?;
    let mime = mime_guess::from_path(clean_path).first_or_octet_stream();

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

pub async fn render_html(pool: &crate::db::DbPool, is_manage: bool) -> String {
    let raw_html = match DistAssets::get("index.html") {
        Some(f) => String::from_utf8_lossy(&f.data).to_string(),
        None => return "Rulist frontend not found".to_string(),
    };

    let settings = crate::db::get_public_settings(pool)
        .await
        .unwrap_or_default();

    let site_title = settings
        .get("site_title")
        .map(|s| s.as_str())
        .unwrap_or("Rulist");
    let safe_site_title = escape_html(site_title);
    let main_color = settings
        .get("main_color")
        .map(|s| s.as_str())
        .unwrap_or("#1890ff");
    let logo = settings
        .get("logo")
        .map(|s| s.as_str())
        .unwrap_or("favicon.ico");
    let favicon = settings.get("favicon").map(|s| s.as_str()).unwrap_or("");
    let logo_first = logo.lines().next().unwrap_or(logo);
    let fav = if favicon.is_empty() {
        "favicon.ico"
    } else {
        favicon
    };

    let mut html = raw_html
        .replace("cdn: undefined", "cdn: ''")
        .replace("base_path: undefined", "base_path: '/'")
        .replace(
            "main_color: undefined",
            &format!("main_color: '{}'", escape_html(main_color)),
        )
        .replace("https://res.oplist.org/logo/logo.svg", &escape_html(fav))
        .replace(
            "https://res.oplist.org/logo/logo.png",
            &escape_html(logo_first),
        )
        .replace(
            "<title>Rulist</title>",
            &format!("<title>{}</title>", safe_site_title),
        )
        .replace("Loading...", &safe_site_title);

    if !is_manage {
        let customize_head: Option<String> = sqlx::query_scalar(
            "SELECT `value` FROM `x_setting_items` WHERE `key` = 'customize_head'",
        )
        .fetch_optional(pool)
        .await
        .unwrap_or_default();

        let customize_body: Option<String> = sqlx::query_scalar(
            "SELECT `value` FROM `x_setting_items` WHERE `key` = 'customize_body'",
        )
        .fetch_optional(pool)
        .await
        .unwrap_or_default();

        html = html
            .replace(
                "<!-- customize head -->",
                &customize_head.unwrap_or_default(),
            )
            .replace(
                "<!-- customize body -->",
                &customize_body.unwrap_or_default(),
            );
    }

    html
}

pub async fn manifest_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let settings = crate::db::get_public_settings(&state.pool)
        .await
        .unwrap_or_default();
    let site_title = settings
        .get("site_title")
        .cloned()
        .unwrap_or_else(|| "Rulist".to_string());
    let logo = settings
        .get("logo")
        .cloned()
        .unwrap_or_else(|| "favicon.ico".to_string());
    let logo_first = logo.lines().next().unwrap_or(&logo).to_string();

    let manifest = Manifest {
        display: "standalone".to_string(),
        scope: "/".to_string(),
        start_url: "/".to_string(),
        name: site_title,
        icons: vec![ManifestIcon {
            src: logo_first,
            sizes: "512x512".to_string(),
            icon_type: "image/png".to_string(),
        }],
    };

    let mut res = axum::Json(manifest).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    res
}

pub async fn favicon_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let favicon_url = crate::db::get_setting(&state.pool, "favicon")
        .await
        .unwrap_or_default();
    if let Some(fav) = favicon_url
        && !fav.trim().is_empty()
    {
        return Redirect::temporary(&fav).into_response();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(RULIST_SVG))
        .unwrap()
        .into_response()
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

pub async fn rulist_svg_handler() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(RULIST_SVG))
        .unwrap()
}

pub async fn rulist_png_handler() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(RULIST_PNG))
        .unwrap()
}

pub async fn dist_assets_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path();
    if let Some(res) = serve_dist_asset(path) {
        res
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

    // 1. Direct match in embedded assets
    if let Some(res) = serve_dist_asset(path) {
        return res;
    }
    if path == "rulist.svg" {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/svg+xml")
            .body(Body::from(RULIST_SVG))
            .unwrap();
    }
    if path == "rulist.png" {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .body(Body::from(RULIST_PNG))
            .unwrap();
    }

    // 2. Otherwise serve SPA HTML
    let is_manage = path.starts_with("@manage") || path.starts_with("manage");
    let html = render_html(&state.pool, is_manage).await;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(Body::from(html))
        .unwrap()
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
