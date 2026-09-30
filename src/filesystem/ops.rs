use std::path::Path;
use std::{ffi::CString, io};

use anyhow::{Context, Result, anyhow};
use tokio::fs;

pub(super) async fn entry_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path).await {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn rename_no_replace(src: &Path, dst: &Path) -> io::Result<()> {
    let src = src.to_path_buf();
    let dst = dst.to_path_buf();
    tokio::task::spawn_blocking(move || rename_no_replace_sync(&src, &dst))
        .await
        .map_err(io::Error::other)?
}

fn rename_no_replace_sync(src: &Path, dst: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let src = CString::new(src.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let dst = CString::new(dst.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    // RENAME_NOREPLACE keeps the absent-target check and rename in one kernel operation.
    // SAFETY: both paths are NUL-terminated and AT_FDCWD is a valid dirfd.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD as libc::c_long,
            src.as_ptr(),
            libc::AT_FDCWD as libc::c_long,
            dst.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(super) async fn copy_path_safe(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    if src == dst {
        return Err(crate::filesystem::FsError::InvalidPath.into());
    }

    let meta = fs::symlink_metadata(src)
        .await
        .with_context(|| format!("source path does not exist: {:?}", src))?;
    if meta.file_type().is_symlink() {
        return Err(crate::filesystem::FsError::Forbidden.into());
    }
    if meta.is_dir() && dst.starts_with(src) {
        return Err(crate::filesystem::FsError::InvalidPath.into());
    }

    let dst_exists = entry_exists(dst).await?;
    if dst_exists && !overwrite {
        return Err(crate::filesystem::FsError::Conflict.into());
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
        if let Err(error) = rename_no_replace(dst, &backup).await {
            let _ = remove_path_recursive(&stage).await;
            return Err(anyhow!(
                "failed to backup existing destination {:?}: {}",
                dst,
                error
            ));
        }
    }

    let promotion = if overwrite {
        fs::rename(&stage, dst).await
    } else {
        rename_no_replace(&stage, dst).await
    };
    if let Err(error) = promotion {
        if dst_exists {
            if let Err(restore_error) = rename_no_replace(&backup, dst).await {
                tracing::error!(
                    error = %restore_error,
                    "CRITICAL: failed to restore backup after copy promotion failure"
                );
            }
        }
        let _ = remove_path_recursive(&stage).await;
        return Err(anyhow::Error::from(error).context("failed to promote copy stage"));
    }

    if dst_exists {
        if let Err(error) = remove_path_recursive(&backup).await {
            tracing::warn!(
                error = %error,
                path = ?backup,
                "failed to remove backup after successful copy overwrite"
            );
        }
    }

    Ok(())
}

pub(super) async fn move_path_safe(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    if src == dst {
        return Err(crate::filesystem::FsError::InvalidPath.into());
    }

    let meta = fs::symlink_metadata(src)
        .await
        .with_context(|| format!("source path does not exist: {:?}", src))?;
    if meta.file_type().is_symlink() {
        return Err(crate::filesystem::FsError::Forbidden.into());
    }
    if meta.is_dir() && dst.starts_with(src) {
        return Err(crate::filesystem::FsError::InvalidPath.into());
    }

    let dst_exists = entry_exists(dst).await?;
    if !dst_exists {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await?;
        }
        let rename = if overwrite {
            fs::rename(src, dst).await
        } else {
            rename_no_replace(src, dst).await
        };
        return match rename {
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
        return Err(crate::filesystem::FsError::Conflict.into());
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
    rename_no_replace(dst, &backup)
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
            if let Err(restore_error) = rename_no_replace(&backup, dst).await {
                tracing::error!(
                    error = %restore_error,
                    "CRITICAL: failed to restore backup before cross-device move fallback"
                );
                return Err(restore_error.into());
            }
            move_cross_device_safe(src, dst, overwrite).await
        }
        Err(error) => {
            if let Err(restore_error) = rename_no_replace(&backup, dst).await {
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

    let had_destination = entry_exists(dst).await?;
    if had_destination && !overwrite {
        return Err(crate::filesystem::FsError::Conflict.into());
    }

    let nonce = crate::auth::rand_string(24);
    let stage = parent.join(format!(".rulist-move-stage-{nonce}"));
    let backup = parent.join(format!(".rulist-move-backup-{nonce}"));

    if let Err(error) = copy_path_recursive(src, &stage).await {
        let _ = remove_path_recursive(&stage).await;
        return Err(error.context("failed to stage cross-device move"));
    }

    if had_destination {
        if let Err(error) = rename_no_replace(dst, &backup).await {
            let _ = remove_path_recursive(&stage).await;
            return Err(error).with_context(|| format!("failed to backup destination {:?}", dst));
        }
    }

    let promotion = if overwrite {
        fs::rename(&stage, dst).await
    } else {
        rename_no_replace(&stage, dst).await
    };
    if let Err(error) = promotion {
        if had_destination {
            if let Err(restore_error) = rename_no_replace(&backup, dst).await {
                tracing::error!(
                    error = %restore_error,
                    backup = ?backup,
                    dst = ?dst,
                    "CRITICAL: failed to restore destination backup"
                );
            }
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

    if had_destination {
        if let Err(error) = remove_path_recursive(&backup).await {
            tracing::warn!(
                error = %error,
                backup = ?backup,
                "failed to remove move backup after successful operation"
            );
        }
    }

    Ok(())
}

async fn copy_path_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src == dst {
        return Err(crate::filesystem::FsError::InvalidPath.into());
    }

    let meta = fs::symlink_metadata(src)
        .await
        .with_context(|| format!("source path does not exist: {:?}", src))?;
    if meta.file_type().is_symlink() {
        return Err(crate::filesystem::FsError::Forbidden.into());
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
            return Err(crate::filesystem::FsError::InvalidPath.into());
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
                    return Err(crate::filesystem::FsError::Forbidden.into());
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

    Err(crate::filesystem::FsError::InvalidPath.into())
}

pub(super) async fn remove_path_recursive(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .await
        .with_context(|| format!("target does not exist: {:?}", path))?;
    if meta.file_type().is_symlink() {
        return Err(crate::filesystem::FsError::Forbidden.into());
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

#[cfg(test)]
mod tests {
    use super::rename_no_replace_sync;
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_no_replace_renames_preserve_the_losing_source() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let target = temp.path().join("target");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let workers = [first.clone(), second.clone()].map(|source| {
            let barrier = barrier.clone();
            let target = target.clone();
            std::thread::spawn(move || {
                barrier.wait();
                rename_no_replace_sync(&source, &target)
            })
        });
        barrier.wait();

        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .next()
                .unwrap()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert!(target.exists());
        assert_ne!(first.exists(), second.exists());
    }
}
