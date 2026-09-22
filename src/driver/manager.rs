#![allow(dead_code)]

use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::fs;

use crate::db::DbPool;
use crate::driver::local::{LocalDriver, RenameError};
use crate::model::{FileObj, Storage};

#[derive(Clone)]
pub struct MountedStorage {
    pub storage: Storage,
    pub driver: LocalDriver,
}

#[derive(Clone, Default)]
pub struct StorageManager {
    storages: Arc<std::sync::RwLock<Vec<MountedStorage>>>,
}

impl StorageManager {
    pub async fn load_from_db(pool: &DbPool) -> Result<Self> {
        let rows = sqlx::query_as::<_, Storage>(
            "SELECT * FROM `x_storages` WHERE `disabled` = 0 ORDER BY `order` ASC, `id` ASC",
        )
        .fetch_all(pool)
        .await?;

        let mut storages = Vec::new();
        for s in rows {
            let addition = s.addition.clone().unwrap_or_default();
            match LocalDriver::new(&addition) {
                Ok(driver) => {
                    storages.push(MountedStorage { storage: s, driver });
                }
                Err(err) => {
                    tracing::warn!("failed to mount storage {}: {:?}", s.mount_path, err);
                    let _ =
                        sqlx::query("UPDATE `x_storages` SET `status` = 'invalid' WHERE `id` = ?")
                            .bind(s.id)
                            .execute(pool)
                            .await;
                }
            }
        }

        Ok(Self {
            storages: Arc::new(std::sync::RwLock::new(storages)),
        })
    }

    pub async fn reload_from_db(&self, pool: &DbPool) -> Result<()> {
        let new_manager = Self::load_from_db(pool).await?;
        let new_storages = new_manager.storages.read().unwrap().clone();
        let mut w = self.storages.write().unwrap();
        *w = new_storages;
        Ok(())
    }

    /// Match the best storage for a given request path
    pub fn find_storage(&self, req_path: &str) -> Option<(MountedStorage, String)> {
        let clean_path = if req_path.is_empty() || !req_path.starts_with('/') {
            format!("/{}", req_path)
        } else {
            req_path.to_string()
        };

        let storages = self.storages.read().unwrap();
        let mut matched: Option<(MountedStorage, String)> = None;
        let mut max_prefix_len = 0;

        for ms in storages.iter() {
            let mount = &ms.storage.mount_path;
            if mount == "/" {
                if max_prefix_len == 0 {
                    matched = Some((ms.clone(), clean_path.trim_start_matches('/').to_string()));
                }
            } else if clean_path == *mount {
                return Some((ms.clone(), String::new()));
            } else if clean_path.starts_with(mount)
                && clean_path.as_bytes().get(mount.len()) == Some(&b'/')
                && mount.len() > max_prefix_len
            {
                max_prefix_len = mount.len();
                let sub = &clean_path[mount.len()..];
                matched = Some((ms.clone(), sub.trim_start_matches('/').to_string()));
            }
        }

        matched
    }

    /// List directory contents (virtual root or driver delegator)
    pub async fn list(&self, req_path: &str) -> Result<Vec<FileObj>> {
        let clean_path = req_path.trim_matches('/');

        // Root virtual directory listing when no storage is mounted directly at '/'
        if clean_path.is_empty() {
            if let Some((ms, sub)) = self.find_storage("/")
                && ms.storage.mount_path == "/"
            {
                return ms.driver.list(&sub).await;
            }

            // Return virtual folders for all mount paths
            let mut list = Vec::new();
            let storages = self.storages.read().unwrap();
            for ms in storages.iter() {
                let name = ms.storage.mount_path.trim_matches('/').to_string();
                if !name.is_empty() {
                    list.push(FileObj::new(name, 0, true, ""));
                }
            }
            return Ok(list);
        }

        if let Some((ms, sub)) = self.find_storage(req_path) {
            ms.driver.list(&sub).await
        } else {
            Err(anyhow!("mount storage not found for path: {}", req_path))
        }
    }

    /// Check if a path is physically empty on disk
    pub async fn is_physically_empty(&self, req_path: &str) -> Result<bool> {
        let (storage, subpath) = self
            .find_storage(req_path)
            .ok_or_else(|| anyhow!("storage not found"))?;

        storage.driver.is_physically_empty(&subpath).await
    }

    /// Read physical directory entries without filtering hidden files
    pub async fn read_dir_physical(&self, req_path: &str) -> Result<Vec<(String, bool)>> {
        let (storage, subpath) = self
            .find_storage(req_path)
            .ok_or_else(|| anyhow!("storage not found"))?;

        storage.driver.read_dir_physical(&subpath).await
    }

    /// Get object metadata
    pub async fn get(&self, req_path: &str) -> Result<FileObj> {
        let clean = req_path.trim_matches('/');
        if clean.is_empty() {
            return Ok(FileObj::new("/", 0, true, ""));
        }

        // Check if path is exactly a virtual mount point
        let is_mount = {
            let storages = self.storages.read().unwrap();
            storages
                .iter()
                .any(|ms| ms.storage.mount_path.trim_matches('/') == clean)
        };
        if is_mount {
            return Ok(FileObj::new(clean, 0, true, ""));
        }

        if let Some((ms, sub)) = self.find_storage(req_path) {
            ms.driver.get(&sub).await
        } else {
            Err(anyhow!("path not found: {}", req_path))
        }
    }

    /// Open file
    pub async fn open(&self, req_path: &str) -> Result<fs::File> {
        if let Some((ms, sub)) = self.find_storage(req_path) {
            ms.driver.open(&sub).await
        } else {
            Err(anyhow!("file not found: {}", req_path))
        }
    }

    /// Make directory
    pub async fn mkdir(&self, req_path: &str) -> Result<()> {
        if let Some((ms, sub)) = self.find_storage(req_path) {
            ms.driver.mkdir(&sub).await
        } else {
            Err(anyhow!("target storage not found: {}", req_path))
        }
    }

    /// Remove file or directory
    pub async fn remove(&self, req_path: &str) -> Result<()> {
        if let Some((ms, sub)) = self.find_storage(req_path) {
            ms.driver.remove(&sub).await
        } else {
            Err(anyhow!("target storage not found: {}", req_path))
        }
    }

    /// Rename file or directory safely with conflict/overwrite handling
    pub async fn rename_safe(
        &self,
        req_path: &str,
        new_name: &str,
        overwrite: bool,
    ) -> Result<(), RenameError> {
        let (storage, subpath) = self.find_storage(req_path).ok_or_else(|| {
            RenameError::NotFound(format!("target storage not found: {}", req_path))
        })?;

        storage
            .driver
            .rename_safe(&subpath, new_name, overwrite)
            .await
    }

    /// Two-phase batch rename within a directory
    pub async fn batch_rename(
        &self,
        src_dir: &str,
        pairs: &[(String, String)],
    ) -> Result<(), RenameError> {
        let (storage, subpath) = self.find_storage(src_dir).ok_or_else(|| {
            RenameError::NotFound(format!("target storage not found: {}", src_dir))
        })?;

        storage.driver.batch_rename(&subpath, pairs).await
    }

    /// Safely move a file or directory with overwrite conflict handling and rollback
    pub async fn move_to_safe(
        &self,
        src_path: &str,
        dst_path: &str,
        overwrite: bool,
    ) -> Result<()> {
        let src_match = self
            .find_storage(src_path)
            .ok_or_else(|| anyhow!("src storage not found"))?;
        let dst_match = self
            .find_storage(dst_path)
            .ok_or_else(|| anyhow!("dst storage not found"))?;

        if src_match.0.storage.id == dst_match.0.storage.id {
            src_match
                .0
                .driver
                .move_to_safe(&src_match.1, &dst_match.1, overwrite)
                .await
        } else {
            let src_full = src_match.0.driver.safe_resolve(&src_match.1)?;
            let dst_full = dst_match.0.driver.safe_resolve(&dst_match.1)?;
            crate::driver::local::move_path_safe(
                &src_full,
                &dst_full,
                overwrite,
                crate::driver::local::MoveFailurePoint::None,
            )
            .await
        }
    }

    /// Safely copy a file or directory with staging, overwrite backup, and rollback
    pub async fn copy_to_safe(
        &self,
        src_path: &str,
        dst_path: &str,
        overwrite: bool,
    ) -> Result<()> {
        let src_match = self
            .find_storage(src_path)
            .ok_or_else(|| anyhow!("src storage not found"))?;
        let dst_match = self
            .find_storage(dst_path)
            .ok_or_else(|| anyhow!("dst storage not found"))?;

        if src_match.0.storage.id == dst_match.0.storage.id {
            src_match
                .0
                .driver
                .copy_to_safe(&src_match.1, &dst_match.1, overwrite)
                .await
        } else {
            let src_full = src_match.0.driver.safe_resolve(&src_match.1)?;
            let dst_full = dst_match.0.driver.safe_resolve(&dst_match.1)?;
            crate::driver::local::copy_path_safe(&src_full, &dst_full, overwrite, false).await
        }
    }
}

pub type SharedStorageManager = Arc<StorageManager>;
