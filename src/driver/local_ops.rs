use std::path::Path;

use anyhow::{Context, Result, anyhow};
use tokio::fs;

pub(super) async fn copy_path_safe(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    if src == dst {
        return Err(anyhow!("source and destination are identical: {:?}", src));
    }

    let meta = fs::symlink_metadata(src)
        .await
        .with_context(|| format!("source path does not exist: {:?}", src))?;
    if meta.file_type().is_symlink() {
        return Err(anyhow!("symlinks are not supported: {:?}", src));
    }
    if meta.is_dir() && dst.starts_with(src) {
        return Err(anyhow!(
            "cannot copy directory into itself: {:?} -> {:?}",
            src,
            dst
        ));
    }

    let dst_exists = fs::symlink_metadata(dst).await.is_ok();
    if dst_exists && !overwrite {
        return Err(anyhow!("destination path already exists: {:?}", dst));
    }

    let parent = dst.parent().ok_or_else(|| anyhow!("cannot copy to root"))?;
    fs::create_dir_all(parent).await?;
    let nonce = crate::auth::rand_string(24);
    let stage = parent.join(format!(".rulist-copy-stage-{nonce}"));
    let backup = parent.join(format!(".rulist-backup-{nonce}"));

    if let Err(error) = copy_path_recursive(src, &stage).await {
        let _ = remove_path_recursive(&stage).await;
        return Err(error.context("failed to copy source to staging directory"));
    }

    if dst_exists {
        if let Err(error) = fs::rename(dst, &backup).await {
            let _ = remove_path_recursive(&stage).await;
            return Err(anyhow!(
                "failed to backup existing destination {:?}: {}",
                dst,
                error
            ));
        }
    }

    if let Err(error) = fs::rename(&stage, dst).await {
        if dst_exists && let Err(restore_error) = fs::rename(&backup, dst).await {
            tracing::error!(
                error = %restore_error,
                "CRITICAL: failed to restore backup after copy promotion failure"
            );
        }
        let _ = remove_path_recursive(&stage).await;
        return Err(anyhow!("failed to replace destination with stage: {error}"));
    }

    if dst_exists && let Err(error) = remove_path_recursive(&backup).await {
        tracing::warn!(
            error = %error,
            path = ?backup,
            "failed to remove backup after successful copy overwrite"
        );
    }

    Ok(())
}

pub(super) async fn move_path_safe(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    if src == dst {
        return Err(anyhow!("source and destination are identical: {:?}", src));
    }

    let meta = fs::symlink_metadata(src)
        .await
        .with_context(|| format!("source path does not exist: {:?}", src))?;
    if meta.file_type().is_symlink() {
        return Err(anyhow!("symlinks are not supported: {:?}", src));
    }
    if meta.is_dir() && dst.starts_with(src) {
        return Err(anyhow!(
            "cannot move directory into itself: {:?} -> {:?}",
            src,
            dst
        ));
    }

    let dst_exists = fs::symlink_metadata(dst).await.is_ok();
    if !dst_exists {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await?;
        }
        return match fs::rename(src, dst).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
                move_cross_device_safe(src, dst, overwrite).await
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to move {:?} to {:?}", src, dst))
            }
        };
    }

    if !overwrite {
        return Err(anyhow!("destination path already exists: {:?}", dst));
    }

    let parent = dst.parent().ok_or_else(|| anyhow!("cannot move to root"))?;
    fs::create_dir_all(parent).await?;

    let src_canon = fs::canonicalize(src).await.ok();
    let dst_canon = fs::canonicalize(dst).await.ok();
    if src_canon.is_some() && src_canon == dst_canon {
        let temp = parent.join(format!(
            ".rulist-move-case-{}",
            crate::auth::rand_string(24)
        ));
        fs::rename(src, &temp).await?;
        if let Err(error) = fs::rename(&temp, dst).await {
            let _ = fs::rename(&temp, src).await;
            return Err(error.into());
        }
        return Ok(());
    }

    let backup = parent.join(format!(".rulist-backup-{}", crate::auth::rand_string(24)));
    fs::rename(dst, &backup)
        .await
        .with_context(|| format!("failed to backup existing destination {:?}", dst))?;

    match fs::rename(src, dst).await {
        Ok(()) => {
            if let Err(error) = remove_path_recursive(&backup).await {
                tracing::warn!(
                    error = %error,
                    path = ?backup,
                    "failed to remove backup after successful move overwrite"
                );
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
            if let Err(restore_error) = fs::rename(&backup, dst).await {
                tracing::error!(
                    error = %restore_error,
                    "CRITICAL: failed to restore backup before cross-device move fallback"
                );
                return Err(restore_error.into());
            }
            move_cross_device_safe(src, dst, overwrite).await
        }
        Err(error) => {
            if let Err(restore_error) = fs::rename(&backup, dst).await {
                tracing::error!(
                    error = %restore_error,
                    "CRITICAL: failed to restore backup after move failure"
                );
            }
            Err(error).with_context(|| format!("failed to move {:?} to {:?}", src, dst))
        }
    }
}

async fn move_cross_device_safe(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    let parent = dst
        .parent()
        .ok_or_else(|| anyhow!("destination has no parent"))?;
    fs::create_dir_all(parent).await?;

    let had_destination = fs::symlink_metadata(dst).await.is_ok();
    if had_destination && !overwrite {
        return Err(anyhow!("destination path already exists: {:?}", dst));
    }

    let nonce = crate::auth::rand_string(24);
    let stage = parent.join(format!(".rulist-move-stage-{nonce}"));
    let backup = parent.join(format!(".rulist-move-backup-{nonce}"));

    if let Err(error) = copy_path_recursive(src, &stage).await {
        let _ = remove_path_recursive(&stage).await;
        return Err(error.context("failed to stage cross-device move"));
    }

    if had_destination {
        if let Err(error) = fs::rename(dst, &backup).await {
            let _ = remove_path_recursive(&stage).await;
            return Err(error).with_context(|| format!("failed to backup destination {:?}", dst));
        }
    }

    if let Err(error) = fs::rename(&stage, dst).await {
        if had_destination && let Err(restore_error) = fs::rename(&backup, dst).await {
            tracing::error!(
                error = %restore_error,
                backup = ?backup,
                dst = ?dst,
                "CRITICAL: failed to restore destination backup"
            );
        }
        let _ = remove_path_recursive(&stage).await;
        return Err(error.into());
    }

    if let Err(error) = remove_path_recursive(src).await {
        tracing::error!(
            error = %error,
            src = ?src,
            dst = ?dst,
            backup = ?backup,
            "source cleanup failed after completed cross-device copy; preserving recovery data"
        );
        return Err(error.context("destination was copied successfully but source cleanup failed"));
    }

    if had_destination && let Err(error) = remove_path_recursive(&backup).await {
        tracing::warn!(
            error = %error,
            backup = ?backup,
            "failed to remove move backup after successful operation"
        );
    }

    Ok(())
}

async fn copy_path_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src == dst {
        return Err(anyhow!("source and destination are identical: {:?}", src));
    }

    let meta = fs::symlink_metadata(src)
        .await
        .with_context(|| format!("source path does not exist: {:?}", src))?;
    if meta.file_type().is_symlink() {
        return Err(anyhow!("symlinks are not supported: {:?}", src));
    }

    if meta.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::copy(src, dst)
            .await
            .with_context(|| format!("failed to copy {:?} to {:?}", src, dst))?;
        return Ok(());
    }

    if meta.is_dir() {
        if dst.starts_with(src) {
            return Err(anyhow!(
                "cannot copy directory into itself: {:?} -> {:?}",
                src,
                dst
            ));
        }

        fs::create_dir_all(dst).await?;
        let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
        while let Some((current_src, current_dst)) = stack.pop() {
            let mut entries = fs::read_dir(&current_src)
                .await
                .with_context(|| format!("failed to read directory: {:?}", current_src))?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let file_type = entry.file_type().await?;
                if file_type.is_symlink() {
                    return Err(anyhow!("symlinks are not supported: {:?}", path));
                }

                let target = current_dst.join(entry.file_name());
                if file_type.is_dir() {
                    fs::create_dir_all(&target).await?;
                    stack.push((path, target));
                } else if file_type.is_file() {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).await?;
                    }
                    fs::copy(&path, &target)
                        .await
                        .with_context(|| format!("failed to copy {:?} to {:?}", path, target))?;
                }
            }
        }
        return Ok(());
    }

    Err(anyhow!("unsupported file type for {:?}", src))
}

pub(super) async fn remove_path_recursive(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .await
        .with_context(|| format!("target does not exist: {:?}", path))?;
    if meta.file_type().is_symlink() {
        return Err(anyhow!("symlinks are not supported: {:?}", path));
    }

    if meta.is_dir() {
        fs::remove_dir_all(path)
            .await
            .with_context(|| format!("failed to remove directory: {:?}", path))?;
    } else {
        fs::remove_file(path)
            .await
            .with_context(|| format!("failed to remove file: {:?}", path))?;
    }
    Ok(())
}
