pub mod auth;
pub mod fs;
pub mod stream;
pub mod users;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post, put};
use subtle::ConstantTimeEq;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::auth::parse_jwt;
use crate::config::Config;
use crate::db::{DbPool, get_admin, get_public_settings, get_setting, get_user_by_name};
use crate::driver::{SharedStorageManager, StorageManager};
use crate::model::{ApiResponse, User};

pub struct AppState {
    pub pool: DbPool,
    pub config: Config,
    pub storage: SharedStorageManager,
}

pub type SharedState = Arc<AppState>;

pub async fn run_server(
    config: Config,
    pool: DbPool,
    storage: StorageManager,
) -> Result<(), anyhow::Error> {
    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        storage: Arc::new(storage),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health, favicon, manifest, robots
        .route("/ping", get(crate::static_files::ping_handler))
        .route("/favicon.ico", get(crate::static_files::favicon_handler))
        .route("/robots.txt", get(crate::static_files::robots_handler))
        .route("/manifest.json", get(crate::static_files::manifest_handler))
        .route("/rulist.svg", get(crate::static_files::rulist_svg_handler))
        .route("/rulist.png", get(crate::static_files::rulist_png_handler))
        // Static assets from frontend dist
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
        // Settings
        .route(
            "/api/public/settings",
            get(public_settings_handler).post(public_settings_handler),
        )
        // Authentication
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/auth/me", get(auth::current_user_handler))
        .route("/api/me", get(auth::current_user_handler))
        .route("/api/me/update", post(auth::update_current_handler))
        .route(
            "/api/auth/logout",
            get(auth::logout_handler).post(auth::logout_handler),
        )
        .route(
            "/api/auth/2fa/generate",
            post(auth::two_factor_generate_handler),
        )
        .route(
            "/api/auth/2fa/verify",
            post(auth::two_factor_verify_handler),
        )
        // File system read
        .route(
            "/api/fs/list",
            post(fs::fs_list_handler).get(fs::fs_list_handler),
        )
        .route(
            "/api/fs/get",
            post(fs::fs_get_handler).get(fs::fs_get_handler),
        )
        .route(
            "/api/fs/dirs",
            post(fs::fs_dirs_handler).get(fs::fs_dirs_handler),
        )
        // File system write
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
            post(fs::fs_remove_handler),
        )
        .route("/api/fs/put", put(fs::fs_put_handler))
        // File system batch & link
        .route("/api/fs/batch_rename", post(fs::fs_batch_rename_handler))
        .route("/api/fs/link", post(fs::fs_link_handler))
        // Admin User Management
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
            post(users::admin_user_delete_handler).get(users::admin_user_delete_handler),
        )
        .route(
            "/api/admin/user/cancel_2fa",
            post(users::admin_user_cancel_2fa_handler).get(users::admin_user_cancel_2fa_handler),
        )
        // Direct download & streaming
        .route(
            "/d/{*path}",
            get(stream::raw_download_handler).head(stream::raw_download_handler),
        )
        .route(
            "/p/{*path}",
            get(stream::raw_preview_handler).head(stream::raw_preview_handler),
        )
        // SPA Fallback for all other routes
        .fallback(crate::static_files::spa_fallback_handler)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr =
        format!("{}:{}", config.scheme.address, config.scheme.http_port).parse()?;
    info!("start HTTP server @ {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn public_settings_handler(State(state): State<SharedState>) -> Response {
    match get_public_settings(&state.pool).await {
        Ok(settings) => api_success(settings),
        Err(err) => api_error(StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string()),
    }
}

pub(crate) fn api_error<T: Into<String>>(status: StatusCode, code: i32, msg: T) -> Response {
    (status, Json(ApiResponse::<()>::error(code, msg))).into_response()
}

pub(crate) fn api_success<T: serde::Serialize>(data: T) -> Response {
    Json(ApiResponse::success(data)).into_response()
}

pub(crate) fn permission_denied() -> Response {
    api_error(StatusCode::OK, 403, "Permission denied")
}

pub(crate) async fn authenticate_user(headers: &HeaderMap, state: &AppState) -> Option<User> {
    let auth_header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    // Check if token matches admin token
    if let Ok(Some(admin_token)) = get_setting(&state.pool, "token").await
        && !admin_token.is_empty()
        && admin_token.as_bytes().ct_eq(token.as_bytes()).into()
    {
        return get_admin(&state.pool).await.ok().flatten();
    }

    // Parse JWT
    let claims = parse_jwt(token, &state.config.jwt_secret).ok()?;
    let user = get_user_by_name(&state.pool, &claims.username)
        .await
        .ok()
        .flatten()?;

    if user.disabled || user.pwd_ts != claims.pwd_ts {
        return None;
    }

    Some(user)
}

pub(crate) fn user_path(user: &User, requested: &str) -> Result<String, &'static str> {
    if requested
        .split(['/', '\\'])
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
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AdminUserSaveReq, ConflictPolicy, FsRecursiveMoveReq, LoginReq, TwoFaGenerateReq,
        TwoFaVerifyReq, UpdateCurrentReq,
    };
    use axum::http::HeaderValue;

    #[test]
    fn user_paths_stay_under_base_and_permissions_are_enforced() {
        let user = User {
            id: 2,
            username: "alice".into(),
            pwd_hash: String::new(),
            pwd_ts: 0,
            salt: String::new(),
            password: None,
            base_path: "/.users/2".into(),
            role: 0,
            disabled: false,
            permission: 0,
            otp_secret: None,
            sso_id: None,
            otp: false,
        };
        assert_eq!(
            user_path(&user, "/secret.txt").unwrap(),
            "/.users/2/secret.txt"
        );
        assert_eq!(user_path(&user, "/").unwrap(), "/.users/2");
        assert!(user_path(&user, "../Local/secret.txt").is_err());
        assert!(user_path(&user, "/foo/../secret.txt").is_err());
        assert!(user_path(&user, "/foo/./secret.txt").is_err());
        assert!(!permitted(&user, 3));
        assert!(!permitted(&user, 7));
        assert!(!valid_name(""));
        assert!(!valid_name("../Local"));
        assert_eq!(encode_url_path("/a/hello #%.txt"), "/a/hello%20%23%25.txt");
    }

    #[tokio::test]
    async fn test_2fa_lifecycle_and_login_enforcement() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        let config = Config::default();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // 1. Get initial admin user and admin token
        crate::db::set_admin_password(&pool, "TestPass123!")
            .await
            .unwrap();
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(!admin.otp);

        let admin_token = crate::db::get_setting(&pool, "token")
            .await
            .unwrap()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // 2. Request 2FA generation with wrong password -> should fail
        let bad_gen_req = TwoFaGenerateReq {
            current_password: "WrongPassword!".to_string(),
        };
        let resp = auth::two_factor_generate_handler(
            headers.clone(),
            State(state.clone()),
            Json(bad_gen_req),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 403);

        // 2b. Request 2FA generation with correct password -> succeeds
        let gen_req = TwoFaGenerateReq {
            current_password: "TestPass123!".to_string(),
        };
        let resp =
            auth::two_factor_generate_handler(headers.clone(), State(state.clone()), Json(gen_req))
                .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let secret = json["data"]["secret"].as_str().unwrap().to_string();
        let qr = json["data"]["qr"].as_str().unwrap().to_string();
        assert!(qr.starts_with("data:image/svg+xml;base64,"));

        // 3. Verify with invalid code -> should fail
        let verify_req = TwoFaVerifyReq {
            code: "999999".to_string(),
        };
        let resp = auth::two_factor_verify_handler(
            headers.clone(),
            State(state.clone()),
            Json(verify_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 4. Verify with valid TOTP code -> should succeed
        let now_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;
        let valid_code = crate::auth::compute_totp(&secret, now_step).unwrap();
        let verify_req = TwoFaVerifyReq { code: valid_code };
        let resp = auth::two_factor_verify_handler(
            headers.clone(),
            State(state.clone()),
            Json(verify_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // 5. Check user in DB now has otp = true
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(admin.otp);

        // 6. Generating again should fail as 2FA is already enabled
        let gen_again = TwoFaGenerateReq {
            current_password: "TestPass123!".to_string(),
        };
        let resp = auth::two_factor_generate_handler(
            headers.clone(),
            State(state.clone()),
            Json(gen_again),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 6b. Login with wrong password -> should return code 400
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "WrongPassword!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 7. Login without OTP code -> should return code 402 (OTP required)
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 402);

        // 8. Login with invalid OTP code -> should return code 400
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: Some("000000".to_string()),
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 9. Login with valid OTP code -> should succeed (code 200)
        let cur_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;
        let valid_code = crate::auth::compute_totp(&secret, cur_step).unwrap();
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: Some(valid_code),
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(json["data"]["token"].is_string());

        // 10. Admin cancel 2FA
        let cancel_query = users::IdQuery { id: Some(admin.id) };
        let resp = users::admin_user_cancel_2fa_handler(
            headers.clone(),
            axum::extract::Query(cancel_query),
            State(state.clone()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify 2FA is now cancelled
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(!admin.otp);

        // Login without OTP code succeeds again
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
    }

    #[tokio::test]
    async fn client_cannot_choose_otp_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "TestPass123!")
            .await
            .unwrap();
        let config = Config::default();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        let admin_token = crate::db::get_setting(&pool, "token")
            .await
            .unwrap()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // Attacker has their own secret A
        let secret_a = crate::auth::generate_otp_secret();

        // Server generates secret B and saves to x_otp_pending
        let gen_req = TwoFaGenerateReq {
            current_password: "TestPass123!".to_string(),
        };
        let resp =
            auth::two_factor_generate_handler(headers.clone(), State(state.clone()), Json(gen_req))
                .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let secret_b = json["data"]["secret"].as_str().unwrap().to_string();
        assert_ne!(secret_a, secret_b);

        let now_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;

        // Attacker computes code from their chosen secret A and calls verify -> MUST FAIL
        let code_a = crate::auth::compute_totp(&secret_a, now_step).unwrap();
        let verify_req_a = TwoFaVerifyReq { code: code_a };
        let resp = auth::two_factor_verify_handler(
            headers.clone(),
            State(state.clone()),
            Json(verify_req_a),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // Verification with valid code from secret B succeeds
        let code_b = crate::auth::compute_totp(&secret_b, now_step).unwrap();
        let verify_req_b = TwoFaVerifyReq { code: code_b };
        let resp = auth::two_factor_verify_handler(
            headers.clone(),
            State(state.clone()),
            Json(verify_req_b),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert_eq!(admin.otp_secret.as_deref(), Some(secret_b.as_str()));
    }

    #[tokio::test]
    async fn test_sensitive_account_changes_require_current_password() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "OldPassword123!")
            .await
            .unwrap();
        let config = Config::default();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // Login to get valid JWT token
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "OldPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let token = json["data"]["token"].as_str().unwrap().to_string();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );

        // 1. JWT + 无 current_password 修改用户名 → 失败 (400)
        let req1 = UpdateCurrentReq {
            username: Some("newadmin".to_string()),
            password: None,
            current_password: None,
        };
        let resp =
            auth::update_current_handler(headers.clone(), State(state.clone()), Json(req1)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 2. JWT + 错 current_password 修改用户名 → 失败 (403)
        let req2 = UpdateCurrentReq {
            username: Some("newadmin".to_string()),
            password: None,
            current_password: Some("WrongPass123!".to_string()),
        };
        let resp =
            auth::update_current_handler(headers.clone(), State(state.clone()), Json(req2)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 403);

        // 3. JWT + 正确 current_password 修改用户名 → 成功 (200)
        let req3 = UpdateCurrentReq {
            username: Some("newadmin".to_string()),
            password: None,
            current_password: Some("OldPassword123!".to_string()),
        };
        let resp =
            auth::update_current_handler(headers.clone(), State(state.clone()), Json(req3)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // Verify username updated in DB
        let admin = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert_eq!(admin.username, "newadmin");

        // Obtain new token for newadmin
        let login_req = LoginReq {
            username: "newadmin".to_string(),
            password: "OldPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        let new_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut new_headers = HeaderMap::new();
        new_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", new_token)).unwrap(),
        );

        // 4. JWT + 正确旧密码修改密码 → 成功 (200)
        let req4 = UpdateCurrentReq {
            username: None,
            password: Some("NewPassword123!".to_string()),
            current_password: Some("OldPassword123!".to_string()),
        };
        let resp =
            auth::update_current_handler(new_headers.clone(), State(state.clone()), Json(req4))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // 5. 修改密码后旧 JWT → 无效 (authenticate_user returns None)
        assert!(authenticate_user(&new_headers, &state).await.is_none());

        // Login with new password succeeds
        let login_req = LoginReq {
            username: "newadmin".to_string(),
            password: "NewPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
    }

    #[tokio::test]
    async fn test_fs_conflict_policies_and_recursive_move_hierarchy() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        // Create storage directory structure
        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

        // Insert storage record in DB
        sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `driver`, `addition`) VALUES (?, ?, ?)",
        )
        .bind("/local")
        .bind("Local")
        .bind(
            serde_json::json!({
                "root_folder_path": storage_root.to_str().unwrap()
            })
            .to_string(),
        )
        .execute(&pool)
        .await
        .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let config = Config::default();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // Get admin token
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token = json["data"]["token"].as_str().unwrap().to_string();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );

        // Setup directories:
        // /local/src/sub/file1.txt ("file1 v1")
        // /local/src/sub/file2.txt ("file2 v1")
        // /local/src/root.txt ("root v1")
        // /local/dst/sub/file1.txt ("existing dst file1")
        let src_dir = storage_root.join("src/sub");
        tokio::fs::create_dir_all(&src_dir).await.unwrap();
        tokio::fs::write(src_dir.join("file1.txt"), b"file1 v1")
            .await
            .unwrap();
        tokio::fs::write(src_dir.join("file2.txt"), b"file2 v1")
            .await
            .unwrap();
        tokio::fs::write(storage_root.join("src/root.txt"), b"root v1")
            .await
            .unwrap();

        let dst_sub = storage_root.join("dst/sub");
        tokio::fs::create_dir_all(&dst_sub).await.unwrap();
        tokio::fs::write(dst_sub.join("file1.txt"), b"existing dst file1")
            .await
            .unwrap();

        // 1. Test recursive_move with ConflictPolicy::Cancel
        // Because /local/dst/sub/file1.txt already exists, it must return 403, and move NO files!
        let req_cancel = FsRecursiveMoveReq {
            src_dir: "/local/src".to_string(),
            dst_dir: "/local/dst".to_string(),
            conflict_policy: ConflictPolicy::Cancel,
        };
        let resp =
            fs::fs_recursive_move_handler(State(state.clone()), headers.clone(), Json(req_cancel))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 403);

        // Verify NOTHING was moved
        assert!(src_dir.join("file1.txt").exists());
        assert!(src_dir.join("file2.txt").exists());
        assert!(storage_root.join("src/root.txt").exists());
        assert_eq!(
            tokio::fs::read(dst_sub.join("file1.txt")).await.unwrap(),
            b"existing dst file1"
        );
        assert!(!dst_sub.join("file2.txt").exists());
        assert!(!storage_root.join("dst/root.txt").exists());

        // 2. Test recursive_move with ConflictPolicy::Skip
        // Skips file1.txt, moves file2.txt and root.txt, PRESERVING nested sub/ directory structure!
        let req_skip = FsRecursiveMoveReq {
            src_dir: "/local/src".to_string(),
            dst_dir: "/local/dst".to_string(),
            conflict_policy: ConflictPolicy::Skip,
        };
        let resp =
            fs::fs_recursive_move_handler(State(state.clone()), headers.clone(), Json(req_skip))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // Existing dst file1 was kept intact
        assert_eq!(
            tokio::fs::read(dst_sub.join("file1.txt")).await.unwrap(),
            b"existing dst file1"
        );
        // Skipped source file1 remains in src
        assert!(src_dir.join("file1.txt").exists());
        // Moved files are now in dst with preserved hierarchy
        assert!(!src_dir.join("file2.txt").exists());
        assert_eq!(
            tokio::fs::read(dst_sub.join("file2.txt")).await.unwrap(),
            b"file2 v1"
        );
        assert!(!storage_root.join("src/root.txt").exists());
        assert_eq!(
            tokio::fs::read(storage_root.join("dst/root.txt"))
                .await
                .unwrap(),
            b"root v1"
        );

        // 3. Test recursive_move with ConflictPolicy::Overwrite
        // Overwrites dst/sub/file1.txt with src/sub/file1.txt
        let req_overwrite = FsRecursiveMoveReq {
            src_dir: "/local/src".to_string(),
            dst_dir: "/local/dst".to_string(),
            conflict_policy: ConflictPolicy::Overwrite,
        };
        let resp = fs::fs_recursive_move_handler(
            State(state.clone()),
            headers.clone(),
            Json(req_overwrite),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // dst/sub/file1.txt is now overwritten with file1 v1
        assert_eq!(
            tokio::fs::read(dst_sub.join("file1.txt")).await.unwrap(),
            b"file1 v1"
        );
        assert!(!src_dir.join("file1.txt").exists());
    }

    #[tokio::test]
    async fn test_user_administration_transactional_and_last_admin_protection() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let config = Config::default();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            storage: Arc::new(storage_mgr),
        });

        // Get admin token
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token = json["data"]["token"].as_str().unwrap().to_string();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        );

        // 1. 普通用户创建：user + permissions + base_path 均成功
        let user_home = tmp.path().join("user_home");
        tokio::fs::create_dir_all(&user_home).await.unwrap();
        let create_req = AdminUserSaveReq {
            id: None,
            username: "regular_user".to_string(),
            password: Some("UserPass123!".to_string()),
            role: Some(0),
            permission: Some(255),
            disabled: Some(false),
            local_path: Some(user_home.to_str().unwrap().to_string()),
        };
        let resp = users::admin_user_create_handler(
            headers.clone(),
            State(state.clone()),
            Json(create_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        let created_user = crate::db::get_user_by_name(&pool, "regular_user")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(created_user.permission, 255);
        assert_eq!(created_user.role, 0);
        assert_eq!(
            created_user.base_path,
            format!("/.users/{}", created_user.id)
        );
        let storage_row: Option<i64> =
            sqlx::query_scalar("SELECT id FROM x_storages WHERE mount_path = ?")
                .bind(&created_user.base_path)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(storage_row.is_some());

        // 2. 中途失败：事务回滚，无孤立 user 记录
        // Trying to create a user with duplicate username
        let dup_req = AdminUserSaveReq {
            id: None,
            username: "regular_user".to_string(),
            password: Some("AnotherPass123!".to_string()),
            role: Some(0),
            permission: Some(10),
            disabled: Some(false),
            local_path: None,
        };
        let resp =
            users::admin_user_create_handler(headers.clone(), State(state.clone()), Json(dup_req))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // 3. 最后一个 admin 删除保护：必须返回错误，不能删空管理员
        // Trying to delete admin (id=1)
        let resp = users::admin_user_delete_handler(
            headers.clone(),
            axum::extract::Query(users::IdQuery { id: Some(1) }),
            State(state.clone()),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // Create a second admin user
        let second_admin_req = AdminUserSaveReq {
            id: None,
            username: "second_admin".to_string(),
            password: Some("Admin2Pass123!".to_string()),
            role: Some(crate::model::ROLE_ADMIN),
            permission: Some(0),
            disabled: Some(false),
            local_path: None,
        };
        let resp = users::admin_user_create_handler(
            headers.clone(),
            State(state.clone()),
            Json(second_admin_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        let second_admin = crate::db::get_user_by_name(&pool, "second_admin")
            .await
            .unwrap()
            .unwrap();
        assert!(second_admin.is_admin());

        // Deleting second admin succeeds because admin (id=1) still exists
        let resp = users::admin_user_delete_handler(
            headers.clone(),
            axum::extract::Query(users::IdQuery {
                id: Some(second_admin.id),
            }),
            State(state.clone()),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(
            crate::db::get_user_by_id(&pool, second_admin.id)
                .await
                .unwrap()
                .is_none()
        );

        // Now delete regular_user and verify user + storage cleanup
        let resp = users::admin_user_delete_handler(
            headers.clone(),
            axum::extract::Query(users::IdQuery {
                id: Some(created_user.id),
            }),
            State(state.clone()),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(
            crate::db::get_user_by_id(&pool, created_user.id)
                .await
                .unwrap()
                .is_none()
        );
        let storage_row_after: Option<i64> =
            sqlx::query_scalar("SELECT id FROM x_storages WHERE mount_path = ?")
                .bind(&created_user.base_path)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(storage_row_after.is_none());
    }
}
