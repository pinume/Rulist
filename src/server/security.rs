use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;

use crate::auth::parse_jwt;
use crate::db::User;
use crate::db::get_user_by_name;

use super::AppState;

pub(crate) async fn authenticate_user(headers: &HeaderMap, state: &AppState) -> Option<User> {
    let auth_header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    let claims = parse_jwt(token, &state.config.security.jwt_secret).ok()?;
    if crate::db::is_token_revoked(&state.pool, &claims.jti)
        .await
        .ok()?
    {
        return None;
    }

    let user = get_user_by_name(&state.pool, &claims.username)
        .await
        .ok()
        .flatten()?;
    if (claims.user_id > 0 && user.id != claims.user_id)
        || user.disabled
        || user.pwd_ts != claims.pwd_ts
    {
        return None;
    }

    Some(user)
}

pub(crate) fn user_path(_user: &User, requested: &str) -> Result<String, &'static str> {
    if requested
        .split(|ch| ch == '/' || ch == '\\')
        .any(|part| part == "." || part == "..")
    {
        return Err("invalid path");
    }

    let relative = requested.trim_matches('/');
    Ok(if relative.is_empty() {
        "/".to_string()
    } else {
        format!("/{relative}")
    })
}

pub(crate) fn permitted(user: &User, bit: i32) -> bool {
    user.is_admin() || user.permission & (1 << bit) != 0
}

pub(crate) fn encode_url_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
