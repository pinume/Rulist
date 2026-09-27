use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use subtle::ConstantTimeEq;

use crate::auth::parse_jwt;
use crate::db::{get_admin, get_setting, get_user_by_name};
use crate::model::User;

use super::AppState;

pub(crate) async fn authenticate_user(headers: &HeaderMap, state: &AppState) -> Option<User> {
    authenticate_user_with_setup(headers, state, false).await
}

pub(crate) async fn authenticate_user_with_setup(
    headers: &HeaderMap,
    state: &AppState,
    allow_unset: bool,
) -> Option<User> {
    let auth_header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    if let Ok(Some(admin_token)) = get_setting(&state.pool, "token").await
        && !admin_token.is_empty()
        && admin_token.as_bytes().ct_eq(token.as_bytes()).into()
    {
        return get_admin(&state.pool).await.ok().flatten();
    }

    let claims = parse_jwt(token, &state.config.jwt_secret).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let revoked: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM `x_revoked_tokens` WHERE `jti` = ? AND `expires_at` >= ?",
    )
    .bind(&claims.jti)
    .bind(now)
    .fetch_optional(&state.pool)
    .await
    .ok()?;
    if revoked.is_some() {
        return None;
    }

    let user = get_user_by_name(&state.pool, &claims.username)
        .await
        .ok()
        .flatten()?;
    if (claims.user_id > 0 && user.id != claims.user_id)
        || user.disabled
        || user.pwd_ts != claims.pwd_ts
        || (user.is_admin() && user.password_unset && !allow_unset)
    {
        return None;
    }

    Some(user)
}

pub(crate) fn user_path(user: &User, requested: &str) -> Result<String, &'static str> {
    if requested
        .split(|ch| ch == '/' || ch == '\\')
        .any(|part| part == "." || part == "..")
    {
        return Err("invalid path");
    }

    let relative = requested.trim_start_matches('/');
    let base = user.base_path.trim_end_matches('/');
    Ok(if relative.is_empty() {
        if base.is_empty() {
            "/".to_string()
        } else {
            base.to_string()
        }
    } else {
        format!("{base}/{relative}")
    })
}

pub(crate) fn permitted(user: &User, bit: i32) -> bool {
    user.is_admin() || user.permission & (1 << bit) != 0
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
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
