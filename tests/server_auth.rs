use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use rulist::auth::compute_totp;
use rulist::config::Config;
use rulist::db;
use rulist::driver::StorageManager;
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

#[tokio::test]
async fn two_factor_lifecycle_is_enforced_and_replay_safe() {
    let temp = tempfile::tempdir().unwrap();
    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();
    db::set_admin_password(&pool, "TestPass123!").await.unwrap();

    let master_token = db::get_setting(&pool, "token").await.unwrap().unwrap();
    let storage = StorageManager::load_from_db(&pool).await.unwrap();
    let app = build_app(Arc::new(AppState {
        pool: pool.clone(),
        config: Config::default(),
        storage,
    }));

    let (status, generated) = json_request(
        &app,
        "POST",
        "/api/auth/2fa/generate",
        Some(&master_token),
        json!({ "current_password": "TestPass123!" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(generated["code"], 200);
    let secret = generated["data"]["secret"].as_str().unwrap();

    let step = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 30;
    let setup_code = compute_totp(secret, step).unwrap();
    let (status, verified) = json_request(
        &app,
        "POST",
        "/api/auth/2fa/verify",
        Some(&master_token),
        json!({ "code": setup_code }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(verified["code"], 200);

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

    let login_code = compute_totp(secret, step + 1).unwrap();
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

    let replay_code = compute_totp(secret, step + 1).unwrap();
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
