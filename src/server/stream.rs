use std::path::Path;

use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;

use crate::db::get_setting;
use crate::server::{SharedState, authenticate_user, encode_url_path};
use crate::sign::verify_sign;

#[derive(Debug, Deserialize)]
pub struct SignQuery {
    pub sign: Option<String>,
}

pub async fn raw_download_handler(
    State(state): State<SharedState>,
    AxumPath(path): AxumPath<String>,
    Query(query): Query<SignQuery>,
    headers: HeaderMap,
) -> Response {
    stream_file(state, path, query.sign, headers, true).await
}

pub async fn raw_preview_handler(
    State(state): State<SharedState>,
    AxumPath(path): AxumPath<String>,
    Query(query): Query<SignQuery>,
    headers: HeaderMap,
) -> Response {
    stream_file(state, path, query.sign, headers, false).await
}

pub fn percent_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let input = s.as_bytes();
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%'
            && i + 2 < input.len()
            && let Ok(hex) = std::str::from_utf8(&input[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            bytes.push(byte);
            i += 3;
            continue;
        }
        bytes.push(input[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

async fn stream_file(
    state: SharedState,
    raw_path: String,
    sign: Option<String>,
    headers: HeaderMap,
    as_attachment: bool,
) -> Response {
    let clean_path = format!("/{}", raw_path.trim_start_matches('/'));
    if clean_path.split(['/', '\\']).any(|p| p == "." || p == "..") {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    // Check signature if sign_all is enabled or sign is provided
    let sign_all = get_setting(&state.pool, "sign_all")
        .await
        .ok()
        .flatten()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if sign_all || sign.is_some() {
        let token = get_setting(&state.pool, "token")
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let s = sign.unwrap_or_default();
        if verify_sign(&token, &clean_path, &s).is_err() {
            return (
                StatusCode::FORBIDDEN,
                "Invalid or expired download link signature",
            )
                .into_response();
        }
    } else {
        let Some(user) = authenticate_user(&headers, &state).await else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        let base = user.base_path.trim_end_matches('/');
        if !user.is_admin() && clean_path != base && !clean_path.starts_with(&format!("{base}/")) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    let mut file = match state.storage.open(&clean_path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    let meta = match file.metadata().await {
        Ok(m) => m,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read file metadata",
            )
                .into_response();
        }
    };

    if meta.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot download directory directly",
        )
            .into_response();
    }

    let file_size = meta.len();
    let content_type = mime_guess::from_path(&clean_path)
        .first_or_octet_stream()
        .to_string();

    let filename = Path::new(&clean_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");

    let disposition = safe_content_disposition(filename, as_attachment);

    // Range header handling
    let range_header = headers.get("range").and_then(|r| r.to_str().ok());

    if let Some(range_val) = range_header
        && let Some((start, end)) = parse_range(range_val, file_size)
    {
        let part_len = end - start + 1;
        if file.seek(SeekFrom::Start(start)).await.is_err() {
            return (StatusCode::RANGE_NOT_SATISFIABLE, "Range Not Satisfiable").into_response();
        }

        let stream = ReaderStream::new(file.take(part_len));
        let body = Body::from_stream(stream);

        let mut resp = (StatusCode::PARTIAL_CONTENT, body).into_response();
        let h = resp.headers_mut();
        h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        h.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(&content_type)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        );
        h.insert(
            CONTENT_LENGTH,
            HeaderValue::from_str(&part_len.to_string()).unwrap_or(HeaderValue::from_static("0")),
        );
        h.insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size)).unwrap(),
        );
        h.insert(CONTENT_DISPOSITION, disposition.clone());
        return resp;
    }

    // Full response
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut resp = (StatusCode::OK, body).into_response();
    let h = resp.headers_mut();
    h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&file_size.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );
    h.insert(CONTENT_DISPOSITION, disposition);
    resp
}

fn safe_content_disposition(filename: &str, as_attachment: bool) -> HeaderValue {
    let disp_type = if as_attachment {
        "attachment"
    } else {
        "inline"
    };
    let encoded = encode_url_path(filename);
    let ascii_fallback: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let header_str =
        format!("{disp_type}; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}");
    HeaderValue::from_str(&header_str)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"file\""))
}

fn parse_range(range: &str, total: u64) -> Option<(u64, u64)> {
    let bytes_prefix = "bytes=";
    if !range.starts_with(bytes_prefix) {
        return None;
    }
    let s = &range[bytes_prefix.len()..];
    let mut parts = s.split('-');
    let start_str = parts.next()?.trim();
    let end_str = parts.next()?.trim();

    if start_str.is_empty() {
        // Suffix range: -N means last N bytes
        let len: u64 = end_str.parse().ok()?;
        let start = total.saturating_sub(len);
        Some((start, total.saturating_sub(1)))
    } else {
        let start: u64 = start_str.parse().ok()?;
        let end = if end_str.is_empty() {
            total.saturating_sub(1)
        } else {
            end_str.parse().ok()?
        };
        if start <= end && start < total {
            Some((start, end.min(total - 1)))
        } else {
            None
        }
    }
}
