use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use rulist::auth::{compute_totp, generate_otp_secret};
use rulist::config::Config;
use rulist::db;
use rulist::driver::StorageManager;
use rulist::model::PERM_ALLOW_EMPTY_PASSWORD;
use rulist::server::{AppState, build_app};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn json_request(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap();
    (status, body)
}

async fn app_for(pool: &db::DbPool) -> axum::Router {
    let storage = StorageManager::load_from_db(pool).await.unwrap();
    build_app(Arc::new(AppState {
        pool: pool.clone(),
        config: Config::default(),
        storage,
    }))
}

async fn login_token(app: &axum::Router, username: &str, password: &str) -> String {
    let (status, body) = json_request(
        app,
        "POST",
        "/api/auth/login",
        None,
        json!({ "username": username, "password": password }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], 200);
    body["data"]["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn two_factor_login_is_enforced_and_replay_safe() {
    let temp = tempfile::tempdir().unwrap();
    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    db::set_admin_password(&pool, "TestPass123!").await.unwrap();

    let admin = db::get_admin(&pool).await.unwrap().unwrap();
    let secret = generate_otp_secret();
    let step = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 30;
    db::enable_user_2fa(&pool, admin.id, &secret, step.saturating_sub(1) as i64)
        .await
        .unwrap();

    let app = app_for(&pool).await;

    let (status, missing_otp) = json_request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        json!({ "username": "admin", "password": "TestPass123!" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing_otp["code"], 402);

    let login_code = compute_totp(&secret, step).unwrap();
    let (status, logged_in) = json_request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        json!({
            "username": "admin",
            "password": "TestPass123!",
            "otp_code": login_code,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(logged_in["code"], 200);
    assert!(logged_in["data"]["token"].is_string());

    let replay_code = compute_totp(&secret, step).unwrap();
    let (status, replayed) = json_request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        json!({
            "username": "admin",
            "password": "TestPass123!",
            "otp_code": replay_code,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(replayed["code"], 400);
}

#[tokio::test]
async fn unknown_user_login_is_recorded_before_password_verification() {
    let temp = tempfile::tempdir().unwrap();
    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    let app = app_for(&pool).await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        json!({ "username": "does-not-exist", "password": "anything" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], 401);

    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM `x_login_attempts`")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn database_rejects_disabling_passwordless_for_unset_password() {
    let temp = tempfile::tempdir().unwrap();
    let user_root = temp.path().join("guarded-guest");
    tokio::fs::create_dir_all(&user_root).await.unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    let permission = 1 << PERM_ALLOW_EMPTY_PASSWORD;
    let user_id = db::create_user_direct(
        &pool,
        "guarded-guest",
        "",
        0,
        Some(user_root.to_str().unwrap()),
        permission,
        false,
    )
    .await
    .unwrap();

    let error = db::set_user_permission(&pool, user_id, 0)
        .await
        .unwrap_err();
    let err_msg = error.to_string();
    assert!(
        err_msg.contains("CHECK constraint failed") || err_msg.contains("non-empty password"),
        "unexpected error message: {err_msg}"
    );
}

#[tokio::test]
async fn signing_secret_is_not_an_admin_credential() {
    let temp = tempfile::tempdir().unwrap();
    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    let config = Config::default();
    let signing_secret = config.security.signing_secret.clone();
    let storage = StorageManager::load_from_db(&pool).await.unwrap();
    let app = build_app(Arc::new(AppState {
        pool: pool.clone(),
        config,
        storage,
    }));

    let (status, body) = json_request(
        &app,
        "GET",
        "/api/admin/user/list",
        Some(&signing_secret),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], 401);
}

#[tokio::test]
async fn disabling_passwordless_login_requires_a_nonempty_password() {
    let temp = tempfile::tempdir().unwrap();
    let user_root = temp.path().join("guest");
    tokio::fs::create_dir_all(&user_root).await.unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    db::set_admin_password(&pool, "AdminPass123!")
        .await
        .unwrap();
    let permission = 1 << PERM_ALLOW_EMPTY_PASSWORD;
    let user_id = db::create_user_direct(
        &pool,
        "guest",
        "",
        0,
        Some(user_root.to_str().unwrap()),
        permission,
        false,
    )
    .await
    .unwrap();
    let app = app_for(&pool).await;
    let admin_token = login_token(&app, "admin", "AdminPass123!").await;

    let (status, rejected) = json_request(
        &app,
        "POST",
        "/api/admin/user/update",
        Some(&admin_token),
        json!({
            "id": user_id,
            "username": "guest",
            "permission": 0
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(rejected["code"], 400);

    let (status, updated) = json_request(
        &app,
        "POST",
        "/api/admin/user/update",
        Some(&admin_token),
        json!({
            "id": user_id,
            "username": "guest",
            "password": "NewPass123!",
            "permission": 0
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["code"], 200);

    let user = db::get_user_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(!user.password_unset);
    assert_eq!(user.permission & (1 << PERM_ALLOW_EMPTY_PASSWORD), 0);

    let (status, _) = json_request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        json!({ "username": "guest", "password": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, logged_in) = json_request(
        &app,
        "POST",
        "/api/auth/login",
        None,
        json!({ "username": "guest", "password": "NewPass123!" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(logged_in["code"], 200);
}

#[tokio::test]
async fn admin_password_update_keeps_pwd_ts_monotonic() {
    let temp = tempfile::tempdir().unwrap();
    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    db::set_admin_password(&pool, "AdminPass123!")
        .await
        .unwrap();
    let admin = db::get_admin(&pool).await.unwrap().unwrap();
    let old_pwd_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 3600;
    sqlx::query("UPDATE `x_users` SET `pwd_ts` = ? WHERE `id` = ?")
        .bind(old_pwd_ts)
        .bind(admin.id)
        .execute(&pool)
        .await
        .unwrap();

    let app = app_for(&pool).await;
    let admin_token = login_token(&app, "admin", "AdminPass123!").await;
    let (status, updated) = json_request(
        &app,
        "POST",
        "/api/admin/user/update",
        Some(&admin_token),
        json!({
            "id": admin.id,
            "username": "admin",
            "password": "NextAdminPass123!"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["code"], 200);

    let after = db::get_admin(&pool).await.unwrap().unwrap();
    assert_eq!(after.pwd_ts, old_pwd_ts + 1);
}

#[tokio::test]
async fn user_update_rejects_invalid_local_path_without_mutating_user() {
    let temp = tempfile::tempdir().unwrap();
    let old_root = temp.path().join("old-root");
    let missing_root = temp.path().join("missing-root");
    tokio::fs::create_dir_all(&old_root).await.unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();
    db::set_admin_password(&pool, "AdminPass123!")
        .await
        .unwrap();
    let user_id = db::create_user_direct(
        &pool,
        "rollback-guest",
        "GuestPass123!",
        0,
        Some(old_root.to_str().unwrap()),
        0,
        false,
    )
    .await
    .unwrap();
    let before = db::get_user_by_id(&pool, user_id).await.unwrap().unwrap();

    let app = app_for(&pool).await;
    let admin_token = login_token(&app, "admin", "AdminPass123!").await;
    let (status, body) = json_request(
        &app,
        "POST",
        "/api/admin/user/update",
        Some(&admin_token),
        json!({
            "id": user_id,
            "username": "changed-guest",
            "password": "ChangedPass123!",
            "permission": 7,
            "disabled": true,
            "local_path": missing_root,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], 400);

    let after = db::get_user_by_id(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.username, before.username);
    assert_eq!(after.pwd_hash, before.pwd_hash);
    assert_eq!(after.pwd_ts, before.pwd_ts);
    assert_eq!(after.local_path, before.local_path);
    assert_eq!(after.role, before.role);
    assert_eq!(after.disabled, before.disabled);
    assert_eq!(after.permission, before.permission);
    assert_eq!(after.password_unset, before.password_unset);
    assert_eq!(after.otp_secret, before.otp_secret);
    assert_eq!(after.last_otp_step, before.last_otp_step);
    assert_eq!(after.otp, before.otp);
}

#[tokio::test]
async fn server_rejects_addresses_other_than_exact_localhost() {
    let temp = tempfile::tempdir().unwrap();
    let pool = db::init_db(&temp.path().join("rulist.db"), temp.path())
        .await
        .unwrap();

    for host in ["127.0.0.2", "0.0.0.0"] {
        let mut config = Config::default();
        config.server.address = host.to_string();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rulist::server::run_server(config, pool.clone(), StorageManager::default()),
        )
        .await
        .expect("server should reject the address before binding");
        let error = result.expect_err("non-localhost address must be rejected");
        assert!(
            error
                .to_string()
                .contains("only 127.0.0.1 or ::1 is permitted")
        );
    }
}
