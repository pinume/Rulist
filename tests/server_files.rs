mod common;
use common::{app_for, json_request, login_token};

use axum::http::StatusCode;
use rulist::db;
use rulist::permissions::{COPY, MOVE, WRITE_CONTENT};
use serde_json::json;

#[tokio::test]
async fn filesystem_handlers_map_typed_errors_and_conflicts_to_http_statuses() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("files");
    let copy_src = root.join("copy-src");
    let copy_dst = root.join("copy-dst");
    let move_src = root.join("move-src");
    let move_dst = root.join("move-dst");
    for dir in [&copy_src, &copy_dst, &move_src, &move_dst] {
        tokio::fs::create_dir_all(dir).await.unwrap();
    }
    tokio::fs::write(root.join("plain-file"), b"file")
        .await
        .unwrap();
    tokio::fs::write(copy_src.join("exists.txt"), b"source copy")
        .await
        .unwrap();
    tokio::fs::write(copy_dst.join("exists.txt"), b"destination copy")
        .await
        .unwrap();
    tokio::fs::write(move_src.join("exists.txt"), b"source move")
        .await
        .unwrap();
    tokio::fs::write(move_dst.join("exists.txt"), b"destination move")
        .await
        .unwrap();
    std::os::unix::fs::symlink("missing-target", copy_dst.join("broken-link")).unwrap();

    let pool = db::init_db(&temp.path().join("rulist.db")).await.unwrap();
    db::create_user(
        &pool,
        "file-user",
        "FilesPass123!",
        0,
        Some(root.to_str().unwrap()),
        (1 << WRITE_CONTENT) | (1 << MOVE) | (1 << COPY),
        false,
    )
    .await
    .unwrap();
    let app = app_for(&pool).await;
    let token = login_token(&app, "file-user", "FilesPass123!").await;

    let (status, missing) = json_request(
        &app,
        "POST",
        "/api/fs/get",
        Some(&token),
        json!({ "path": "/missing.txt" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], 404);

    let (status, missing_preview) = json_request(
        &app,
        "POST",
        "/api/fs/preview",
        Some(&token),
        json!({ "path": "/missing.txt" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing_preview["code"], 404);

    let (status, invalid_path) = json_request(
        &app,
        "POST",
        "/api/fs/list",
        Some(&token),
        json!({ "path": "/../plain-file" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_path["code"], 400);

    let (status, forbidden_symlink) = json_request(
        &app,
        "POST",
        "/api/fs/get",
        Some(&token),
        json!({ "path": "/copy-dst/broken-link" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden_symlink["code"], 403);

    let (status, not_directory) = json_request(
        &app,
        "POST",
        "/api/fs/list",
        Some(&token),
        json!({ "path": "/plain-file/child" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(not_directory["code"], 400);

    let (status, empty_mkdir) = json_request(
        &app,
        "POST",
        "/api/fs/mkdir",
        Some(&token),
        json!({ "path": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(empty_mkdir["code"], 400);

    for (route, src_dir, dst_dir) in [
        ("/api/fs/copy", "/copy-src", "/copy-dst"),
        ("/api/fs/move", "/move-src", "/move-dst"),
    ] {
        let (status, conflict) = json_request(
            &app,
            "POST",
            route,
            Some(&token),
            json!({
                "src_dir": src_dir,
                "dst_dir": dst_dir,
                "names": ["exists.txt"],
                "conflict_policy": "cancel",
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{route}: {conflict}");
        assert_eq!(conflict["code"], 409);
    }

    let (status, duplicate_names) = json_request(
        &app,
        "POST",
        "/api/fs/copy",
        Some(&token),
        json!({
            "src_dir": "/copy-src",
            "dst_dir": "/copy-dst",
            "names": ["exists.txt", "exists.txt"],
            "conflict_policy": "skip",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(duplicate_names["code"], 400);

    let (status, symlink_conflict) = json_request(
        &app,
        "POST",
        "/api/fs/copy",
        Some(&token),
        json!({
            "src_dir": "/copy-src",
            "dst_dir": "/copy-dst",
            "names": ["broken-link"],
            "conflict_policy": "cancel",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(symlink_conflict["code"], 409);
}
