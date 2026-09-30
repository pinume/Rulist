use rulist::filesystem::local::LocalFs;
use std::os::unix::ffi::OsStringExt;

fn driver_with_hidden(root: &std::path::Path, show_hidden: bool) -> LocalFs {
    LocalFs::new(root, show_hidden).unwrap()
}

fn driver(root: &std::path::Path) -> LocalFs {
    driver_with_hidden(root, false)
}

#[tokio::test]
async fn rejects_traversal_and_filesystem_root_removal() {
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
async fn rejects_special_files() {
    let temp = tempfile::tempdir().unwrap();
    let socket_path = temp.path().join("test.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let fs = driver(temp.path());

    let entries = fs.list("").await.unwrap();
    assert!(!entries.iter().any(|entry| entry.name == "test.sock"));
    assert!(fs.get("test.sock").await.is_err());
    assert!(fs.open("test.sock").await.is_err());
}

#[tokio::test]
async fn skips_non_utf8_filenames() {
    let temp = tempfile::tempdir().unwrap();
    let invalid_name = std::ffi::OsString::from_vec(b"invalid-\xff.txt".to_vec());
    std::fs::write(temp.path().join(invalid_name), b"hidden from web").unwrap();
    tokio::fs::write(temp.path().join("valid.txt"), b"visible")
        .await
        .unwrap();
    let fs = driver(temp.path());

    let entries = fs.list("").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "valid.txt");
}

#[tokio::test]
async fn hidden_paths_follow_show_hidden_policy() {
    let temp = tempfile::tempdir().unwrap();
    tokio::fs::create_dir_all(temp.path().join(".secret"))
        .await
        .unwrap();
    tokio::fs::write(temp.path().join(".secret/file.txt"), b"secret")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("visible.txt"), b"visible")
        .await
        .unwrap();

    let hidden = driver_with_hidden(temp.path(), false);
    assert!(hidden.safe_resolve(".secret/file.txt").is_err());
    assert!(hidden.list(".secret").await.is_err());
    assert!(hidden.mkdir(".created").await.is_err());
    assert!(
        hidden
            .rename_safe("visible.txt", ".renamed", false)
            .await
            .is_err()
    );
    assert!(
        hidden
            .batch_rename("", &[("visible.txt".to_string(), ".batch".to_string())])
            .await
            .is_err()
    );

    let visible = driver_with_hidden(temp.path(), true);
    assert!(visible.safe_resolve(".secret/file.txt").is_ok());
    let entries = visible.list(".secret").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.txt");
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

    driver
        .copy_to_safe("source.txt", "copy-no-overwrite.txt", false)
        .await
        .unwrap();
    driver
        .move_to_safe("copy-no-overwrite.txt", "move-no-overwrite.txt", false)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(temp.path().join("move-no-overwrite.txt"))
            .await
            .unwrap(),
        b"new"
    );

    tokio::fs::write(temp.path().join("move-source.txt"), b"source remains")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("move-target.txt"), b"existing target")
        .await
        .unwrap();
    assert!(
        driver
            .move_to_safe("move-source.txt", "move-target.txt", false)
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("move-source.txt"))
            .await
            .unwrap(),
        b"source remains"
    );
    assert_eq!(
        tokio::fs::read(temp.path().join("move-target.txt"))
            .await
            .unwrap(),
        b"existing target"
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

    tokio::fs::write(
        temp.path().join("rename-no-overwrite.txt"),
        b"renamed safely",
    )
    .await
    .unwrap();
    driver
        .rename_safe("rename-no-overwrite.txt", "renamed-no-overwrite.txt", false)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(temp.path().join("renamed-no-overwrite.txt"))
            .await
            .unwrap(),
        b"renamed safely"
    );
}
