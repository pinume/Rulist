use rulist::driver::local::LocalDriver;

fn driver(root: &std::path::Path) -> LocalDriver {
    LocalDriver::new(
        &serde_json::json!({
            "root_folder_path": root,
            "show_hidden": false
        })
        .to_string(),
    )
    .unwrap()
}

#[tokio::test]
async fn rejects_traversal_and_storage_root_removal() {
    let temp = tempfile::tempdir().unwrap();
    let driver = driver(temp.path());

    assert!(driver.safe_resolve("../etc/passwd").is_err());
    assert!(driver.safe_resolve("folder/../../etc").is_err());
    assert!(driver.remove("").await.is_err());
    assert!(driver.remove("/").await.is_err());
    assert!(temp.path().exists());
}

#[tokio::test]
async fn rejects_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    let driver = driver(root.path());

    assert!(driver.safe_resolve("escape/secret.txt").is_err());
    assert!(driver.open("escape/secret.txt").await.is_err());
}

#[tokio::test]
async fn copy_and_move_preserve_expected_contents() {
    let temp = tempfile::tempdir().unwrap();
    let driver = driver(temp.path());

    tokio::fs::create_dir_all(temp.path().join("source/nested"))
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("source/nested/file.txt"), b"content")
        .await
        .unwrap();

    driver.copy_to("source", "copy").await.unwrap();
    assert_eq!(
        tokio::fs::read(temp.path().join("copy/nested/file.txt"))
            .await
            .unwrap(),
        b"content"
    );

    driver
        .move_to("copy/nested/file.txt", "moved/file.txt")
        .await
        .unwrap();
    assert!(!temp.path().join("copy/nested/file.txt").exists());
    assert_eq!(
        tokio::fs::read(temp.path().join("moved/file.txt"))
            .await
            .unwrap(),
        b"content"
    );
}

#[tokio::test]
async fn overwrite_is_explicit_for_copy_and_rename() {
    let temp = tempfile::tempdir().unwrap();
    let driver = driver(temp.path());

    tokio::fs::write(temp.path().join("source.txt"), b"new")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("target.txt"), b"old")
        .await
        .unwrap();

    assert!(
        driver
            .copy_to_safe("source.txt", "target.txt", false)
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("target.txt"))
            .await
            .unwrap(),
        b"old"
    );

    driver
        .copy_to_safe("source.txt", "target.txt", true)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(temp.path().join("target.txt"))
            .await
            .unwrap(),
        b"new"
    );

    tokio::fs::write(temp.path().join("rename-source.txt"), b"renamed")
        .await
        .unwrap();
    assert!(
        driver
            .rename_safe("rename-source.txt", "target.txt", false)
            .await
            .is_err()
    );
    driver
        .rename_safe("rename-source.txt", "target.txt", true)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(temp.path().join("target.txt"))
            .await
            .unwrap(),
        b"renamed"
    );
}
