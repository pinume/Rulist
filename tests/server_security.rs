mod common;
use common::{app_for, json_request, login_token, raw_status_request};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use rulist::auth::generate_otp_secret;
use rulist::config::Config;
use rulist::db;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn spa_fallback_returns_404_for_unknown_api_and_stream_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("files");
    tokio::fs::create_dir_all(&root).await.unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();
    db::create_user(
        &pool,
        "fallback-user",
        "FallbackPass123!",
        0,
        Some(root.to_str().unwrap()),
        0,
        false,
    )
    .await
    .unwrap();
    let app = app_for(&pool).await;

    assert_eq!(
        raw_status_request(&app, "/api/not-exist").await,
        StatusCode::NOT_FOUND
    );
    let page = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/unknown-page")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        page.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/html; charset=utf-8")
    );

    let token = login_token(&app, "fallback-user", "FallbackPass123!").await;
    let (status, link) = json_request(
        &app,
        "POST",
        "/api/fs/link",
        Some(&token),
        json!({ "path": "/unknown-file" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let download_path = link["data"]["url"].as_str().unwrap();
    assert_eq!(
        raw_status_request(&app, download_path).await,
        StatusCode::NOT_FOUND
    );
    let preview_path = download_path.replacen("/d/", "/p/", 1);
    assert_eq!(
        raw_status_request(&app, &preview_path).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn disabling_user_permanently_revokes_jwt_and_signed_links() {
    let temp = tempfile::tempdir().unwrap();
    let guest_root = temp.path().join("guest-root");
    tokio::fs::create_dir(&guest_root).await.unwrap();
    tokio::fs::write(guest_root.join("signed.txt"), b"signed content")
        .await
        .unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();
    let guest_id = db::create_user(
        &pool,
        "disable-guest",
        "GuestPass123!",
        0,
        Some(guest_root.to_str().unwrap()),
        0,
        false,
    )
    .await
    .unwrap();
    let app = app_for(&pool).await;
    let jwt_a = login_token(&app, "disable-guest", "GuestPass123!").await;
    let (status, link) = json_request(
        &app,
        "POST",
        "/api/fs/link",
        Some(&jwt_a),
        json!({ "path": "/signed.txt" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let signed_url = link["data"]["url"].as_str().unwrap();
    assert_eq!(raw_status_request(&app, signed_url).await, StatusCode::OK);

    let pwd_ts_before_disable = db::get_user_by_id(&pool, guest_id)
        .await
        .unwrap()
        .unwrap()
        .pwd_ts;
    db::set_user_disabled(&pool, guest_id, true).await.unwrap();
    let disabled = db::get_user_by_id(&pool, guest_id).await.unwrap().unwrap();
    assert!(disabled.pwd_ts > pwd_ts_before_disable);
    assert_eq!(
        raw_status_request(&app, signed_url).await,
        StatusCode::FORBIDDEN
    );
    let (status, _) = json_request(&app, "GET", "/api/me", Some(&jwt_a), Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    db::set_user_disabled(&pool, guest_id, false).await.unwrap();
    let enabled = db::get_user_by_id(&pool, guest_id).await.unwrap().unwrap();
    assert!(!enabled.disabled);
    assert_eq!(enabled.pwd_ts, disabled.pwd_ts);
    assert_eq!(
        raw_status_request(&app, signed_url).await,
        StatusCode::FORBIDDEN
    );
    let (status, _) = json_request(&app, "GET", "/api/me", Some(&jwt_a), Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let jwt_b = login_token(&app, "disable-guest", "GuestPass123!").await;
    let (status, _) = json_request(&app, "GET", "/api/me", Some(&jwt_b), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn root_and_2fa_changes_revoke_existing_jwts_and_signed_links() {
    let temp = tempfile::tempdir().unwrap();
    let first_root = temp.path().join("first-root");
    let second_root = temp.path().join("second-root");
    tokio::fs::create_dir_all(&first_root).await.unwrap();
    tokio::fs::create_dir_all(&second_root).await.unwrap();
    tokio::fs::write(first_root.join("signed.txt"), b"first")
        .await
        .unwrap();
    tokio::fs::write(second_root.join("signed.txt"), b"second")
        .await
        .unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();
    let user_id = db::create_user(
        &pool,
        "security-user",
        "SecurityPass123!",
        0,
        Some(first_root.to_str().unwrap()),
        0,
        false,
    )
    .await
    .unwrap();
    let app = app_for(&pool).await;
    let before_root_change = db::get_user_by_id(&pool, user_id)
        .await
        .unwrap()
        .unwrap()
        .pwd_ts;
    let first_jwt = login_token(&app, "security-user", "SecurityPass123!").await;
    let (status, first_link) = json_request(
        &app,
        "POST",
        "/api/fs/link",
        Some(&first_jwt),
        json!({ "path": "/signed.txt" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first_url = first_link["data"]["url"].as_str().unwrap();

    db::set_user_local_path(&pool, user_id, second_root.to_str().unwrap())
        .await
        .unwrap();
    let after_root_change = db::get_user_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(after_root_change.pwd_ts > before_root_change);
    assert_eq!(
        json_request(&app, "GET", "/api/me", Some(&first_jwt), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        raw_status_request(&app, first_url).await,
        StatusCode::FORBIDDEN
    );

    let second_jwt = login_token(&app, "security-user", "SecurityPass123!").await;
    let (status, second_link) = json_request(
        &app,
        "POST",
        "/api/fs/link",
        Some(&second_jwt),
        json!({ "path": "/signed.txt" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_url = second_link["data"]["url"].as_str().unwrap();

    let before_enable = after_root_change.pwd_ts;
    let secret = generate_otp_secret();
    db::enable_user_2fa(&pool, user_id, &secret, 1)
        .await
        .unwrap();
    let after_enable = db::get_user_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(after_enable.pwd_ts > before_enable);
    assert_eq!(
        json_request(&app, "GET", "/api/me", Some(&second_jwt), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        raw_status_request(&app, second_url).await,
        StatusCode::FORBIDDEN
    );

    db::disable_user_2fa(&pool, user_id).await.unwrap();
    let after_disable = db::get_user_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(after_disable.pwd_ts > after_enable.pwd_ts);
    assert_eq!(
        json_request(&app, "GET", "/api/me", Some(&second_jwt), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        raw_status_request(&app, second_url).await,
        StatusCode::FORBIDDEN
    );
    let fresh_jwt = login_token(&app, "security-user", "SecurityPass123!").await;
    assert_eq!(
        json_request(&app, "GET", "/api/me", Some(&fresh_jwt), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn user_local_path_update_rejects_invalid_path_without_mutating_user() {
    let temp = tempfile::tempdir().unwrap();
    let old_root = temp.path().join("old-root");
    let missing_root = temp.path().join("missing-root");
    tokio::fs::create_dir_all(&old_root).await.unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();
    db::set_admin_password(&pool, "AdminPass123!")
        .await
        .unwrap();
    let user_id = db::create_user(
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

    assert!(
        db::set_user_local_path(&pool, user_id, &missing_root.to_string_lossy())
            .await
            .is_err()
    );

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
    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();

    for host in ["127.0.0.2", "0.0.0.0"] {
        let mut config = Config::default();
        config.scheme.address = host.to_string();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rulist::server::run_server(config, pool.clone()),
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
