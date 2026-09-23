#![allow(dead_code)]

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tokio::fs;

use crate::model::FileObj;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LocalAddition {
    #[serde(default)]
    pub root_folder_path: String,
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Clone)]
pub struct LocalDriver {
    pub root_path: PathBuf,
    pub show_hidden: bool,
}

#[derive(Debug)]
pub enum RenameError {
    Conflict(String),
    NotFound(String),
    BadRequest(String),
    Internal(anyhow::Error),
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::BadRequest(msg) => write!(f, "bad request: {msg}"),
            Self::Internal(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for RenameError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MoveFailurePoint {
    #[default]
    None,
    AfterDestinationBackup,
    CrossDeviceCopy,
    StagingPromotion,
    SourceCleanup,
}

impl LocalDriver {
    pub fn new(addition_json: &str) -> Result<Self> {
        let addition: LocalAddition = if addition_json.is_empty() {
            return Err(anyhow!("storage configuration is empty"));
        } else {
            serde_json::from_str(addition_json).context("failed to parse storage configuration")?
        };

        let root_str = addition.root_folder_path.trim();
        if root_str.is_empty() {
            return Err(anyhow!("root_folder_path cannot be empty"));
        }

        let path = Path::new(root_str);
        if !path.is_absolute() {
            return Err(anyhow!(
                "root_folder_path must be an absolute path: {}",
                root_str
            ));
        }

        if !path.exists() {
            return Err(anyhow!("root_folder_path does not exist: {:?}", path));
        }

        let root_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        Ok(Self {
            root_path,
            show_hidden: addition.show_hidden,
        })
    }

    /// Resolve a subpath securely, preventing any Path Traversal attacks (../)
    pub fn safe_resolve(&self, subpath: &str) -> Result<PathBuf> {
        let clean = subpath.trim_matches('/');
        let mut target = self.root_path.clone();

        for component in Path::new(clean).components() {
            match component {
                Component::Normal(c) => target.push(c),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(anyhow!(
                        "access denied: parent directory traversal is forbidden"
                    ));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(anyhow!(
                        "access denied: absolute path components are forbidden"
                    ));
                }
            }
            match std::fs::symlink_metadata(&target) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(anyhow!("access denied: symbolic links are forbidden"));
                }
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }

        Ok(target)
    }

    /// Check if a directory is physically empty on disk without ignoring hidden files
    pub async fn is_physically_empty(&self, subpath: &str) -> Result<bool> {
        let full_path = self.safe_resolve(subpath)?;

        let meta = fs::symlink_metadata(&full_path).await?;

        if !meta.is_dir() {
            return Err(anyhow!("path is not a directory"));
        }

        let mut entries = fs::read_dir(&full_path).await?;

        Ok(entries.next_entry().await?.is_none())
    }

    /// Read physical directory entries without filtering hidden files
    pub async fn read_dir_physical(&self, subpath: &str) -> Result<Vec<(String, bool)>> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::symlink_metadata(&full_path).await?;
        if !meta.is_dir() {
            return Err(anyhow!("path is not a directory"));
        }
        let mut read_dir = fs::read_dir(&full_path).await?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().await?.is_dir();
            entries.push((name, is_dir));
        }
        Ok(entries)
    }

    /// List directory contents
    pub async fn list(&self, subpath: &str) -> Result<Vec<FileObj>> {
        let full_path = self.safe_resolve(subpath)?;
        let show_hidden = self.show_hidden;
        let full_path_clone = full_path.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<FileObj>> {
            let read_dir = std::fs::read_dir(&full_path_clone)
                .with_context(|| format!("failed to read directory: {:?}", full_path_clone))?;

            let mut items = Vec::new();

            for entry in read_dir {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let file_name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files if show_hidden is false
                if !show_hidden && file_name.starts_with('.') {
                    continue;
                }

                let meta = match std::fs::metadata(entry.path()) {
                    Ok(m) => m,
                    Err(_) => continue, // Skip unreadable entries or broken symlinks
                };

                let is_dir = meta.is_dir();
                let size = if is_dir { 0 } else { meta.len() as i64 };
                let modified = meta
                    .modified()
                    .ok()
                    .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
                    .unwrap_or_default();

                items.push(FileObj::new(file_name, size, is_dir, modified));
            }

            Ok(items)
        })
        .await
        .context("directory scan task panicked or failed")?
    }

    /// Get metadata for a single file or directory
    pub async fn get(&self, subpath: &str) -> Result<FileObj> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::metadata(&full_path)
            .await
            .with_context(|| format!("file not found: {:?}", full_path))?;

        let file_name = full_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() as i64 };
        let modified = meta
            .modified()
            .ok()
            .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
            .unwrap_or_default();

        Ok(FileObj::new(file_name, size, is_dir, modified))
    }

    /// Open file for reading
    pub async fn open(&self, subpath: &str) -> Result<fs::File> {
        let full_path = self.safe_resolve(subpath)?;
        let file = fs::File::open(&full_path)
            .await
            .with_context(|| format!("failed to open file: {:?}", full_path))?;
        Ok(file)
    }

    /// Create directory
    pub async fn mkdir(&self, subpath: &str) -> Result<()> {
        let full_path = self.safe_resolve(subpath)?;
        fs::create_dir_all(&full_path)
            .await
            .with_context(|| format!("failed to create directory: {:?}", full_path))?;
        Ok(())
    }

    /// Delete file or directory
    pub async fn remove(&self, subpath: &str) -> Result<()> {
        if subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot remove storage root"));
        }
        let full_path = self.safe_resolve(subpath)?;
        remove_path_recursive(&full_path).await
    }

    /// Safely rename an item with overwrite and backup restoration
    pub async fn rename_safe(
        &self,
        subpath: &str,
        new_name: &str,
        overwrite: bool,
    ) -> Result<(), RenameError> {
        self.rename_safe_internal(subpath, new_name, overwrite, false)
            .await
    }

    pub async fn rename_safe_internal(
        &self,
        subpath: &str,
        new_name: &str,
        overwrite: bool,
        simulate_second_step_failure: bool,
    ) -> Result<(), RenameError> {
        if subpath.trim_matches('/').is_empty() {
            return Err(RenameError::BadRequest("cannot rename storage root".into()));
        }
        if !crate::server::valid_name(new_name) {
            return Err(RenameError::BadRequest(format!(
                "invalid new name: {}",
                new_name
            )));
        }

        let src_path = self.safe_resolve(subpath).map_err(RenameError::Internal)?;
        let parent = src_path
            .parent()
            .ok_or_else(|| RenameError::BadRequest("cannot rename root".into()))?;
        let dst_path = parent.join(new_name);

        if src_path == dst_path {
            return Ok(());
        }

        if fs::symlink_metadata(&src_path).await.is_err() {
            return Err(RenameError::NotFound(format!(
                "source file [{}] not found",
                subpath
            )));
        }

        let dst_exists = fs::symlink_metadata(&dst_path).await.is_ok();
        if dst_exists {
            let src_canon = fs::canonicalize(&src_path).await.ok();
            let dst_canon = fs::canonicalize(&dst_path).await.ok();
            if src_canon.is_some() && src_canon == dst_canon {
                // Case-only / same physical file rename
                let temp = parent.join(format!(
                    ".rulist-rename-case-{}",
                    crate::auth::rand_string(24)
                ));
                fs::rename(&src_path, &temp)
                    .await
                    .map_err(|e| RenameError::Internal(e.into()))?;
                if let Err(err) = fs::rename(&temp, &dst_path).await {
                    let _ = fs::rename(&temp, &src_path).await;
                    return Err(RenameError::Internal(err.into()));
                }
                return Ok(());
            }

            if !overwrite {
                return Err(RenameError::Conflict(format!("file [{}] exists", new_name)));
            }

            let backup = parent.join(format!(
                ".rulist-rename-backup-{}",
                crate::auth::rand_string(24)
            ));
            fs::rename(&dst_path, &backup)
                .await
                .map_err(|e| RenameError::Internal(e.into()))?;

            if simulate_second_step_failure {
                let restore = fs::rename(&backup, &dst_path).await;
                if let Err(restore_err) = restore {
                    tracing::error!(
                        error = %restore_err,
                        "CRITICAL: failed to restore rename backup"
                    );
                }
                return Err(RenameError::Internal(anyhow!("simulated rename failure")));
            }

            match fs::rename(&src_path, &dst_path).await {
                Ok(_) => {
                    if let Err(err) = remove_path_recursive(&backup).await {
                        tracing::warn!(
                            error = %err,
                            path = ?backup,
                            "failed to remove backup after successful rename"
                        );
                    }
                    Ok(())
                }
                Err(err) => {
                    let restore = fs::rename(&backup, &dst_path).await;
                    if let Err(restore_err) = restore {
                        tracing::error!(
                            error = %restore_err,
                            "CRITICAL: failed to restore rename backup"
                        );
                    }
                    Err(RenameError::Internal(err.into()))
                }
            }
        } else {
            fs::rename(&src_path, &dst_path)
                .await
                .map_err(|e| RenameError::Internal(e.into()))?;
            Ok(())
        }
    }

    /// Two-phase rollback-safe batch rename within the same directory
    pub async fn batch_rename(
        &self,
        src_dir_subpath: &str,
        pairs: &[(String, String)],
    ) -> Result<(), RenameError> {
        let dir_path = self
            .safe_resolve(src_dir_subpath)
            .map_err(RenameError::Internal)?;

        if pairs.is_empty() {
            return Ok(());
        }

        // Pre-check phase
        for (src_name, new_name) in pairs {
            if !crate::server::valid_name(src_name) || !crate::server::valid_name(new_name) {
                return Err(RenameError::BadRequest("invalid filename".into()));
            }
        }

        let mut src_set = std::collections::HashSet::new();
        for (src, _) in pairs {
            if !src_set.insert(src.as_str()) {
                return Err(RenameError::Conflict(format!(
                    "duplicate source name [{src}]"
                )));
            }
        }

        let mut dst_set = std::collections::HashSet::new();
        for (_, dst) in pairs {
            if !dst_set.insert(dst.as_str()) {
                return Err(RenameError::Conflict(format!(
                    "duplicate target name [{dst}]"
                )));
            }
        }

        for (src, _) in pairs {
            let src_path = dir_path.join(src);
            if fs::symlink_metadata(&src_path).await.is_err() {
                return Err(RenameError::NotFound(format!("source [{src}] not found")));
            }
        }

        for (_, dst) in pairs {
            let dst_path = dir_path.join(dst);
            if fs::symlink_metadata(&dst_path).await.is_ok() && !src_set.contains(dst.as_str()) {
                return Err(RenameError::Conflict(format!(
                    "target [{dst}] already exists"
                )));
            }
        }

        let active_pairs: Vec<(&String, &String)> = pairs
            .iter()
            .filter(|(s, d)| s != d)
            .map(|(s, d)| (s, d))
            .collect();

        if active_pairs.is_empty() {
            return Ok(());
        }

        // Phase 1: Staging
        let mut staged: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();

        for (i, (src, dst)) in active_pairs.iter().enumerate() {
            let src_path = dir_path.join(src);
            let temp_name = format!(
                ".rulist-rename-stage-{}-{}",
                i,
                crate::auth::rand_string(16)
            );
            let temp_path = dir_path.join(temp_name);
            let final_path = dir_path.join(dst);

            if let Err(err) = fs::rename(&src_path, &temp_path).await {
                tracing::error!(error = %err, src = %src, "failed to stage file during batch rename");
                for (orig, staged_temp, _) in staged.iter().rev() {
                    if let Err(rb_err) = fs::rename(staged_temp, orig).await {
                        tracing::error!(
                            error = %rb_err,
                            "CRITICAL: batch rename rollback failed during stage 1"
                        );
                    }
                }
                return Err(RenameError::Internal(err.into()));
            }
            staged.push((src_path, temp_path, final_path));
        }

        // Phase 2: Final
        let mut finalized: Vec<(PathBuf, PathBuf)> = Vec::new();

        for (_, temp_path, final_path) in &staged {
            if let Err(err) = fs::rename(temp_path, final_path).await {
                tracing::error!(error = %err, dst = ?final_path, "failed to finalize file during batch rename");
                let mut rollback_failed = false;
                for (temp_p, final_p) in finalized.iter().rev() {
                    if let Err(rb_err) = fs::rename(final_p, temp_p).await {
                        tracing::error!(
                            error = %rb_err,
                            "CRITICAL: batch rename rollback failed restoring finalized file"
                        );
                        rollback_failed = true;
                    }
                }
                for (orig_p, temp_p, _) in staged.iter().rev() {
                    if let Err(rb_err) = fs::rename(temp_p, orig_p).await {
                        tracing::error!(
                            error = %rb_err,
                            "CRITICAL: batch rename rollback failed restoring original file"
                        );
                        rollback_failed = true;
                    }
                }
                if rollback_failed {
                    tracing::error!("CRITICAL: batch rename rollback failed");
                }
                return Err(RenameError::Internal(err.into()));
            }
            finalized.push((temp_path.clone(), final_path.clone()));
        }

        Ok(())
    }

    /// Move file or directory
    pub async fn move_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        self.move_to_safe(src_subpath, dst_subpath, true).await
    }

    /// Safely move a file or directory with overwrite conflict handling and rollback
    pub async fn move_to_safe(
        &self,
        src_subpath: &str,
        dst_subpath: &str,
        overwrite: bool,
    ) -> Result<()> {
        self.move_to_safe_internal(src_subpath, dst_subpath, overwrite, MoveFailurePoint::None)
            .await
    }

    pub async fn move_to_safe_internal(
        &self,
        src_subpath: &str,
        dst_subpath: &str,
        overwrite: bool,
        failure: MoveFailurePoint,
    ) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot move storage root"));
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;
        move_path_safe(&src_path, &dst_path, overwrite, failure).await
    }

    /// Copy file or directory
    pub async fn copy_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        self.copy_to_safe(src_subpath, dst_subpath, true).await
    }

    /// Safely copy a file or directory with staging, overwrite backup, and rollback
    pub async fn copy_to_safe(
        &self,
        src_subpath: &str,
        dst_subpath: &str,
        overwrite: bool,
    ) -> Result<()> {
        self.copy_to_safe_internal(src_subpath, dst_subpath, overwrite, false)
            .await
    }

    pub async fn copy_to_safe_internal(
        &self,
        src_subpath: &str,
        dst_subpath: &str,
        overwrite: bool,
        simulate_failure: bool,
    ) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot copy storage root"));
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;
        copy_path_safe(&src_path, &dst_path, overwrite, simulate_failure).await
    }
}

/// Safely copy a file or directory with staging, backup, and rollback.
pub(crate) async fn copy_path_safe(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    simulate_failure: bool,
) -> Result<()> {
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
    let rand = crate::auth::rand_string(24);
    let stage = parent.join(format!(".rulist-copy-stage-{}", rand));
    let backup = parent.join(format!(".rulist-backup-{}", rand));

    // Step A: copy src to staging path
    if let Err(err) = copy_path_recursive(src, &stage).await {
        let _ = remove_path_recursive(&stage).await;
        return Err(err.context("failed to copy source to staging directory"));
    }

    if simulate_failure {
        let _ = remove_path_recursive(&stage).await;
        return Err(anyhow!("simulated copy failure before destination backup"));
    }

    // Step B: backup existing destination if present
    if dst_exists {
        if let Err(err) = fs::rename(dst, &backup).await {
            let _ = remove_path_recursive(&stage).await;
            return Err(anyhow!(
                "failed to backup existing destination {:?}: {}",
                dst,
                err
            ));
        }
    }

    // Step C: promote stage to destination
    if let Err(err) = fs::rename(&stage, dst).await {
        if dst_exists {
            let restore_res = fs::rename(&backup, dst).await;
            if let Err(re) = restore_res {
                tracing::error!(
                    error = %re,
                    "CRITICAL: failed to restore backup after stage rename failure"
                );
            }
        }
        let _ = remove_path_recursive(&stage).await;
        return Err(anyhow!("failed to replace destination with stage: {}", err));
    }

    // Step D: remove backup if present
    if dst_exists {
        if let Err(err) = remove_path_recursive(&backup).await {
            tracing::warn!(
                error = %err,
                path = ?backup,
                "failed to remove backup after successful copy overwrite"
            );
        }
    }

    Ok(())
}

/// Safely move a file or directory across devices by staging, promoting, and cleaning up source.
pub(crate) async fn move_cross_device_safe(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    failure: MoveFailurePoint,
) -> Result<()> {
    let parent = dst
        .parent()
        .ok_or_else(|| anyhow!("destination has no parent"))?;

    fs::create_dir_all(parent).await?;

    let had_destination = fs::symlink_metadata(dst).await.is_ok();
    if had_destination && !overwrite {
        return Err(anyhow!("destination path already exists: {:?}", dst));
    }

    let rand = crate::auth::rand_string(24);
    let stage = parent.join(format!(".rulist-move-stage-{}", rand));
    let backup = parent.join(format!(".rulist-move-backup-{}", rand));

    // 1. Stage copy
    let copy_res = if failure == MoveFailurePoint::CrossDeviceCopy {
        Err(anyhow!("simulated cross-device copy failure"))
    } else {
        copy_path_recursive(src, &stage).await
    };

    if let Err(err) = copy_res {
        if let Err(clean_err) = remove_path_recursive(&stage).await {
            if !clean_err.to_string().contains("does not exist") {
                tracing::warn!(
                    error = %clean_err,
                    path = ?stage,
                    "failed to clean failed move staging path"
                );
            }
        }
        return Err(err.context("failed to stage cross-device move"));
    }

    // 2. Backup existing destination
    if had_destination {
        if let Err(err) = fs::rename(dst, &backup).await {
            let _ = remove_path_recursive(&stage).await;
            return Err(err).with_context(|| format!("failed to backup destination {:?}", dst));
        }
    }

    if failure == MoveFailurePoint::AfterDestinationBackup {
        if had_destination {
            let _ = fs::rename(&backup, dst).await;
        }
        let _ = remove_path_recursive(&stage).await;
        return Err(anyhow!("simulated move failure after destination backup"));
    }

    // 3. Promote stage to destination
    let promote_res = if failure == MoveFailurePoint::StagingPromotion {
        Err(anyhow!("simulated staging promotion failure"))
    } else {
        fs::rename(&stage, dst).await.map_err(anyhow::Error::from)
    };

    if let Err(err) = promote_res {
        if had_destination {
            if let Err(restore_err) = fs::rename(&backup, dst).await {
                tracing::error!(
                    error = %restore_err,
                    backup = ?backup,
                    dst = ?dst,
                    "CRITICAL: failed to restore destination backup"
                );
            }
        }
        let _ = remove_path_recursive(&stage).await;
        return Err(err.context("failed to promote staged move"));
    }

    // 4. Cleanup source
    let source_cleanup = if failure == MoveFailurePoint::SourceCleanup {
        Err(anyhow!("simulated source cleanup failure"))
    } else {
        remove_path_recursive(src).await
    };

    if let Err(err) = source_cleanup {
        tracing::error!(
            error = %err,
            src = ?src,
            dst = ?dst,
            backup = ?backup,
            "source cleanup failed after completed cross-device copy; preserving recovery data"
        );
        // CRITICAL:
        // Do NOT remove dst!
        // Do NOT restore backup!
        return Err(err.context("destination was copied successfully but source cleanup failed"));
    }

    // 5. Remove backup after successful operation
    if had_destination {
        if let Err(err) = remove_path_recursive(&backup).await {
            tracing::warn!(
                error = %err,
                backup = ?backup,
                "failed to remove move backup after successful operation"
            );
        }
    }

    Ok(())
}

/// Safely move a file or directory with backup and rollback on overwrite.
pub(crate) async fn move_path_safe(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    failure: MoveFailurePoint,
) -> Result<()> {
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
        match fs::rename(src, dst).await {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::CrossesDevices => {
                return move_cross_device_safe(src, dst, overwrite, failure).await;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("failed to move {:?} to {:?}", src, dst));
            }
        }
    }

    if !overwrite {
        return Err(anyhow!("destination path already exists: {:?}", dst));
    }

    let parent = dst.parent().ok_or_else(|| anyhow!("cannot move to root"))?;
    fs::create_dir_all(parent).await?;

    // Check same physical file / case-only rename
    let src_canon = fs::canonicalize(src).await.ok();
    let dst_canon = fs::canonicalize(dst).await.ok();
    if src_canon.is_some() && src_canon == dst_canon {
        let temp = parent.join(format!(
            ".rulist-move-case-{}",
            crate::auth::rand_string(24)
        ));
        fs::rename(src, &temp).await?;
        if let Err(err) = fs::rename(&temp, dst).await {
            let _ = fs::rename(&temp, src).await;
            return Err(err.into());
        }
        return Ok(());
    }

    let rand = crate::auth::rand_string(24);
    let backup = parent.join(format!(".rulist-backup-{}", rand));

    // Step A: move dst to backup
    if let Err(err) = fs::rename(dst, &backup).await {
        return Err(anyhow!(
            "failed to backup existing destination {:?}: {}",
            dst,
            err
        ));
    }

    if failure == MoveFailurePoint::AfterDestinationBackup {
        let _ = fs::rename(&backup, dst).await;
        return Err(anyhow!("simulated move failure after destination backup"));
    }

    // Step B: move src to dst
    match fs::rename(src, dst).await {
        Ok(()) => {
            // Step C: remove backup
            if let Err(err) = remove_path_recursive(&backup).await {
                tracing::warn!(
                    error = %err,
                    path = ?backup,
                    "failed to remove backup after successful move overwrite"
                );
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::CrossesDevices => {
            if let Err(re) = fs::rename(&backup, dst).await {
                tracing::error!(
                    error = %re,
                    "CRITICAL: failed to restore backup before cross-device move fallback"
                );
            }
            move_cross_device_safe(src, dst, overwrite, failure).await
        }
        Err(err) => {
            let restore_res = fs::rename(&backup, dst).await;
            if let Err(re) = restore_res {
                tracing::error!(
                    error = %re,
                    "CRITICAL: failed to restore backup after move failure"
                );
            }
            Err(err).with_context(|| format!("failed to move {:?} to {:?}", src, dst))
        }
    }
}

pub(crate) async fn copy_path_recursive(src: &Path, dst: &Path) -> Result<()> {
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

        while let Some((cur_src, cur_dst)) = stack.pop() {
            let mut entries = fs::read_dir(&cur_src)
                .await
                .with_context(|| format!("failed to read directory: {:?}", cur_src))?;

            while let Some(entry) = entries.next_entry().await? {
                let entry_path = entry.path();
                let file_type = entry.file_type().await?;

                if file_type.is_symlink() {
                    return Err(anyhow!("symlinks are not supported: {:?}", entry_path));
                }

                let target_path = cur_dst.join(entry.file_name());

                if file_type.is_dir() {
                    fs::create_dir_all(&target_path).await?;
                    stack.push((entry_path, target_path));
                } else if file_type.is_file() {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent).await?;
                    }
                    fs::copy(&entry_path, &target_path).await.with_context(|| {
                        format!("failed to copy {:?} to {:?}", entry_path, target_path)
                    })?;
                }
            }
        }
        return Ok(());
    }

    Err(anyhow!("unsupported file type for {:?}", src))
}

pub(crate) async fn remove_path_recursive(path: &Path) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_local_driver_crud() {
        let tmp = tempdir().unwrap();
        let json = format!(
            r#"{{"root_folder_path":"{}","show_hidden":false}}"#,
            tmp.path().to_str().unwrap()
        );
        let driver = LocalDriver::new(&json).unwrap();

        // Test mkdir
        driver.mkdir("folder1").await.unwrap();
        let list = driver.list("").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "folder1");
        assert!(list[0].is_dir);

        // Test create file & get
        let file_path = tmp.path().join("folder1/test.txt");
        tokio::fs::write(&file_path, b"hello rust!").await.unwrap();

        let file_obj = driver.get("folder1/test.txt").await.unwrap();
        assert_eq!(file_obj.name, "test.txt");
        assert_eq!(file_obj.size, 11);
        assert!(!file_obj.is_dir);

        // Test rename
        driver
            .rename_safe("folder1/test.txt", "renamed.txt", false)
            .await
            .unwrap();
        assert!(driver.get("folder1/renamed.txt").await.is_ok());

        // Test remove
        driver.remove("folder1/renamed.txt").await.unwrap();
        assert!(driver.get("folder1/renamed.txt").await.is_err());

        // Test path traversal security
        assert!(driver.safe_resolve("../etc/passwd").is_err());
        assert!(driver.safe_resolve("folder1/../../etc").is_err());
    }

    #[tokio::test]
    async fn rejects_storage_root_removal() {
        let tmp = tempdir().unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": tmp.path()}).to_string())
                .unwrap();
        assert!(driver.remove("").await.is_err());
        assert!(driver.remove("/").await.is_err());
        assert!(tmp.path().exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlinks_outside_storage() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": root.path()}).to_string())
                .unwrap();
        assert!(driver.safe_resolve("escape/secret.txt").is_err());
        assert!(driver.open("escape/secret.txt").await.is_err());
    }

    #[tokio::test]
    async fn test_copy_single_file() {
        let tmp = tempdir().unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": tmp.path()}).to_string())
                .unwrap();

        tokio::fs::write(tmp.path().join("file.txt"), b"sample content")
            .await
            .unwrap();
        driver.copy_to("file.txt", "file_copy.txt").await.unwrap();

        assert_eq!(
            tokio::fs::read(tmp.path().join("file.txt")).await.unwrap(),
            b"sample content"
        );
        assert_eq!(
            tokio::fs::read(tmp.path().join("file_copy.txt"))
                .await
                .unwrap(),
            b"sample content"
        );
    }

    #[tokio::test]
    async fn test_move_single_file_across_paths() {
        let tmp = tempdir().unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": tmp.path()}).to_string())
                .unwrap();

        tokio::fs::create_dir_all(tmp.path().join("src_dir"))
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("src_dir/file.txt"), b"move content")
            .await
            .unwrap();

        driver
            .move_to("src_dir/file.txt", "dst_dir/file_moved.txt")
            .await
            .unwrap();

        assert!(!tmp.path().join("src_dir/file.txt").exists());
        assert_eq!(
            tokio::fs::read(tmp.path().join("dst_dir/file_moved.txt"))
                .await
                .unwrap(),
            b"move content"
        );
    }

    #[tokio::test]
    async fn test_copy_empty_dir() {
        let tmp = tempdir().unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": tmp.path()}).to_string())
                .unwrap();

        driver.mkdir("empty_dir").await.unwrap();
        driver.copy_to("empty_dir", "empty_copy").await.unwrap();

        let meta = tokio::fs::metadata(tmp.path().join("empty_copy"))
            .await
            .unwrap();
        assert!(meta.is_dir());
        let list = driver.list("empty_copy").await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_copy_nested_multilevel_dir() {
        let tmp = tempdir().unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": tmp.path()}).to_string())
                .unwrap();

        let deep_dir = tmp.path().join("nested/sub1/sub2");
        tokio::fs::create_dir_all(&deep_dir).await.unwrap();
        tokio::fs::write(deep_dir.join("deep.txt"), b"deep content")
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join("nested/root_level.txt"), b"root content")
            .await
            .unwrap();

        driver.copy_to("nested", "nested_copy").await.unwrap();

        assert_eq!(
            tokio::fs::read(tmp.path().join("nested_copy/sub1/sub2/deep.txt"))
                .await
                .unwrap(),
            b"deep content"
        );
        assert_eq!(
            tokio::fs::read(tmp.path().join("nested_copy/root_level.txt"))
                .await
                .unwrap(),
            b"root content"
        );
    }

    #[tokio::test]
    async fn test_copy_unicode_chinese_filenames() {
        let tmp = tempdir().unwrap();
        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": tmp.path()}).to_string())
                .unwrap();

        let chinese_dir = tmp.path().join("中文目录/子目录");
        tokio::fs::create_dir_all(&chinese_dir).await.unwrap();
        let test_file = chinese_dir.join("测试文档.txt");
        tokio::fs::write(&test_file, "你好，世界！🦀".as_bytes())
            .await
            .unwrap();

        driver.copy_to("中文目录", "备份目录").await.unwrap();

        let copied_file = tmp.path().join("备份目录/子目录/测试文档.txt");
        assert!(copied_file.exists());
        assert_eq!(
            tokio::fs::read_to_string(&copied_file).await.unwrap(),
            "你好，世界！🦀"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_reject_symlink_copy() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        tokio::fs::write(&secret, b"sensitive data").await.unwrap();

        let symlink_path = root.path().join("link_to_outside");
        std::os::unix::fs::symlink(outside.path(), &symlink_path).unwrap();

        let driver =
            LocalDriver::new(&serde_json::json!({"root_folder_path": root.path()}).to_string())
                .unwrap();

        // 1. safe_resolve rejects top-level symlink
        assert!(
            driver
                .copy_to("link_to_outside", "copy_dest")
                .await
                .is_err()
        );

        // 2. copy_path_recursive directly rejects symlink source
        assert!(
            copy_path_recursive(&symlink_path, &root.path().join("copy_dest"))
                .await
                .is_err()
        );

        // 3. Directory containing symlink is rejected during recursive copy
        let dir_with_link = root.path().join("dir_with_link");
        tokio::fs::create_dir_all(&dir_with_link).await.unwrap();
        std::os::unix::fs::symlink(&secret, dir_with_link.join("link_file")).unwrap();

        assert!(driver.copy_to("dir_with_link", "dir_copy").await.is_err());
    }

    #[test]
    fn test_local_driver_new_validation() {
        let tmp = tempdir().unwrap();

        // 1. Empty root_folder_path -> fails
        assert!(LocalDriver::new(r#"{"root_folder_path":""}"#).is_err());
        assert!(LocalDriver::new(r#"{}"#).is_err());
        assert!(LocalDriver::new("").is_err());

        // 2. Relative path -> fails
        assert!(LocalDriver::new(r#"{"root_folder_path":"./relative/path"}"#).is_err());
        assert!(LocalDriver::new(r#"{"root_folder_path":"relative"}"#).is_err());

        // 3. Non-existent path -> fails
        let non_existent = tmp.path().join("does_not_exist");
        let json_non_existent = format!(
            r#"{{"root_folder_path":"{}"}}"#,
            non_existent.to_str().unwrap()
        );
        assert!(LocalDriver::new(&json_non_existent).is_err());

        // 4. Valid absolute path -> succeeds
        let valid_json = format!(
            r#"{{"root_folder_path":"{}"}}"#,
            tmp.path().to_str().unwrap()
        );
        let driver = LocalDriver::new(&valid_json).unwrap();
        assert_eq!(driver.root_path, tmp.path().canonicalize().unwrap());
    }

    #[tokio::test]
    async fn test_rename_safe_rollback_on_simulated_second_step_failure() {
        let tmp = tempdir().unwrap();
        let addition = serde_json::json!({
            "root_folder_path": tmp.path().to_str().unwrap()
        })
        .to_string();
        let driver = LocalDriver::new(&addition).unwrap();

        let src_file = tmp.path().join("src.txt");
        let dst_file = tmp.path().join("dst.txt");
        tokio::fs::write(&src_file, b"original source data")
            .await
            .unwrap();
        tokio::fs::write(&dst_file, b"original destination data")
            .await
            .unwrap();

        // Perform rename with simulate_second_step_failure = true
        let result = driver
            .rename_safe_internal("src.txt", "dst.txt", true, true)
            .await;
        assert!(result.is_err());

        // Verify that dst.txt was restored with its original content
        assert!(dst_file.exists());
        assert_eq!(
            tokio::fs::read(&dst_file).await.unwrap(),
            b"original destination data"
        );

        // Verify that src.txt is untouched
        assert!(src_file.exists());
        assert_eq!(
            tokio::fs::read(&src_file).await.unwrap(),
            b"original source data"
        );

        // Verify no leftover backup files remain
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.starts_with(".rulist-rename-backup-"));
        }
    }

    #[tokio::test]
    async fn test_copy_safe_rollback_on_simulated_failure() {
        let tmp = tempdir().unwrap();
        let addition = serde_json::json!({
            "root_folder_path": tmp.path().to_str().unwrap()
        })
        .to_string();
        let driver = LocalDriver::new(&addition).unwrap();

        let src_file = tmp.path().join("src.txt");
        let dst_file = tmp.path().join("dst.txt");
        tokio::fs::write(&src_file, b"new source data")
            .await
            .unwrap();
        tokio::fs::write(&dst_file, b"original destination data")
            .await
            .unwrap();

        // Perform copy with simulate_failure = true
        let result = driver
            .copy_to_safe_internal("src.txt", "dst.txt", true, true)
            .await;
        assert!(result.is_err());

        // Verify dst.txt is untouched with original content
        assert!(dst_file.exists());
        assert_eq!(
            tokio::fs::read(&dst_file).await.unwrap(),
            b"original destination data"
        );

        // Verify src.txt is untouched
        assert!(src_file.exists());
        assert_eq!(
            tokio::fs::read(&src_file).await.unwrap(),
            b"new source data"
        );

        // Verify no leftover stage or backup files remain
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.starts_with(".rulist-copy-stage-"));
            assert!(!name.starts_with(".rulist-backup-"));
        }
    }

    #[tokio::test]
    async fn test_copy_safe_nonexistent_dst_aborted_leaves_no_partial_destination() {
        let tmp = tempdir().unwrap();
        let addition = serde_json::json!({
            "root_folder_path": tmp.path().to_str().unwrap()
        })
        .to_string();
        let driver = LocalDriver::new(&addition).unwrap();

        let src_dir = tmp.path().join("src_dir");
        tokio::fs::create_dir(&src_dir).await.unwrap();
        tokio::fs::write(src_dir.join("a.txt"), b"file a")
            .await
            .unwrap();
        tokio::fs::write(src_dir.join("b.txt"), b"file b")
            .await
            .unwrap();
        tokio::fs::write(src_dir.join("c.txt"), b"file c")
            .await
            .unwrap();

        let dst_dir = tmp.path().join("dst_dir");

        // Copy to non-existent destination with simulate_failure = true
        let result = driver
            .copy_to_safe_internal("src_dir", "dst_dir", false, true)
            .await;
        assert!(result.is_err());

        // Source directory and all files remain intact
        assert!(src_dir.join("a.txt").exists());
        assert!(src_dir.join("b.txt").exists());
        assert!(src_dir.join("c.txt").exists());

        // Destination directory was never promoted and does NOT exist
        assert!(!dst_dir.exists());

        // No leftover staging or backup directories
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.starts_with(".rulist-copy-stage-"));
            assert!(!name.starts_with(".rulist-backup-"));
        }
    }

    #[tokio::test]
    async fn test_move_safe_rollback_on_simulated_failure() {
        let tmp = tempdir().unwrap();
        let addition = serde_json::json!({
            "root_folder_path": tmp.path().to_str().unwrap()
        })
        .to_string();
        let driver = LocalDriver::new(&addition).unwrap();

        let src_file = tmp.path().join("src.txt");
        let dst_file = tmp.path().join("dst.txt");
        tokio::fs::write(&src_file, b"new source data")
            .await
            .unwrap();
        tokio::fs::write(&dst_file, b"original destination data")
            .await
            .unwrap();

        // Perform move with simulate_failure = true
        let result = driver
            .move_to_safe_internal(
                "src.txt",
                "dst.txt",
                true,
                MoveFailurePoint::AfterDestinationBackup,
            )
            .await;
        assert!(result.is_err());

        // Verify dst.txt was restored with original content
        assert!(dst_file.exists());
        assert_eq!(
            tokio::fs::read(&dst_file).await.unwrap(),
            b"original destination data"
        );

        // Verify src.txt is untouched
        assert!(src_file.exists());
        assert_eq!(
            tokio::fs::read(&src_file).await.unwrap(),
            b"new source data"
        );

        // Verify no leftover backup files remain
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.starts_with(".rulist-backup-"));
        }
    }

    #[tokio::test]
    async fn test_cross_device_move_preserves_dst_on_source_cleanup_failure() {
        let tmp = tempdir().unwrap();
        let src_file = tmp.path().join("src.txt");
        let dst_file = tmp.path().join("dst.txt");
        tokio::fs::write(&src_file, b"new source data")
            .await
            .unwrap();
        tokio::fs::write(&dst_file, b"original destination data")
            .await
            .unwrap();

        let result =
            move_cross_device_safe(&src_file, &dst_file, true, MoveFailurePoint::SourceCleanup)
                .await;
        assert!(result.is_err());

        // CRITICAL: Completed dst is preserved!
        assert!(dst_file.exists());
        assert_eq!(
            tokio::fs::read(&dst_file).await.unwrap(),
            b"new source data"
        );

        // Source is preserved
        assert!(src_file.exists());
        assert_eq!(
            tokio::fs::read(&src_file).await.unwrap(),
            b"new source data"
        );

        // Backup file is preserved so recovery of old destination is possible
        let mut backup_found = false;
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(".rulist-move-backup-") {
                backup_found = true;
                assert_eq!(
                    tokio::fs::read(entry.path()).await.unwrap(),
                    b"original destination data"
                );
            }
        }
        assert!(backup_found);
    }

    #[tokio::test]
    async fn test_cross_device_move_staging_failure() {
        let tmp = tempdir().unwrap();
        let src_file = tmp.path().join("src.txt");
        let dst_file = tmp.path().join("dst.txt");
        tokio::fs::write(&src_file, b"new source data")
            .await
            .unwrap();
        tokio::fs::write(&dst_file, b"original destination data")
            .await
            .unwrap();

        let result = move_cross_device_safe(
            &src_file,
            &dst_file,
            true,
            MoveFailurePoint::CrossDeviceCopy,
        )
        .await;
        assert!(result.is_err());

        // Both src and dst are untouched
        assert_eq!(
            tokio::fs::read(&src_file).await.unwrap(),
            b"new source data"
        );
        assert_eq!(
            tokio::fs::read(&dst_file).await.unwrap(),
            b"original destination data"
        );

        // No leftover staging or backup files
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.starts_with(".rulist-move-stage-"));
            assert!(!name.starts_with(".rulist-move-backup-"));
        }
    }

    #[tokio::test]
    async fn test_cross_device_move_promotion_failure() {
        let tmp = tempdir().unwrap();
        let src_file = tmp.path().join("src.txt");
        let dst_file = tmp.path().join("dst.txt");
        tokio::fs::write(&src_file, b"new source data")
            .await
            .unwrap();
        tokio::fs::write(&dst_file, b"original destination data")
            .await
            .unwrap();

        let result = move_cross_device_safe(
            &src_file,
            &dst_file,
            true,
            MoveFailurePoint::StagingPromotion,
        )
        .await;
        assert!(result.is_err());

        // Backup was restored to dst
        assert_eq!(
            tokio::fs::read(&dst_file).await.unwrap(),
            b"original destination data"
        );
        assert_eq!(
            tokio::fs::read(&src_file).await.unwrap(),
            b"new source data"
        );

        // No leftover staging or backup files
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.starts_with(".rulist-move-stage-"));
            assert!(!name.starts_with(".rulist-move-backup-"));
        }
    }

    #[tokio::test]
    async fn test_non_cross_device_error_does_not_fallback_to_copy() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempdir().unwrap();
        let ro_dir = tmp.path().join("readonly");
        tokio::fs::create_dir(&ro_dir).await.unwrap();

        let src_file = tmp.path().join("src.txt");
        tokio::fs::write(&src_file, b"source data").await.unwrap();

        let dst_file = ro_dir.join("dst.txt");

        // Make ro_dir read-only so rename fails with PermissionDenied
        tokio::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o555))
            .await
            .unwrap();

        let result = move_path_safe(&src_file, &dst_file, false, MoveFailurePoint::None).await;
        assert!(result.is_err());

        // Restore permissions for cleanup
        tokio::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();

        // Source file is intact and destination does not exist
        assert!(src_file.exists());
        assert!(!dst_file.exists());
    }

    #[tokio::test]
    async fn test_local_driver_list_correctness_and_edge_cases() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();

        // 1. Create a regular file
        let regular_file = root.join("hello.txt");
        tokio::fs::write(&regular_file, b"12345").await.unwrap();

        // 2. Create a subdirectory
        let sub_dir = root.join("sub_directory");
        tokio::fs::create_dir(&sub_dir).await.unwrap();

        // 3. Create a hidden file
        let hidden_file = root.join(".hidden_file");
        tokio::fs::write(&hidden_file, b"secret").await.unwrap();

        // 4. Create a broken symlink (unreadable / invalid target)
        let broken_link = root.join("broken_link.txt");
        let non_existent_target = root.join("does_not_exist.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&non_existent_target, &broken_link).unwrap();

        // Test with show_hidden = false
        let driver_no_hidden = LocalDriver {
            root_path: root.to_path_buf(),
            show_hidden: false,
        };
        let items = driver_no_hidden.list("").await.unwrap();
        let names: Vec<String> = items.iter().map(|i| i.name.clone()).collect();

        // .hidden_file must NOT be included
        assert!(!names.contains(&".hidden_file".to_string()));
        // broken_link is unreadable/broken, metadata() fails, so it must be safely skipped
        #[cfg(unix)]
        assert!(!names.contains(&"broken_link.txt".to_string()));

        // hello.txt must have size 5, not a dir, and valid modified date
        let file_obj = items.iter().find(|i| i.name == "hello.txt").unwrap();
        assert_eq!(file_obj.size, 5);
        assert!(!file_obj.is_dir);
        assert!(!file_obj.modified.is_empty());

        // sub_directory must have size 0, is_dir = true
        let dir_obj = items.iter().find(|i| i.name == "sub_directory").unwrap();
        assert_eq!(dir_obj.size, 0);
        assert!(dir_obj.is_dir);
        assert!(!dir_obj.modified.is_empty());

        // Test with show_hidden = true
        let driver_show_hidden = LocalDriver {
            root_path: root.to_path_buf(),
            show_hidden: true,
        };
        let items_hidden = driver_show_hidden.list("").await.unwrap();
        let names_hidden: Vec<String> = items_hidden.iter().map(|i| i.name.clone()).collect();
        assert!(names_hidden.contains(&".hidden_file".to_string()));

        // Test non-existent path returns error
        assert!(driver_no_hidden.list("non_existent_subpath").await.is_err());
    }
}
