use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

use crate::filesystem::{FileEntry, FsError, valid_name};

use super::ops::{
    copy_path_safe, entry_exists, move_path_safe, remove_path_recursive, rename_no_replace,
};

#[derive(Debug, Clone)]
pub struct LocalFs {
    pub root_path: PathBuf,
    pub show_hidden: bool,
}

impl LocalFs {
    pub fn new(root_path: impl AsRef<Path>, show_hidden: bool) -> Result<Self> {
        let root_path = root_path.as_ref();
        if root_path.as_os_str().is_empty() {
            return Err(anyhow!("local_path cannot be empty"));
        }
        if !root_path.is_absolute() {
            return Err(anyhow!("local_path must be an absolute path"));
        }

        Ok(Self {
            root_path: root_path.to_path_buf(),
            show_hidden,
        })
    }

    fn hidden_name_denied(&self, name: &str) -> bool {
        !self.show_hidden && name.starts_with('.')
    }

    pub fn safe_resolve(&self, subpath: &str) -> Result<PathBuf> {
        let clean = subpath.trim_matches('/');
        let mut target = self.root_path.clone();

        for component in Path::new(clean).components() {
            match component {
                Component::Normal(value) => {
                    if self.hidden_name_denied(&value.to_string_lossy()) {
                        return Err(FsError::Forbidden.into());
                    }
                    target.push(value);
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(FsError::InvalidPath.into());
                }
                _ => {
                    return Err(FsError::InvalidPath.into());
                }
            }

            match std::fs::symlink_metadata(&target) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(FsError::Forbidden.into());
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        Ok(target)
    }

    pub async fn entry_exists(&self, subpath: &str) -> Result<bool> {
        let clean = subpath.trim_matches('/');
        if clean.is_empty() {
            return entry_exists(&self.root_path).await;
        }

        let relative = Path::new(clean);
        let name = relative
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| valid_name(name))
            .ok_or(FsError::InvalidPath)?;
        if self.hidden_name_denied(name) {
            return Err(FsError::Forbidden.into());
        }
        let parent = relative.parent().and_then(Path::to_str).unwrap_or_default();
        let parent = self.safe_resolve(parent)?;
        entry_exists(&parent.join(name)).await
    }

    pub async fn list(&self, subpath: &str) -> Result<Vec<FileEntry>> {
        let full_path = self.safe_resolve(subpath)?;
        let show_hidden = self.show_hidden;

        tokio::task::spawn_blocking(move || -> Result<Vec<FileEntry>> {
            let read_dir = std::fs::read_dir(&full_path)
                .with_context(|| format!("failed to read directory: {:?}", full_path))?;
            let mut items = Vec::new();

            for entry in read_dir.flatten() {
                let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                    tracing::warn!(
                        path = ?entry.path(),
                        "skipping non-UTF-8 filename"
                    );
                    continue;
                };
                if !show_hidden && file_name.starts_with('.') {
                    continue;
                }

                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
                    continue;
                }
                if !file_type.is_file() && !file_type.is_dir() {
                    tracing::debug!(
                        path = ?entry.path(),
                        "skipping unsupported filesystem entry"
                    );
                    continue;
                }
                let Ok(meta) = std::fs::metadata(entry.path()) else {
                    continue;
                };

                let is_dir = meta.is_dir();
                let size = if is_dir { 0 } else { meta.len() as i64 };
                let modified = meta
                    .modified()
                    .ok()
                    .map(|time| DateTime::<Utc>::from(time).to_rfc3339())
                    .unwrap_or_default();

                let permissions = crate::filesystem::format_mode(meta.permissions().mode(), is_dir);

                let mut item = FileEntry::new(file_name, size, is_dir, modified);
                item.permissions = Some(permissions);
                items.push(item);
            }

            Ok(items)
        })
        .await
        .context("directory scan task panicked or failed")?
    }

    pub async fn get(&self, subpath: &str) -> Result<FileEntry> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::symlink_metadata(&full_path)
            .await
            .with_context(|| format!("file not found: {:?}", full_path))?;
        let file_type = meta.file_type();
        if !file_type.is_file() && !file_type.is_dir() {
            return Err(FsError::InvalidPath.into());
        }
        let file_name = match full_path.file_name() {
            Some(name) => name
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("filename is not valid UTF-8"))?,
            None => "/".to_string(),
        };
        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() as i64 };
        let modified = meta
            .modified()
            .ok()
            .map(|time| DateTime::<Utc>::from(time).to_rfc3339())
            .unwrap_or_default();

        let permissions = crate::filesystem::format_mode(meta.permissions().mode(), is_dir);

        let mut item = FileEntry::new(file_name, size, is_dir, modified);
        item.permissions = Some(permissions);
        Ok(item)
    }

    pub async fn open(&self, subpath: &str) -> Result<fs::File> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::symlink_metadata(&full_path)
            .await
            .with_context(|| format!("failed to inspect file: {:?}", full_path))?;
        if !meta.file_type().is_file() {
            return Err(FsError::InvalidPath.into());
        }
        fs::File::open(&full_path)
            .await
            .with_context(|| format!("failed to open file: {:?}", full_path))
    }

    pub async fn mkdir(&self, subpath: &str) -> Result<()> {
        let full_path = self.safe_resolve(subpath)?;
        fs::create_dir_all(&full_path)
            .await
            .with_context(|| format!("failed to create directory: {:?}", full_path))?;
        Ok(())
    }

    pub async fn remove(&self, subpath: &str) -> Result<()> {
        if subpath.trim_matches('/').is_empty() {
            return Err(FsError::InvalidPath.into());
        }
        let full_path = self.safe_resolve(subpath)?;
        remove_path_recursive(&full_path).await
    }

    pub async fn rename_safe(&self, subpath: &str, new_name: &str, overwrite: bool) -> Result<()> {
        if subpath.trim_matches('/').is_empty() {
            return Err(FsError::InvalidPath.into());
        }
        if !valid_name(new_name) {
            return Err(FsError::InvalidPath.into());
        }
        if self.hidden_name_denied(new_name) {
            return Err(FsError::Forbidden.into());
        }

        let src_path = self.safe_resolve(subpath)?;
        let parent = src_path.parent().ok_or(FsError::InvalidPath)?;
        let dst_path = parent.join(new_name);
        if src_path == dst_path {
            return Ok(());
        }
        fs::symlink_metadata(&src_path).await?;

        let dst_exists = entry_exists(&dst_path).await?;
        if !dst_exists {
            if overwrite {
                fs::rename(&src_path, &dst_path).await?;
            } else {
                rename_no_replace(&src_path, &dst_path).await?;
            }
            return Ok(());
        }

        let src_canon = fs::canonicalize(&src_path).await.ok();
        let dst_canon = fs::canonicalize(&dst_path).await.ok();
        if src_canon.is_some() && src_canon == dst_canon {
            let temp = parent.join(format!(
                ".rulist-rename-case-{}",
                crate::auth::rand_string(24)
            ));
            rename_no_replace(&src_path, &temp).await?;
            let result = if overwrite {
                fs::rename(&temp, &dst_path).await
            } else {
                rename_no_replace(&temp, &dst_path).await
            };
            if let Err(error) = result {
                if let Err(restore_error) = rename_no_replace(&temp, &src_path).await {
                    tracing::error!(
                        error = %restore_error,
                        "CRITICAL: failed to restore source after case rename failure"
                    );
                }
                return Err(error.into());
            }
            return Ok(());
        }

        if !overwrite {
            return Err(FsError::Conflict.into());
        }

        let backup = parent.join(format!(
            ".rulist-rename-backup-{}",
            crate::auth::rand_string(24)
        ));
        rename_no_replace(&dst_path, &backup).await?;

        match fs::rename(&src_path, &dst_path).await {
            Ok(()) => {
                if let Err(error) = remove_path_recursive(&backup).await {
                    tracing::warn!(
                        error = %error,
                        path = ?backup,
                        "failed to remove backup after successful rename"
                    );
                }
                Ok(())
            }
            Err(error) => {
                if let Err(restore_error) = rename_no_replace(&backup, &dst_path).await {
                    tracing::error!(
                        error = %restore_error,
                        "CRITICAL: failed to restore rename backup"
                    );
                }
                Err(error.into())
            }
        }
    }

    pub async fn batch_rename(
        &self,
        src_dir_subpath: &str,
        pairs: &[(String, String)],
    ) -> Result<()> {
        let dir_path = self.safe_resolve(src_dir_subpath)?;
        if pairs.is_empty() {
            return Ok(());
        }

        for (src_name, new_name) in pairs {
            if !valid_name(src_name) || !valid_name(new_name) {
                return Err(FsError::InvalidPath.into());
            }
            if self.hidden_name_denied(src_name) || self.hidden_name_denied(new_name) {
                return Err(FsError::Forbidden.into());
            }
        }

        let mut src_set = std::collections::HashSet::new();
        for (src, _) in pairs {
            if !src_set.insert(src.as_str()) {
                return Err(FsError::Conflict.into());
            }
        }
        let mut dst_set = std::collections::HashSet::new();
        for (_, dst) in pairs {
            if !dst_set.insert(dst.as_str()) {
                return Err(FsError::Conflict.into());
            }
        }

        for (src, _) in pairs {
            self.safe_resolve(&format!(
                "{}/{}",
                src_dir_subpath.trim_end_matches('/'),
                src
            ))?;
            let src_path = dir_path.join(src);
            fs::symlink_metadata(src_path).await?;
        }
        for (_, dst) in pairs {
            if entry_exists(&dir_path.join(dst)).await? && !src_set.contains(dst.as_str()) {
                return Err(FsError::Conflict.into());
            }
        }

        let active_pairs: Vec<_> = pairs.iter().filter(|(src, dst)| src != dst).collect();
        if active_pairs.is_empty() {
            return Ok(());
        }

        let mut staged: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();
        for (index, (src, dst)) in active_pairs.iter().enumerate() {
            let src_path = dir_path.join(src);
            let temp_path = dir_path.join(format!(
                ".rulist-rename-stage-{index}-{}",
                crate::auth::rand_string(16)
            ));
            let final_path = dir_path.join(dst);

            if let Err(error) = rename_no_replace(&src_path, &temp_path).await {
                for (original, staged_temp, _) in staged.iter().rev() {
                    if let Err(rollback_error) = rename_no_replace(staged_temp, original).await {
                        tracing::error!(
                            error = %rollback_error,
                            "CRITICAL: batch rename rollback failed during staging"
                        );
                    }
                }
                return Err(error.into());
            }
            staged.push((src_path, temp_path, final_path));
        }

        let mut finalized: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (_, temp_path, final_path) in &staged {
            if let Err(error) = rename_no_replace(temp_path, final_path).await {
                for (temp, final_path) in finalized.iter().rev() {
                    let _ = rename_no_replace(final_path, temp).await;
                }
                for (original, temp, _) in staged.iter().rev() {
                    if let Err(rollback_error) = rename_no_replace(temp, original).await {
                        tracing::error!(
                            error = %rollback_error,
                            "CRITICAL: batch rename rollback failed restoring original file"
                        );
                    }
                }
                return Err(error.into());
            }
            finalized.push((temp_path.clone(), final_path.clone()));
        }

        Ok(())
    }

    pub async fn move_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        self.move_to_safe(src_subpath, dst_subpath, true).await
    }

    pub async fn move_to_safe(
        &self,
        src_subpath: &str,
        dst_subpath: &str,
        overwrite: bool,
    ) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(FsError::InvalidPath.into());
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;
        move_path_safe(&src_path, &dst_path, overwrite).await
    }

    pub async fn copy_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        self.copy_to_safe(src_subpath, dst_subpath, true).await
    }

    pub async fn copy_to_safe(
        &self,
        src_subpath: &str,
        dst_subpath: &str,
        overwrite: bool,
    ) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(FsError::InvalidPath.into());
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;
        copy_path_safe(&src_path, &dst_path, overwrite).await
    }
}
