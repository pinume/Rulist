use rulist::db;
use rulist::driver::StorageManager;

#[tokio::test]
async fn cross_mount_operations_reject_storage_roots() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let pool = db::init_db(&data.path().join("rulist.db"), data.path())
        .await
        .unwrap();

    tokio::fs::write(source.path().join("file"), b"data")
        .await
        .unwrap();

    for (mount, root) in [
        ("/source", source.path()),
        ("/destination", destination.path()),
    ] {
        sqlx::query(
            "INSERT INTO `x_storages` (`mount_path`, `local_path`, `show_hidden`) VALUES (?, ?, 0)",
        )
        .bind(mount)
        .bind(root.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    }

    let manager = StorageManager::load_from_db(&pool).await.unwrap();
    manager
        .copy_to_safe("/source/file", "/destination/copied", false)
        .await
        .unwrap();
    assert_eq!(
        tokio::fs::read(destination.path().join("copied"))
            .await
            .unwrap(),
        b"data"
    );

    assert!(
        manager
            .move_to_safe("/source", "/destination/file", false)
            .await
            .is_err()
    );
    assert!(
        manager
            .move_to_safe("/source/file", "/destination", false)
            .await
            .is_err()
    );
    assert!(
        manager
            .copy_to_safe("/source", "/destination/file", false)
            .await
            .is_err()
    );
    assert!(
        manager
            .copy_to_safe("/source/file", "/destination", false)
            .await
            .is_err()
    );
    assert!(source.path().join("file").exists());
}
