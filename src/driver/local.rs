use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

use crate::model::FileObj;

use super::local_ops::{copy_path_safe, move_path_safe, remove_path_recursive};

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

impl LocalDriver {
    pub fn new(addition_json: &str) -> Result<Self> {
        if addition_json.is_empty() {
            return Err(anyhow!("storage configuration is empty"));
        }
        let addition: LocalAddition =
            serde_json::from_str(addition_json).context("failed to parse storage configuration")?;

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

        Ok(Self {
            root_path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
            show_hidden: addition.show_hidden,
        })
    }

    pub fn safe_resolve(&self, subpath: &str) -> Result<PathBuf> {
        let clean = subpath.trim_matches('/');
        let mut target = self.root_path.clone();

        for component in Path::new(clean).components() {
            match component {
                Component::Normal(value) => target.push(value),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(anyhow!(
                        "access denied: parent directory traversal is forbidden"
                    ));
                }
                _ => {
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
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        Ok(target)
    }

    pub async fn is_physically_empty(&self, subpath: &str) -> Result<bool> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::symlink_metadata(&full_path).await?;
        if !meta.is_dir() {
            return Err(anyhow!("path is not a directory"));
        }
        let mut entries = fs::read_dir(&full_path).await?;
        Ok(entries.next_entry().await?.is_none())
    }

    pub async fn read_dir_physical(&self, subpath: &str) -> Result<Vec<(String, bool)>> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::symlink_metadata(&full_path).await?;
        if !meta.is_dir() {
            return Err(anyhow!("path is not a directory"));
        }

        let mut read_dir = fs::read_dir(&full_path).await?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_symlink() {
                continue;
            }
            entries.push((
                entry.file_name().to_string_lossy().to_string(),
                file_type.is_dir(),
            ));
        }
        Ok(entries)
    }

    pub async fn list(&self, subpath: &str) -> Result<Vec<FileObj>> {
        let full_path = self.safe_resolve(subpath)?;
        let show_hidden = self.show_hidden;

        tokio::task::spawn_blocking(move || -> Result<Vec<FileObj>> {
            let read_dir = std::fs::read_dir(&full_path)
                .with_context(|| format!("failed to read directory: {:?}", full_path))?;
            let mut items = Vec::new();

            for entry in read_dir.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if !show_hidden && file_name.starts_with('.') {
                    continue;
                }

                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
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

                let permissions = crate::model::format_mode(meta.permissions().mode(), is_dir);

                let mut item = FileObj::new(file_name, size, is_dir, modified);
                item.permissions = Some(permissions);
                items.push(item);
            }

            Ok(items)
        })
        .await
        .context("directory scan task panicked or failed")?
    }

    pub async fn get(&self, subpath: &str) -> Result<FileObj> {
        let full_path = self.safe_resolve(subpath)?;
        let meta = fs::metadata(&full_path)
            .await
            .with_context(|| format!("file not found: {:?}", full_path))?;
        let file_name = full_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() as i64 };
        let modified = meta
            .modified()
            .ok()
            .map(|time| DateTime::<Utc>::from(time).to_rfc3339())
            .unwrap_or_default();

        let permissions = crate::model::format_mode(meta.permissions().mode(), is_dir);

        let mut item = FileObj::new(file_name, size, is_dir, modified);
        item.permissions = Some(permissions);
        Ok(item)
    }

    pub async fn open(&self, subpath: &str) -> Result<fs::File> {
        let full_path = self.safe_resolve(subpath)?;
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
            return Err(anyhow!("cannot remove storage root"));
        }
        let full_path = self.safe_resolve(subpath)?;
        remove_path_recursive(&full_path).await
    }

    pub async fn rename_safe(
        &self,
        subpath: &str,
        new_name: &str,
        overwrite: bool,
    ) -> Result<(), RenameError> {
        if subpath.trim_matches('/').is_empty() {
            return Err(RenameError::BadRequest("cannot rename storage root".into()));
        }
        if !crate::server::valid_name(new_name) {
            return Err(RenameError::BadRequest(format!(
                "invalid new name: {new_name}"
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
                "source file [{subpath}] not found"
            )));
        }

        let dst_exists = fs::symlink_metadata(&dst_path).await.is_ok();
        if !dst_exists {
            fs::rename(&src_path, &dst_path)
                .await
                .map_err(|error| RenameError::Internal(error.into()))?;
            return Ok(());
        }

        let src_canon = fs::canonicalize(&src_path).await.ok();
        let dst_canon = fs::canonicalize(&dst_path).await.ok();
        if src_canon.is_some() && src_canon == dst_canon {
            let temp = parent.join(format!(
                ".rulist-rename-case-{}",
                crate::auth::rand_string(24)
            ));
            fs::rename(&src_path, &temp)
                .await
                .map_err(|error| RenameError::Internal(error.into()))?;
            if let Err(error) = fs::rename(&temp, &dst_path).await {
                let _ = fs::rename(&temp, &src_path).await;
                return Err(RenameError::Internal(error.into()));
            }
            return Ok(());
        }

        if !overwrite {
            return Err(RenameError::Conflict(format!("file [{new_name}] exists")));
        }

        let backup = parent.join(format!(
            ".rulist-rename-backup-{}",
            crate::auth::rand_string(24)
        ));
        fs::rename(&dst_path, &backup)
            .await
            .map_err(|error| RenameError::Internal(error.into()))?;

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
                if let Err(restore_error) = fs::rename(&backup, &dst_path).await {
                    tracing::error!(
                        error = %restore_error,
                        "CRITICAL: failed to restore rename backup"
                    );
                }
                Err(RenameError::Internal(error.into()))
            }
        }
    }

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
            if fs::symlink_metadata(dir_path.join(src)).await.is_err() {
                return Err(RenameError::NotFound(format!("source [{src}] not found")));
            }
        }
        for (_, dst) in pairs {
            if fs::symlink_metadata(dir_path.join(dst)).await.is_ok()
                && !src_set.contains(dst.as_str())
            {
                return Err(RenameError::Conflict(format!(
                    "target [{dst}] already exists"
                )));
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

            if let Err(error) = fs::rename(&src_path, &temp_path).await {
                for (original, staged_temp, _) in staged.iter().rev() {
                    if let Err(rollback_error) = fs::rename(staged_temp, original).await {
                        tracing::error!(
                            error = %rollback_error,
                            "CRITICAL: batch rename rollback failed during staging"
                        );
                    }
                }
                return Err(RenameError::Internal(error.into()));
            }
            staged.push((src_path, temp_path, final_path));
        }

        let mut finalized: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (_, temp_path, final_path) in &staged {
            if let Err(error) = fs::rename(temp_path, final_path).await {
                for (temp, final_path) in finalized.iter().rev() {
                    let _ = fs::rename(final_path, temp).await;
                }
                for (original, temp, _) in staged.iter().rev() {
                    if let Err(rollback_error) = fs::rename(temp, original).await {
                        tracing::error!(
                            error = %rollback_error,
                            "CRITICAL: batch rename rollback failed restoring original file"
                        );
                    }
                }
                return Err(RenameError::Internal(error.into()));
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
            return Err(anyhow!("cannot move storage root"));
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
            return Err(anyhow!("cannot copy storage root"));
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;
        copy_path_safe(&src_path, &dst_path, overwrite).await
    }
}
