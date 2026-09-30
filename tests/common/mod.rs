use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use rulist::config::Config;
use rulist::db;
use rulist::server::{AppState, build_app};
use serde_json::{Value, json};
use tower::ServiceExt;

pub async fn json_request(
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

#[allow(dead_code)]
pub async fn raw_status_request(app: &axum::Router, path: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

pub async fn app_for(pool: &db::DbPool) -> axum::Router {
    build_app(Arc::new(AppState {
        pool: pool.clone(),
        config: Config::default(),
    }))
}

pub async fn login_token(app: &axum::Router, username: &str, password: &str) -> String {
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
