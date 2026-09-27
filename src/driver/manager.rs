use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::fs;

use crate::db::DbPool;
use crate::driver::local::{LocalDriver, RenameError};
use crate::model::{FileObj, Storage};

use super::local_ops::{copy_path_safe, move_path_safe};

#[derive(Clone)]
pub struct MountedStorage {
    pub storage: Storage,
    pub driver: LocalDriver,
}

#[derive(Clone, Default)]
pub struct StorageManager {
    pool: Option<DbPool>,
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
        for storage in rows {
            let addition = storage.addition.clone().unwrap_or_default();
            match LocalDriver::new(&addition) {
                Ok(driver) => storages.push(MountedStorage { storage, driver }),
                Err(error) => {
                    tracing::warn!(
                        "failed to mount storage {}: {:?}",
                        storage.mount_path,
                        error
                    );
                    let _ =
                        sqlx::query("UPDATE `x_storages` SET `status` = 'invalid' WHERE `id` = ?")
                            .bind(storage.id)
                            .execute(pool)
                            .await;
                }
            }
        }

        Ok(Self {
            pool: Some(pool.clone()),
            storages: Arc::new(std::sync::RwLock::new(storages)),
        })
    }

    pub async fn reload_from_db(&self, pool: &DbPool) -> Result<()> {
        let new_manager = Self::load_from_db(pool).await?;
        let new_storages = new_manager.storages.read().unwrap().clone();
        *self.storages.write().unwrap() = new_storages;
        Ok(())
    }

    pub async fn ensure_mounted(&self, req_path: &str) {
        let clean = if req_path.is_empty() || !req_path.starts_with('/') {
            format!("/{req_path}")
        } else {
            req_path.to_string()
        };

        if clean.starts_with("/.users/") {
            let has_mount = {
                let storages = self.storages.read().unwrap();
                storages.iter().any(|storage| {
                    storage.storage.mount_path == clean
                        || (clean.starts_with(&storage.storage.mount_path)
                            && clean.as_bytes().get(storage.storage.mount_path.len())
                                == Some(&b'/')
                            && storage.storage.mount_path != "/")
                })
            };
            if !has_mount && let Some(ref pool) = self.pool {
                let _ = self.reload_from_db(pool).await;
            }
        }
    }

    pub fn find_storage(&self, req_path: &str) -> Option<(MountedStorage, String)> {
        let clean_path = if req_path.is_empty() || !req_path.starts_with('/') {
            format!("/{req_path}")
        } else {
            req_path.to_string()
        };

        let storages = self.storages.read().unwrap();
        let mut matched: Option<(MountedStorage, String)> = None;
        let mut max_prefix_len = 0;

        for mounted in storages.iter() {
            let mount = &mounted.storage.mount_path;
            if mount == "/" {
                if max_prefix_len == 0 {
                    matched = Some((
                        mounted.clone(),
                        clean_path.trim_start_matches('/').to_string(),
                    ));
                }
            } else if clean_path == *mount {
                return Some((mounted.clone(), String::new()));
            } else if clean_path.starts_with(mount)
                && clean_path.as_bytes().get(mount.len()) == Some(&b'/')
                && mount.len() > max_prefix_len
            {
                max_prefix_len = mount.len();
                let subpath = &clean_path[mount.len()..];
                matched = Some((mounted.clone(), subpath.trim_start_matches('/').to_string()));
            }
        }

        matched
    }

    pub fn storage_context_for_path(&self, req_path: &str) -> String {
        self.find_storage(req_path)
            .map(|(mounted, _)| {
                format!(
                    "id={}:add={}",
                    mounted.storage.id,
                    mounted.storage.addition.as_deref().unwrap_or("")
                )
            })
            .unwrap_or_default()
    }

    pub async fn list(&self, req_path: &str) -> Result<Vec<FileObj>> {
        self.ensure_mounted(req_path).await;
        let clean_path = req_path.trim_matches('/');

        if clean_path.is_empty() {
            if let Some((mounted, subpath)) = self.find_storage("/")
                && mounted.storage.mount_path == "/"
            {
                return mounted.driver.list(&subpath).await;
            }

            let storages = self.storages.read().unwrap();
            return Ok(storages
                .iter()
                .filter_map(|mounted| {
                    let name = mounted.storage.mount_path.trim_matches('/');
                    (!name.is_empty()).then(|| FileObj::new(name, 0, true, ""))
                })
                .collect());
        }

        if let Some((mounted, subpath)) = self.find_storage(req_path) {
            mounted.driver.list(&subpath).await
        } else {
            Err(anyhow!("mount storage not found for path: {req_path}"))
        }
    }

    pub async fn is_physically_empty(&self, req_path: &str) -> Result<bool> {
        self.ensure_mounted(req_path).await;
        let (storage, subpath) = self
            .find_storage(req_path)
            .ok_or_else(|| anyhow!("storage not found"))?;
        storage.driver.is_physically_empty(&subpath).await
    }

    pub async fn read_dir_physical(&self, req_path: &str) -> Result<Vec<(String, bool)>> {
        self.ensure_mounted(req_path).await;
        let (storage, subpath) = self
            .find_storage(req_path)
            .ok_or_else(|| anyhow!("storage not found"))?;
        storage.driver.read_dir_physical(&subpath).await
    }

    pub async fn get(&self, req_path: &str) -> Result<FileObj> {
        self.ensure_mounted(req_path).await;
        let clean = req_path.trim_matches('/');
        if clean.is_empty() {
            return Ok(FileObj::new("/", 0, true, ""));
        }

        let is_mount = self
            .storages
            .read()
            .unwrap()
            .iter()
            .any(|mounted| mounted.storage.mount_path.trim_matches('/') == clean);
        if is_mount {
            return Ok(FileObj::new(clean, 0, true, ""));
        }

        if let Some((mounted, subpath)) = self.find_storage(req_path) {
            mounted.driver.get(&subpath).await
        } else {
            Err(anyhow!("path not found: {req_path}"))
        }
    }

    pub async fn open(&self, req_path: &str) -> Result<fs::File> {
        self.ensure_mounted(req_path).await;
        if let Some((mounted, subpath)) = self.find_storage(req_path) {
            mounted.driver.open(&subpath).await
        } else {
            Err(anyhow!("file not found: {req_path}"))
        }
    }

    pub async fn mkdir(&self, req_path: &str) -> Result<()> {
        self.ensure_mounted(req_path).await;
        if let Some((mounted, subpath)) = self.find_storage(req_path) {
            mounted.driver.mkdir(&subpath).await
        } else {
            Err(anyhow!("target storage not found: {req_path}"))
        }
    }

    pub async fn remove(&self, req_path: &str) -> Result<()> {
        self.ensure_mounted(req_path).await;
        if let Some((mounted, subpath)) = self.find_storage(req_path) {
            mounted.driver.remove(&subpath).await
        } else {
            Err(anyhow!("target storage not found: {req_path}"))
        }
    }

    pub async fn rename_safe(
        &self,
        req_path: &str,
        new_name: &str,
        overwrite: bool,
    ) -> Result<(), RenameError> {
        self.ensure_mounted(req_path).await;
        let (storage, subpath) = self.find_storage(req_path).ok_or_else(|| {
            RenameError::NotFound(format!("target storage not found: {req_path}"))
        })?;
        storage
            .driver
            .rename_safe(&subpath, new_name, overwrite)
            .await
    }

    pub async fn batch_rename(
        &self,
        src_dir: &str,
        pairs: &[(String, String)],
    ) -> Result<(), RenameError> {
        self.ensure_mounted(src_dir).await;
        let (storage, subpath) = self
            .find_storage(src_dir)
            .ok_or_else(|| RenameError::NotFound(format!("target storage not found: {src_dir}")))?;
        storage.driver.batch_rename(&subpath, pairs).await
    }

    pub async fn move_to_safe(
        &self,
        src_path: &str,
        dst_path: &str,
        overwrite: bool,
    ) -> Result<()> {
        self.ensure_mounted(src_path).await;
        self.ensure_mounted(dst_path).await;
        let src_match = self
            .find_storage(src_path)
            .ok_or_else(|| anyhow!("src storage not found"))?;
        let dst_match = self
            .find_storage(dst_path)
            .ok_or_else(|| anyhow!("dst storage not found"))?;

        if src_match.1.trim_matches('/').is_empty() || dst_match.1.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot move storage root"));
        }

        if src_match.0.storage.id == dst_match.0.storage.id {
            src_match
                .0
                .driver
                .move_to_safe(&src_match.1, &dst_match.1, overwrite)
                .await
        } else {
            let src_full = src_match.0.driver.safe_resolve(&src_match.1)?;
            let dst_full = dst_match.0.driver.safe_resolve(&dst_match.1)?;
            move_path_safe(&src_full, &dst_full, overwrite).await
        }
    }

    pub async fn copy_to_safe(
        &self,
        src_path: &str,
        dst_path: &str,
        overwrite: bool,
    ) -> Result<()> {
        self.ensure_mounted(src_path).await;
        self.ensure_mounted(dst_path).await;
        let src_match = self
            .find_storage(src_path)
            .ok_or_else(|| anyhow!("src storage not found"))?;
        let dst_match = self
            .find_storage(dst_path)
            .ok_or_else(|| anyhow!("dst storage not found"))?;

        if src_match.1.trim_matches('/').is_empty() || dst_match.1.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot copy storage root"));
        }

        if src_match.0.storage.id == dst_match.0.storage.id {
            src_match
                .0
                .driver
                .copy_to_safe(&src_match.1, &dst_match.1, overwrite)
                .await
        } else {
            let src_full = src_match.0.driver.safe_resolve(&src_match.1)?;
            let dst_full = dst_match.0.driver.safe_resolve(&dst_match.1)?;
            copy_path_safe(&src_full, &dst_full, overwrite).await
        }
    }
}
