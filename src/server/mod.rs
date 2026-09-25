pub mod auth;
pub mod fs;
pub mod preview;
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

    let app = build_app(state);

    let addr: SocketAddr =
        format!("{}:{}", config.scheme.address, config.scheme.http_port).parse()?;
    info!("start HTTP server @ {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

pub fn build_app(state: SharedState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health, favicon, manifest, robots
        .route("/ping", get(crate::static_files::ping_handler))
        .route("/favicon.ico", get(crate::static_files::favicon_handler))
        .route("/robots.txt", get(crate::static_files::robots_handler))
        .route("/manifest.json", get(crate::static_files::manifest_handler))
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
        .route(
            "/api/auth/2fa/disable",
            post(auth::two_factor_disable_handler),
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
            post(fs::fs_remove_empty_dirs_handler),
        )
        .route("/api/fs/put", put(fs::fs_put_handler))
        // File system batch & link
        .route("/api/fs/batch_rename", post(fs::fs_batch_rename_handler))
        .route("/api/fs/link", post(fs::fs_link_handler))
        .route(
            "/api/fs/preview",
            post(preview::preview_handler).get(preview::preview_handler),
        )
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
            post(users::admin_user_delete_handler),
        )
        .route(
            "/api/admin/user/cancel_2fa",
            post(users::admin_user_cancel_2fa_handler),
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

pub(crate) fn api_error<T: Into<String>>(status: StatusCode, code: i32, msg: T) -> Response {
    (status, Json(ApiResponse::<()>::error(code, msg))).into_response()
}

pub(crate) fn api_success<T: serde::Serialize>(data: T) -> Response {
    Json(ApiResponse::success(data)).into_response()
}

pub(crate) fn permission_denied() -> Response {
    api_error(StatusCode::FORBIDDEN, 403, "Permission denied")
}

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

    // Check if token matches admin token
    if let Ok(Some(admin_token)) = get_setting(&state.pool, "token").await
        && !admin_token.is_empty()
        && admin_token.as_bytes().ct_eq(token.as_bytes()).into()
    {
        return get_admin(&state.pool).await.ok().flatten();
    }

    // Parse JWT
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

    if user.disabled || user.pwd_ts != claims.pwd_ts || (user.password_unset && !allow_unset) {
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
        AdminUserSaveReq, BatchRenameItem, BatchRenameReq, ConflictPolicy, FsListReq,
        FsMoveCopyReq, FsRecursiveMoveReq, FsRemoveEmptyDirsReq, FsRenameReq, LoginReq,
        TwoFaGenerateReq, TwoFaVerifyReq, UpdateCurrentReq,
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
            password_unset: false,
            otp_secret: None,
            last_otp_step: -1,
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
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
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

        // 6b. Login with wrong password -> should return code 401 and StatusCode::UNAUTHORIZED
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "WrongPassword!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 401);
        assert_eq!(json["message"], "invalid username or password");

        // 7. Login without OTP code -> should return code 402 (OTP required)
        let login_req = LoginReq {
            username: "admin".to_string(),
            password: "TestPass123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(login_req)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
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
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
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

        // 10. The owner cancels 2FA with a current code
        let current_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;
        let resp = auth::two_factor_disable_handler(
            headers.clone(),
            State(state.clone()),
            Json(TwoFaVerifyReq {
                code: crate::auth::compute_totp(&secret, current_step).unwrap(),
            }),
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
        let salt = crate::auth::rand_string(16);
        let pwd_hash =
            crate::auth::encode_argon2_hash(&crate::auth::static_hash("OldPassword123!"), &salt);
        sqlx::query("INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`) VALUES ('account', ?, 0, ?)")
            .bind(pwd_hash)
            .bind(salt)
            .execute(&pool)
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
            username: "account".to_string(),
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
            username: Some("updated_account".to_string()),
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
            username: Some("updated_account".to_string()),
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
            username: Some("updated_account".to_string()),
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
        let user = crate::db::get_user_by_name(&pool, "updated_account")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.username, "updated_account");

        // Obtain new token for the renamed user
        let login_req = LoginReq {
            username: "updated_account".to_string(),
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
            username: "updated_account".to_string(),
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
        tokio::fs::write(
            storage_root.join("src/.hidden_root.txt"),
            b"hidden root content",
        )
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
        assert!(!storage_root.join("src/.hidden_root.txt").exists());
        assert_eq!(
            tokio::fs::read(storage_root.join("dst/.hidden_root.txt"))
                .await
                .unwrap(),
            b"hidden root content"
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
            directory_path: None,
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
            directory_path: None,
            role: Some(0),
            permission: Some(10),
            disabled: Some(false),
            local_path: Some(user_home.to_str().unwrap().to_string()),
        };
        let resp =
            users::admin_user_create_handler(headers.clone(), State(state.clone()), Json(dup_req))
                .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 409);

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

        // API cannot create a second admin user.
        let second_admin_req = AdminUserSaveReq {
            id: None,
            username: "second_admin".to_string(),
            password: Some("Admin2Pass123!".to_string()),
            directory_path: None,
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
        assert_eq!(json["code"], 400);

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

    #[tokio::test]
    async fn test_prevent_destructive_self_overwrite_during_copy_and_move() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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

        // Setup test files:
        // /local/a.txt
        // /local/dir/test.txt
        // /local/dir/sub/
        let a_txt = storage_root.join("a.txt");
        tokio::fs::write(&a_txt, b"source a.txt content")
            .await
            .unwrap();

        let dir_path = storage_root.join("dir");
        let dir_sub = dir_path.join("sub");
        tokio::fs::create_dir_all(&dir_sub).await.unwrap();
        tokio::fs::write(dir_path.join("test.txt"), b"test inside dir")
            .await
            .unwrap();

        // Case 1: move /a.txt -> current dir with overwrite
        let move_self = FsMoveCopyReq {
            src_dir: "/local".to_string(),
            dst_dir: "/local".to_string(),
            names: vec!["a.txt".to_string()],
            conflict_policy: ConflictPolicy::Overwrite,
        };
        let resp =
            fs::fs_move_handler(State(state.clone()), headers.clone(), Json(move_self)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(a_txt.exists());
        assert_eq!(
            tokio::fs::read(&a_txt).await.unwrap(),
            b"source a.txt content"
        );

        // Case 2: copy /a.txt -> current dir with overwrite
        let copy_self = FsMoveCopyReq {
            src_dir: "/local".to_string(),
            dst_dir: "/local".to_string(),
            names: vec!["a.txt".to_string()],
            conflict_policy: ConflictPolicy::Overwrite,
        };
        let resp =
            fs::fs_copy_handler(State(state.clone()), headers.clone(), Json(copy_self)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(a_txt.exists());
        assert_eq!(
            tokio::fs::read(&a_txt).await.unwrap(),
            b"source a.txt content"
        );

        // Case 3: move /dir -> /dir/sub
        let move_into_sub = FsMoveCopyReq {
            src_dir: "/local".to_string(),
            dst_dir: "/local/dir/sub".to_string(),
            names: vec!["dir".to_string()],
            conflict_policy: ConflictPolicy::Overwrite,
        };
        let resp =
            fs::fs_move_handler(State(state.clone()), headers.clone(), Json(move_into_sub)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(dir_path.join("test.txt").exists());

        // Case 4: copy /dir -> /dir/sub
        let copy_into_sub = FsMoveCopyReq {
            src_dir: "/local".to_string(),
            dst_dir: "/local/dir/sub".to_string(),
            names: vec!["dir".to_string()],
            conflict_policy: ConflictPolicy::Overwrite,
        };
        let resp =
            fs::fs_copy_handler(State(state.clone()), headers.clone(), Json(copy_into_sub)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(dir_path.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_remove_empty_directory_deepest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

        sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `driver`, `addition`) VALUES (?, ?, ?)",
        )
        .bind("/local")
        .bind("Local")
        .bind(
            serde_json::json!({
                "root_folder_path": storage_root.to_str().unwrap(),
                "show_hidden": false
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

        // root/
        // ├── empty/
        // ├── empty2/
        // │   └── empty3/
        // ├── hidden-file/
        // │   └── .secret
        // ├── git-project/
        // │   └── .git/
        // │       └── config
        // ├── dsstore/
        // │   └── .DS_Store
        // └── normal/
        //     └── file.txt
        let root = storage_root.join("root");
        let empty = root.join("empty");
        let empty2 = root.join("empty2");
        let empty3 = empty2.join("empty3");
        let hidden_file_dir = root.join("hidden-file");
        let git_project_dir = root.join("git-project/.git");
        let dsstore_dir = root.join("dsstore");
        let normal_dir = root.join("normal");

        tokio::fs::create_dir_all(&empty).await.unwrap();
        tokio::fs::create_dir_all(&empty3).await.unwrap();
        tokio::fs::create_dir_all(&hidden_file_dir).await.unwrap();
        tokio::fs::create_dir_all(&git_project_dir).await.unwrap();
        tokio::fs::create_dir_all(&dsstore_dir).await.unwrap();
        tokio::fs::create_dir_all(&normal_dir).await.unwrap();

        tokio::fs::write(hidden_file_dir.join(".secret"), b"secret data")
            .await
            .unwrap();
        tokio::fs::write(git_project_dir.join("config"), b"git config")
            .await
            .unwrap();
        tokio::fs::write(dsstore_dir.join(".DS_Store"), b"ds store")
            .await
            .unwrap();
        tokio::fs::write(normal_dir.join("file.txt"), b"normal file")
            .await
            .unwrap();

        let req = FsRemoveEmptyDirsReq {
            src_dir: "/local/root".to_string(),
        };
        let resp =
            fs::fs_remove_empty_dirs_handler(State(state.clone()), headers.clone(), Json(req))
                .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // root must still exist
        assert!(root.exists());
        // empty and empty2/empty3 must be removed
        assert!(!empty.exists());
        assert!(!empty3.exists());
        assert!(!empty2.exists());
        // hidden data and normal files must be preserved
        assert!(hidden_file_dir.join(".secret").exists());
        assert!(git_project_dir.join("config").exists());
        assert!(dsstore_dir.join(".DS_Store").exists());
        assert!(normal_dir.join("file.txt").exists());
        assert!(hidden_file_dir.exists());
        assert!(root.join("git-project").exists());
        assert!(dsstore_dir.exists());
        assert!(normal_dir.exists());
    }

    #[tokio::test]
    async fn test_profile_update_is_atomic_on_conflict() {
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

        // Create alice and bob
        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        let alice_dir = tmp.path().join("alice");
        let bob_dir = tmp.path().join("bob");
        std::fs::create_dir_all(&alice_dir).unwrap();
        std::fs::create_dir_all(&bob_dir).unwrap();

        let alice_create = AdminUserSaveReq {
            id: None,
            username: "alice".to_string(),
            password: Some("AlicePassword123!".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(15),
            disabled: Some(false),
            local_path: Some(alice_dir.to_str().unwrap().to_string()),
        };
        let _ = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(alice_create),
        )
        .await;

        let bob_create = AdminUserSaveReq {
            id: None,
            username: "bob".to_string(),
            password: Some("BobPassword123!".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(15),
            disabled: Some(false),
            local_path: Some(bob_dir.to_str().unwrap().to_string()),
        };
        let _ = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(bob_create),
        )
        .await;

        // Login as alice
        let alice_login = LoginReq {
            username: "alice".to_string(),
            password: "AlicePassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(alice_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let alice_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut alice_headers = HeaderMap::new();
        alice_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", alice_token)).unwrap(),
        );

        // Alice attempts to change username to 'bob' (conflict) and password to 'NewPassword123!'
        let conflict_req = UpdateCurrentReq {
            username: Some("bob".to_string()),
            password: Some("NewPassword123!".to_string()),
            current_password: Some("AlicePassword123!".to_string()),
        };
        let resp = auth::update_current_handler(
            alice_headers.clone(),
            State(state.clone()),
            Json(conflict_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 409);

        // Verify username is still alice
        let alice_user = crate::db::get_user_by_name(&pool, "alice")
            .await
            .unwrap()
            .expect("alice must still exist");
        assert_eq!(alice_user.username, "alice");

        // Verify alice old password still works
        let alice_relogin_old = LoginReq {
            username: "alice".to_string(),
            password: "AlicePassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(alice_relogin_old)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);

        // Verify new password was NOT applied
        let alice_relogin_new = LoginReq {
            username: "alice".to_string(),
            password: "NewPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(alice_relogin_new)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_ne!(json["code"], 200);
    }

    #[tokio::test]
    async fn test_admin_user_invalid_local_path_rejected_before_persistence() {
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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // Attempt to create user with non-existent local path
        let invalid_req = AdminUserSaveReq {
            id: None,
            username: "bad_user".to_string(),
            password: Some("BadPassword123!".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(15),
            disabled: Some(false),
            local_path: Some("/non/existent/path/for/bad_user".to_string()),
        };
        let resp = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(invalid_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 400);

        // Normal-user passwords may be short.
        let short_pwd_req = AdminUserSaveReq {
            id: None,
            username: "short_pwd_user".to_string(),
            password: Some("short".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(15),
            disabled: Some(false),
            local_path: Some("/tmp".to_string()),
        };
        let resp = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(short_pwd_req),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(
            crate::db::get_user_by_name(&pool, "short_pwd_user")
                .await
                .unwrap()
                .is_some()
        );

        // Verify user was NOT persisted to DB
        assert!(
            crate::db::get_user_by_name(&pool, "bad_user")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_deterministic_rename_overwrite_semantics() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        let a_file = storage_root.join("a.txt");
        tokio::fs::write(&a_file, b"content of a").await.unwrap();

        // 1. rename a.txt -> a.txt + overwrite=true: must succeed and not delete source
        let rename_self = FsRenameReq {
            path: "/local/a.txt".to_string(),
            name: "a.txt".to_string(),
            overwrite: true,
        };
        let resp = fs::fs_rename_handler(
            State(state.clone()),
            admin_headers.clone(),
            Json(rename_self),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(a_file.exists());
        assert_eq!(tokio::fs::read(&a_file).await.unwrap(), b"content of a");

        // 2. create b.txt, then rename a.txt -> b.txt with overwrite=false: returns 409
        let b_file = storage_root.join("b.txt");
        tokio::fs::write(&b_file, b"content of b").await.unwrap();

        let rename_conflict = FsRenameReq {
            path: "/local/a.txt".to_string(),
            name: "b.txt".to_string(),
            overwrite: false,
        };
        let resp = fs::fs_rename_handler(
            State(state.clone()),
            admin_headers.clone(),
            Json(rename_conflict),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert!(a_file.exists());
        assert_eq!(tokio::fs::read(&a_file).await.unwrap(), b"content of a");
        assert!(b_file.exists());
        assert_eq!(tokio::fs::read(&b_file).await.unwrap(), b"content of b");

        // 3. rename a.txt -> b.txt with overwrite=true: succeeds, b.txt gets a's content, a.txt is removed
        let rename_overwrite = FsRenameReq {
            path: "/local/a.txt".to_string(),
            name: "b.txt".to_string(),
            overwrite: true,
        };
        let resp = fs::fs_rename_handler(
            State(state.clone()),
            admin_headers.clone(),
            Json(rename_overwrite),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["code"], 200);
        assert!(!a_file.exists());
        assert!(b_file.exists());
        assert_eq!(tokio::fs::read(&b_file).await.unwrap(), b"content of a");
    }

    #[tokio::test]
    async fn test_login_error_unification_and_normalized_status_codes() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        // Create a disabled user
        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash("DisabledPassword123!");
        let pwd_hash = crate::auth::encode_argon2_hash(&s_hash, &salt);
        sqlx::query(
            "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`) VALUES (?, ?, 0, ?, '/', 0, 1, 0)",
        )
        .bind("disabled_user")
        .bind(&pwd_hash)
        .bind(&salt)
        .execute(&pool)
        .await
        .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: Config::default(),
            storage: Arc::new(storage_mgr),
        });

        // Case 1: Non-existent user -> 401 UNAUTHORIZED, code 401, "invalid username or password"
        let nonexistent_req = LoginReq {
            username: "nonexistent".to_string(),
            password: "AnyPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(nonexistent_req)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 401);
        assert_eq!(json["message"], "invalid username or password");

        // Case 2: Disabled user -> 401 UNAUTHORIZED, code 401, "invalid username or password"
        let disabled_req = LoginReq {
            username: "disabled_user".to_string(),
            password: "DisabledPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(disabled_req)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 401);
        assert_eq!(json["message"], "invalid username or password");

        // Case 3: Wrong password -> 401 UNAUTHORIZED, code 401, "invalid username or password"
        let wrong_pwd_req = LoginReq {
            username: "admin".to_string(),
            password: "WrongPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(wrong_pwd_req)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 401);
        assert_eq!(json["message"], "invalid username or password");
    }

    #[tokio::test]
    async fn test_dangerous_get_admin_routes_are_rejected() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: Config::default(),
            storage: Arc::new(storage_mgr),
        });

        let app = build_app(state);

        // GET /api/admin/user/delete must return 405 Method Not Allowed
        let req = Request::builder()
            .method("GET")
            .uri("/api/admin/user/delete?id=2")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);

        // GET /api/admin/user/cancel_2fa must return 405 Method Not Allowed
        let req = Request::builder()
            .method("GET")
            .uri("/api/admin/user/cancel_2fa?id=2")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn test_unauthenticated_and_forbidden_api_error_normalization() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        // Create regular non-admin user
        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash("UserPass123!");
        let pwd_hash = crate::auth::encode_argon2_hash(&s_hash, &salt);
        sqlx::query(
            "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`) VALUES (?, ?, 0, ?, '/', 0, 0, 0)",
        )
        .bind("regular")
        .bind(&pwd_hash)
        .bind(&salt)
        .execute(&pool)
        .await
        .unwrap();

        let storage_mgr = StorageManager::load_from_db(&pool).await.unwrap();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: Config::default(),
            storage: Arc::new(storage_mgr),
        });

        // 1. /api/me without auth -> 401 UNAUTHORIZED, code 401
        let no_auth_headers = HeaderMap::new();
        let resp = auth::current_user_handler(State(state.clone()), no_auth_headers.clone()).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 401);

        // 2. /api/admin/user/list as non-admin -> 403 FORBIDDEN, code 403
        let regular_token =
            crate::auth::generate_jwt("regular", 0, &state.config.jwt_secret, 3600).unwrap();
        let mut regular_headers = HeaderMap::new();
        regular_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", regular_token)).unwrap(),
        );

        let resp = users::admin_user_list_handler(regular_headers, State(state.clone())).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 403);

        // 3. /api/fs/list without auth -> 401 UNAUTHORIZED, code 401
        let fs_req = crate::model::FsListReq {
            path: "/".to_string(),
            ..Default::default()
        };
        let resp = fs::fs_list_handler(no_auth_headers, State(state.clone()), Json(fs_req)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 401);
    }

    #[tokio::test]
    async fn test_enforce_per_user_storage_isolation() {
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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // 1. Attempt to create non-admin user with empty local_path -> 400 Bad Request
        let empty_path_req = AdminUserSaveReq {
            id: None,
            username: "user_no_dir".to_string(),
            password: Some("UserPass123!".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(255),
            disabled: Some(false),
            local_path: Some("".to_string()),
        };
        let resp = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(empty_path_req),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 400);
        assert_eq!(
            json["message"],
            "Local directory is required for non-admin users"
        );
        assert!(
            crate::db::get_user_by_name(&pool, "user_no_dir")
                .await
                .unwrap()
                .is_none()
        );

        // 2. Create user1 and user2 with distinct directories
        let user1_dir = tmp.path().join("user1_dir");
        let user2_dir = tmp.path().join("user2_dir");
        std::fs::create_dir_all(&user1_dir).unwrap();
        std::fs::create_dir_all(&user2_dir).unwrap();
        std::fs::write(user1_dir.join("file1.txt"), "hello from user1").unwrap();
        std::fs::write(user2_dir.join("file2.txt"), "hello from user2").unwrap();

        let user1_req = AdminUserSaveReq {
            id: None,
            username: "user1".to_string(),
            password: Some("UserPass123!".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(255),
            disabled: Some(false),
            local_path: Some(user1_dir.to_str().unwrap().to_string()),
        };
        let resp = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(user1_req),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let user2_req = AdminUserSaveReq {
            id: None,
            username: "user2".to_string(),
            password: Some("UserPass123!".to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(255),
            disabled: Some(false),
            local_path: Some(user2_dir.to_str().unwrap().to_string()),
        };
        let resp = users::admin_user_create_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(user2_req),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let user1 = crate::db::get_user_by_name(&pool, "user1")
            .await
            .unwrap()
            .unwrap();
        let user2 = crate::db::get_user_by_name(&pool, "user2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user1.base_path, format!("/.users/{}", user1.id));
        assert_eq!(user2.base_path, format!("/.users/{}", user2.id));

        // 3. Login as user1
        let user1_login = LoginReq {
            username: "user1".to_string(),
            password: "UserPass123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(user1_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let user1_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut user1_headers = HeaderMap::new();
        user1_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", user1_token)).unwrap(),
        );

        // user1 listing "/" only enters /.users/{user1.id}
        let list_req = crate::model::FsListReq {
            path: "/".to_string(),
            ..Default::default()
        };
        let resp =
            fs::fs_list_handler(user1_headers.clone(), State(state.clone()), Json(list_req)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], 200);
        let content = json["data"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["name"], "file1.txt");

        // user1 attempting to access user2's mount path directly "/.users/{user2.id}"
        let list_user2_req = crate::model::FsListReq {
            path: format!("/.users/{}", user2.id),
            ..Default::default()
        };
        let resp = fs::fs_list_handler(
            user1_headers.clone(),
            State(state.clone()),
            Json(list_user2_req),
        )
        .await;
        // Should resolve under /.users/{user1.id}/.users/{user2.id} and fail rather than returning user2's files
        assert_eq!(
            user_path(&user1, &format!("/.users/{}", user2.id)).unwrap(),
            format!("/.users/{}/.users/{}", user1.id, user2.id)
        );
        assert_ne!(resp.status(), StatusCode::OK);

        // 4. Verify safety migration for legacy non-admin user with base_path = "/"
        let salt = crate::auth::rand_string(16);
        let s_hash = crate::auth::static_hash("LegacyPass123!");
        let encoded_pwd = crate::auth::encode_argon2_hash(&s_hash, &salt);
        let legacy_id = sqlx::query(
            "INSERT INTO `x_users` (`username`, `pwd_hash`, `pwd_ts`, `salt`, `base_path`, `role`, `disabled`, `permission`) VALUES (?, ?, 0, ?, '/', 0, 0, 0)",
        )
        .bind("legacy_unsafe_user")
        .bind(&encoded_pwd)
        .bind(&salt)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        // Run db init again on this db_path to simulate restart / migration.
        let re_pool = crate::db::init_db(&db_path).await.unwrap();
        let migrated_user = crate::db::get_user_by_id(&re_pool, legacy_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(migrated_user.base_path, format!("/.users/{}", legacy_id));
        assert!(migrated_user.disabled);

        // 5. Verify updating admin does NOT change base_path to /.users/{id}
        let admin_user = crate::db::get_user_by_name(&pool, "admin")
            .await
            .unwrap()
            .unwrap();
        let admin_update_req = AdminUserSaveReq {
            id: Some(admin_user.id),
            username: "admin".to_string(),
            password: None,
            directory_path: None,
            role: Some(2),
            permission: Some(0),
            disabled: Some(false),
            local_path: Some(user1_dir.to_str().unwrap().to_string()),
        };
        let resp = users::admin_user_update_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(admin_update_req),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let updated_admin = crate::db::get_user_by_id(&pool, admin_user.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_admin.base_path, "/");
    }

    #[tokio::test]
    async fn test_batch_rename_rollback_safety_and_swaps() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        let a_file = storage_root.join("a.txt");
        let b_file = storage_root.join("b.txt");
        let c_file = storage_root.join("c.txt");
        let unpart_file = storage_root.join("unrelated.txt");

        tokio::fs::write(&a_file, b"content AAA").await.unwrap();
        tokio::fs::write(&b_file, b"content BBB").await.unwrap();
        tokio::fs::write(&c_file, b"content CCC").await.unwrap();
        tokio::fs::write(&unpart_file, b"content UNRELATED")
            .await
            .unwrap();

        // 1. Target already exists and does not belong to source set -> 409, no files moved
        let req_conflict = BatchRenameReq {
            src_dir: "/local".to_string(),
            rename_objects: vec![BatchRenameItem {
                src_name: "a.txt".to_string(),
                new_name: "unrelated.txt".to_string(),
            }],
        };
        let resp = fs::fs_batch_rename_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(req_conflict),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert_eq!(tokio::fs::read(&a_file).await.unwrap(), b"content AAA");
        assert_eq!(
            tokio::fs::read(&unpart_file).await.unwrap(),
            b"content UNRELATED"
        );

        // 2. Two sources to same target -> 409, no files moved
        let req_two_sources = BatchRenameReq {
            src_dir: "/local".to_string(),
            rename_objects: vec![
                BatchRenameItem {
                    src_name: "a.txt".to_string(),
                    new_name: "same_target.txt".to_string(),
                },
                BatchRenameItem {
                    src_name: "b.txt".to_string(),
                    new_name: "same_target.txt".to_string(),
                },
            ],
        };
        let resp = fs::fs_batch_rename_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(req_two_sources),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert_eq!(tokio::fs::read(&a_file).await.unwrap(), b"content AAA");
        assert_eq!(tokio::fs::read(&b_file).await.unwrap(), b"content BBB");
        assert!(!storage_root.join("same_target.txt").exists());

        // 3. Swap: a <-> b
        let req_swap = BatchRenameReq {
            src_dir: "/local".to_string(),
            rename_objects: vec![
                BatchRenameItem {
                    src_name: "a.txt".to_string(),
                    new_name: "b.txt".to_string(),
                },
                BatchRenameItem {
                    src_name: "b.txt".to_string(),
                    new_name: "a.txt".to_string(),
                },
            ],
        };
        let resp = fs::fs_batch_rename_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(req_swap),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(tokio::fs::read(&a_file).await.unwrap(), b"content BBB");
        assert_eq!(tokio::fs::read(&b_file).await.unwrap(), b"content AAA");

        // 4. Cycle rotation: a -> b, b -> c, c -> a
        // (Currently: a=BBB, b=AAA, c=CCC)
        let req_cycle = BatchRenameReq {
            src_dir: "/local".to_string(),
            rename_objects: vec![
                BatchRenameItem {
                    src_name: "a.txt".to_string(),
                    new_name: "b.txt".to_string(),
                },
                BatchRenameItem {
                    src_name: "b.txt".to_string(),
                    new_name: "c.txt".to_string(),
                },
                BatchRenameItem {
                    src_name: "c.txt".to_string(),
                    new_name: "a.txt".to_string(),
                },
            ],
        };
        let resp = fs::fs_batch_rename_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(req_cycle),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(tokio::fs::read(&b_file).await.unwrap(), b"content BBB");
        assert_eq!(tokio::fs::read(&c_file).await.unwrap(), b"content AAA");
        assert_eq!(tokio::fs::read(&a_file).await.unwrap(), b"content CCC");

        // 5. Case-only rename: foo.txt -> FOO.txt does not lose file
        let foo_file = storage_root.join("foo.txt");
        tokio::fs::write(&foo_file, b"content FOO").await.unwrap();
        let req_case = FsRenameReq {
            path: "/local/foo.txt".to_string(),
            name: "FOO.txt".to_string(),
            overwrite: false,
        };
        let resp =
            fs::fs_rename_handler(State(state.clone()), admin_headers.clone(), Json(req_case))
                .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let foo_final = storage_root.join("FOO.txt");
        assert!(foo_final.exists() || foo_file.exists());
        let read_back = if foo_final.exists() {
            tokio::fs::read(&foo_final).await.unwrap()
        } else {
            tokio::fs::read(&foo_file).await.unwrap()
        };
        assert_eq!(read_back, b"content FOO");
    }

    #[tokio::test]
    async fn test_concurrent_upload_overwrite_false_prevents_clobbering() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();

        let mut headers1 = HeaderMap::new();
        headers1.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );
        headers1.insert(
            "File-Path",
            HeaderValue::from_static("%2Flocal%2Fconcurrent.txt"),
        );
        headers1.insert("Overwrite", HeaderValue::from_static("false"));

        let headers2 = headers1.clone();

        let data1 = vec![b'A'; 65536];
        let data2 = vec![b'B'; 65536];

        let req1 = axum::http::Request::builder()
            .method("PUT")
            .body(axum::body::Body::from(data1.clone()))
            .unwrap();

        let req2 = axum::http::Request::builder()
            .method("PUT")
            .body(axum::body::Body::from(data2.clone()))
            .unwrap();

        let state1 = state.clone();
        let state2 = state.clone();

        let handle1 =
            tokio::spawn(async move { fs::fs_put_handler(State(state1), headers1, req1).await });
        let handle2 =
            tokio::spawn(async move { fs::fs_put_handler(State(state2), headers2, req2).await });

        let (resp1, resp2) = tokio::join!(handle1, handle2);
        let resp1 = resp1.unwrap();
        let resp2 = resp2.unwrap();

        let mut statuses = vec![resp1.status(), resp2.status()];
        statuses.sort();

        // Exactly one must succeed (200 OK) and one must be rejected (409 CONFLICT)
        assert_eq!(statuses, vec![StatusCode::OK, StatusCode::CONFLICT]);

        // Verify the file content on disk is completely intact and matches either data1 or data2
        let file_path = storage_root.join("concurrent.txt");
        assert!(file_path.exists());
        let content = tokio::fs::read(&file_path).await.unwrap();
        assert!(
            content == data1 || content == data2,
            "File content must be completely equal to one of the uploads"
        );
        assert_eq!(content.len(), 65536);

        // Verify no leftover upload temp files
        let mut entries = tokio::fs::read_dir(&storage_root).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with(".rulist-upload-"),
                "Leftover upload temp file: {}",
                name
            );
        }
    }

    #[tokio::test]
    async fn test_range_requests_boundary_and_416() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();

        let mut base_headers = HeaderMap::new();
        base_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        // 1. Empty file: Range request should return 416 with Content-Range: bytes */0
        let empty_path = storage_root.join("empty.txt");
        tokio::fs::write(&empty_path, b"").await.unwrap();

        let mut empty_headers = base_headers.clone();
        empty_headers.insert("range", HeaderValue::from_static("bytes=0-0"));
        let resp = stream::raw_download_handler(
            State(state.clone()),
            axum::extract::Path("local/empty.txt".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            empty_headers,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */0");

        // 2. Non-empty file (10 bytes: 0123456789)
        let test_path = storage_root.join("test.txt");
        tokio::fs::write(&test_path, b"0123456789").await.unwrap();

        // 2a. bytes=-0 should return 416 with Content-Range: bytes */10
        let mut neg_zero_headers = base_headers.clone();
        neg_zero_headers.insert("range", HeaderValue::from_static("bytes=-0"));
        let resp = stream::raw_download_handler(
            State(state.clone()),
            axum::extract::Path("local/test.txt".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            neg_zero_headers,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */10");

        // 2b. Start out of bounds (bytes=10-20 on 10-byte file) should return 416
        let mut oob_headers = base_headers.clone();
        oob_headers.insert("range", HeaderValue::from_static("bytes=10-20"));
        let resp = stream::raw_download_handler(
            State(state.clone()),
            axum::extract::Path("local/test.txt".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            oob_headers,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(resp.headers().get("content-range").unwrap(), "bytes */10");

        // 2c. Normal suffix range (bytes=-5 on 10-byte file) should return 206 with Content-Range: bytes 5-9/10
        let mut suffix_headers = base_headers.clone();
        suffix_headers.insert("range", HeaderValue::from_static("bytes=-5"));
        let resp = stream::raw_download_handler(
            State(state.clone()),
            axum::extract::Path("local/test.txt".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            suffix_headers,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(resp.headers().get("content-range").unwrap(), "bytes 5-9/10");
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body_bytes[..], b"56789");
    }

    #[tokio::test]
    async fn test_fs_list_pagination() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();

        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        for name in &["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
            tokio::fs::write(storage_root.join(name), b"test")
                .await
                .unwrap();
        }

        // 1. Page 1, per_page 2 -> [a.txt, b.txt], total 5
        let req_p1 = FsListReq {
            path: "/local".to_string(),
            page: Some(1),
            per_page: Some(2),
            ..Default::default()
        };
        let resp =
            fs::fs_list_handler(admin_headers.clone(), State(state.clone()), Json(req_p1)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["data"]["total"], 5);
        let items: Vec<String> = json["data"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(items, vec!["a.txt", "b.txt"]);

        // 2. Page 2, per_page 2 -> [c.txt, d.txt], total 5
        let req_p2 = FsListReq {
            path: "/local".to_string(),
            page: Some(2),
            per_page: Some(2),
            ..Default::default()
        };
        let resp =
            fs::fs_list_handler(admin_headers.clone(), State(state.clone()), Json(req_p2)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["data"]["total"], 5);
        let items: Vec<String> = json["data"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(items, vec!["c.txt", "d.txt"]);

        // 3. Page 3, per_page 2 -> [e.txt], total 5
        let req_p3 = FsListReq {
            path: "/local".to_string(),
            page: Some(3),
            per_page: Some(2),
            ..Default::default()
        };
        let resp =
            fs::fs_list_handler(admin_headers.clone(), State(state.clone()), Json(req_p3)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["data"]["total"], 5);
        let items: Vec<String> = json["data"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(items, vec!["e.txt"]);

        // 4. Page 4, per_page 2 -> [], total 5
        let req_p4 = FsListReq {
            path: "/local".to_string(),
            page: Some(4),
            per_page: Some(2),
            ..Default::default()
        };
        let resp =
            fs::fs_list_handler(admin_headers.clone(), State(state.clone()), Json(req_p4)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["data"]["total"], 5);
        assert_eq!(json["data"]["content"].as_array().unwrap().len(), 0);

        // 5. per_page: None or 0 -> all 5 items
        let req_all = FsListReq {
            path: "/local".to_string(),
            page: Some(1),
            per_page: Some(0),
            ..Default::default()
        };
        let resp =
            fs::fs_list_handler(admin_headers.clone(), State(state.clone()), Json(req_all)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["data"]["total"], 5);
        assert_eq!(json["data"]["content"].as_array().unwrap().len(), 5);

        // 6. Pagination with reverse sorting: Page 1, per_page 2, reverse true -> [e.txt, d.txt]
        let req_rev_p1 = FsListReq {
            path: "/local".to_string(),
            page: Some(1),
            per_page: Some(2),
            order_by: Some("name".to_string()),
            reverse: Some(true),
        };
        let resp = fs::fs_list_handler(
            admin_headers.clone(),
            State(state.clone()),
            Json(req_rev_p1),
        )
        .await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["data"]["total"], 5);
        let items: Vec<String> = json["data"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(items, vec!["e.txt", "d.txt"]);
    }

    #[tokio::test]
    async fn test_preview_csp_sandbox_header() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: Config::default(),
            storage: Arc::new(storage_mgr),
        });

        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token = json["data"]["token"].as_str().unwrap().to_string();

        let mut base_headers = HeaderMap::new();
        base_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token)).unwrap(),
        );

        let test_html_path = storage_root.join("test.html");
        tokio::fs::write(&test_html_path, b"<html><script>alert(1)</script></html>")
            .await
            .unwrap();

        // 1. Preview handler (/p): must return Content-Security-Policy: sandbox and inline Content-Disposition
        let preview_resp = stream::raw_preview_handler(
            State(state.clone()),
            axum::extract::Path("local/test.html".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            base_headers.clone(),
        )
        .await;
        assert_eq!(preview_resp.status(), StatusCode::OK);
        assert_eq!(
            preview_resp
                .headers()
                .get(axum::http::header::CONTENT_SECURITY_POLICY)
                .unwrap(),
            "sandbox"
        );
        let disp = preview_resp
            .headers()
            .get(axum::http::header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disp.starts_with("inline"));

        // 2. Download handler (/d): must NOT have Content-Security-Policy and must be attachment
        let download_resp = stream::raw_download_handler(
            State(state.clone()),
            axum::extract::Path("local/test.html".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            base_headers.clone(),
        )
        .await;
        assert_eq!(download_resp.status(), StatusCode::OK);
        assert!(
            download_resp
                .headers()
                .get(axum::http::header::CONTENT_SECURITY_POLICY)
                .is_none()
        );
        let dl_disp = download_resp
            .headers()
            .get(axum::http::header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(dl_disp.starts_with("attachment"));

        // 3. Partial content (Range) preview: must also include Content-Security-Policy: sandbox
        let mut range_headers = base_headers.clone();
        range_headers.insert("range", HeaderValue::from_static("bytes=0-10"));
        let range_preview_resp = stream::raw_preview_handler(
            State(state.clone()),
            axum::extract::Path("local/test.html".to_string()),
            axum::extract::Query(stream::SignQuery { sign: None }),
            range_headers,
        )
        .await;
        assert_eq!(range_preview_resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            range_preview_resp
                .headers()
                .get(axum::http::header::CONTENT_SECURITY_POLICY)
                .unwrap(),
            "sandbox"
        );
    }

    #[tokio::test]
    async fn test_same_second_password_change_invalidates_jwt() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::init_db(&db_path).await.unwrap();
        crate::db::set_admin_password(&pool, "AdminPassword123!")
            .await
            .unwrap();

        let storage_root = tmp.path().join("storage");
        tokio::fs::create_dir_all(&storage_root).await.unwrap();

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
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: Config::default(),
            storage: Arc::new(storage_mgr),
        });

        // Step 1: Admin logs in, gets token1
        let admin_login = LoginReq {
            username: "admin".to_string(),
            password: "AdminPassword123!".to_string(),
            otp_code: None,
        };
        let resp = auth::login_handler(State(state.clone()), Json(admin_login)).await;
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token1 = json["data"]["token"].as_str().unwrap().to_string();

        let mut headers1 = HeaderMap::new();
        headers1.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token1)).unwrap(),
        );
        assert!(authenticate_user(&headers1, &state).await.is_some());

        // Step 2: First password update via update_current_handler
        let req1 = UpdateCurrentReq {
            username: None,
            password: Some("Password2026_A!".to_string()),
            current_password: Some("AdminPassword123!".to_string()),
        };
        let resp1 =
            auth::update_current_handler(headers1.clone(), State(state.clone()), Json(req1)).await;
        assert_eq!(resp1.status(), StatusCode::OK);

        // token1 is invalidated immediately
        assert!(authenticate_user(&headers1, &state).await.is_none());

        // Step 3: Immediately login with new password to get token2
        let login2 = LoginReq {
            username: "admin".to_string(),
            password: "Password2026_A!".to_string(),
            otp_code: None,
        };
        let resp2 = auth::login_handler(State(state.clone()), Json(login2)).await;
        let body_bytes = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json2: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let token2 = json2["data"]["token"].as_str().unwrap().to_string();

        let mut headers2 = HeaderMap::new();
        headers2.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token2)).unwrap(),
        );
        assert!(authenticate_user(&headers2, &state).await.is_some());

        // Read pwd_ts after first update
        let admin_before_second = crate::db::get_admin(&pool).await.unwrap().unwrap();
        let ts_before = admin_before_second.pwd_ts;

        // Step 4: Immediately in the same second, update password again
        let req2 = UpdateCurrentReq {
            username: None,
            password: Some("Password2026_B!".to_string()),
            current_password: Some("Password2026_A!".to_string()),
        };
        let resp3 =
            auth::update_current_handler(headers2.clone(), State(state.clone()), Json(req2)).await;
        assert_eq!(resp3.status(), StatusCode::OK);

        let admin_after_second = crate::db::get_admin(&pool).await.unwrap().unwrap();
        let ts_after = admin_after_second.pwd_ts;

        // pwd_ts must be strictly greater than ts_before
        assert!(
            ts_after > ts_before,
            "pwd_ts must strictly increase even in same second: ts_after={}, ts_before={}",
            ts_after,
            ts_before
        );

        // Crucial check: token2 MUST now be invalidated because ts_after > token2's pwd_ts
        assert!(
            authenticate_user(&headers2, &state).await.is_none(),
            "token2 must be invalidated by immediate same-second password update"
        );

        // Step 5: Test admin_user_update_handler strictly increments pwd_ts
        let admin_login3 = LoginReq {
            username: "admin".to_string(),
            password: "Password2026_B!".to_string(),
            otp_code: None,
        };
        let resp_admin3 = auth::login_handler(State(state.clone()), Json(admin_login3)).await;
        let body_bytes = axum::body::to_bytes(resp_admin3.into_body(), usize::MAX)
            .await
            .unwrap();
        let json_admin3: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let admin_token3 = json_admin3["data"]["token"].as_str().unwrap().to_string();

        let mut admin_headers3 = HeaderMap::new();
        admin_headers3.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", admin_token3)).unwrap(),
        );

        let create_user_req = AdminUserSaveReq {
            id: None,
            username: "testuser".to_string(),
            password: Some("UserPass123!".to_string()),
            local_path: Some(storage_root.to_str().unwrap().to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(0),
            disabled: Some(false),
        };
        let resp_create = users::admin_user_create_handler(
            admin_headers3.clone(),
            State(state.clone()),
            Json(create_user_req),
        )
        .await;
        assert_eq!(resp_create.status(), StatusCode::OK);

        let created_user = crate::db::get_user_by_name(&pool, "testuser")
            .await
            .unwrap()
            .unwrap();

        // Login as testuser -> get user_token1
        let user_login1 = LoginReq {
            username: "testuser".to_string(),
            password: "UserPass123!".to_string(),
            otp_code: None,
        };
        let resp_u1 = auth::login_handler(State(state.clone()), Json(user_login1)).await;
        let body_bytes = axum::body::to_bytes(resp_u1.into_body(), usize::MAX)
            .await
            .unwrap();
        let json_u1: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let user_token1 = json_u1["data"]["token"].as_str().unwrap().to_string();

        let mut user_headers1 = HeaderMap::new();
        user_headers1.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", user_token1)).unwrap(),
        );
        assert!(authenticate_user(&user_headers1, &state).await.is_some());

        // Admin updates testuser's password
        let update_user_req1 = AdminUserSaveReq {
            id: Some(created_user.id),
            username: "testuser".to_string(),
            password: Some("UserPassNew1!".to_string()),
            local_path: Some(storage_root.to_str().unwrap().to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(0),
            disabled: Some(false),
        };
        let resp_u_update1 = users::admin_user_update_handler(
            admin_headers3.clone(),
            State(state.clone()),
            Json(update_user_req1),
        )
        .await;
        assert_eq!(resp_u_update1.status(), StatusCode::OK);

        // user_token1 is invalidated
        assert!(authenticate_user(&user_headers1, &state).await.is_none());

        // Login with new password -> user_token2
        let user_login2 = LoginReq {
            username: "testuser".to_string(),
            password: "UserPassNew1!".to_string(),
            otp_code: None,
        };
        let resp_u2 = auth::login_handler(State(state.clone()), Json(user_login2)).await;
        let body_bytes = axum::body::to_bytes(resp_u2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json_u2: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let user_token2 = json_u2["data"]["token"].as_str().unwrap().to_string();

        let mut user_headers2 = HeaderMap::new();
        user_headers2.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", user_token2)).unwrap(),
        );
        assert!(authenticate_user(&user_headers2, &state).await.is_some());

        // Immediately update password again via admin_user_update_handler
        let update_user_req2 = AdminUserSaveReq {
            id: Some(created_user.id),
            username: "testuser".to_string(),
            password: Some("UserPassNew2!".to_string()),
            local_path: Some(storage_root.to_str().unwrap().to_string()),
            directory_path: None,
            role: Some(0),
            permission: Some(0),
            disabled: Some(false),
        };
        let resp_u_update2 = users::admin_user_update_handler(
            admin_headers3.clone(),
            State(state.clone()),
            Json(update_user_req2),
        )
        .await;
        assert_eq!(resp_u_update2.status(), StatusCode::OK);

        // user_token2 must be invalidated
        assert!(
            authenticate_user(&user_headers2, &state).await.is_none(),
            "user_token2 must be invalidated by immediate same-second admin password update"
        );

        // Step 6: Test set_admin_password strictly increments pwd_ts
        let admin_before_reset = crate::db::get_admin(&pool).await.unwrap().unwrap();
        crate::db::set_admin_password(&pool, "ResetPass1!")
            .await
            .unwrap();
        let admin_after_reset1 = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(admin_after_reset1.pwd_ts > admin_before_reset.pwd_ts);

        crate::db::set_admin_password(&pool, "ResetPass2!")
            .await
            .unwrap();
        let admin_after_reset2 = crate::db::get_admin(&pool).await.unwrap().unwrap();
        assert!(admin_after_reset2.pwd_ts > admin_after_reset1.pwd_ts);
    }
}
