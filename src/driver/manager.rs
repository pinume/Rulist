#![allow(dead_code)]

use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::fs;

use crate::db::DbPool;
use crate::driver::local::LocalDriver;
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
                }
            }
        }

        // If no storages exist in database, create a default local mount at /Local pointing to current directory
        if storages.is_empty() {
            let current_dir = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string());

            let addition = format!(
                r#"{{"root_folder_path":"{}","show_hidden":false,"mkdir_perm":"0755"}}"#,
                current_dir
            );

            let default_storage = Storage {
                id: 1,
                mount_path: "/Local".to_string(),
                order: 0,
                driver: "Local".to_string(),
                cache_expiration: 0,
                status: Some("work".to_string()),
                addition: Some(addition.clone()),
                remark: Some("Default local storage".to_string()),
                disabled: false,
                enable_sign: false,
                order_by: None,
                order_direction: None,
            };

            // Save to DB
            let _ = sqlx::query(
                r#"
                INSERT OR IGNORE INTO `x_storages`
                (`mount_path`, `order`, `driver`, `cache_expiration`, `status`, `addition`, `remark`, `disabled`, `enable_sign`)
                VALUES ('/Local', 0, 'Local', 0, 'work', ?, 'Default local storage', 0, 0)
                "#,
            )
            .bind(&addition)
            .execute(pool)
            .await;

            if let Ok(driver) = LocalDriver::new(&addition) {
                storages.push(MountedStorage {
                    storage: default_storage,
                    driver,
                });
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
            {
                if mount.len() > max_prefix_len {
                    max_prefix_len = mount.len();
                    let sub = &clean_path[mount.len()..];
                    matched = Some((ms.clone(), sub.trim_start_matches('/').to_string()));
                }
            }
        }

        matched
    }

    /// List directory contents (virtual root or driver delegator)
    pub async fn list(&self, req_path: &str) -> Result<Vec<FileObj>> {
        let clean_path = req_path.trim_matches('/');

        // Root virtual directory listing when no storage is mounted directly at '/'
        if clean_path.is_empty() {
            if let Some((ms, sub)) = self.find_storage("/") {
                if ms.storage.mount_path == "/" {
                    return ms.driver.list(&sub).await;
                }
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

    /// Rename file or directory
    pub async fn rename(&self, req_path: &str, new_name: &str) -> Result<()> {
        if let Some((ms, sub)) = self.find_storage(req_path) {
            ms.driver.rename(&sub, new_name).await
        } else {
            Err(anyhow!("target storage not found: {}", req_path))
        }
    }

    /// Move file or directory
    pub async fn move_to(&self, src_path: &str, dst_path: &str) -> Result<()> {
        let src_match = self
            .find_storage(src_path)
            .ok_or_else(|| anyhow!("src storage not found"))?;
        let dst_match = self
            .find_storage(dst_path)
            .ok_or_else(|| anyhow!("dst storage not found"))?;

        if src_match.0.storage.id == dst_match.0.storage.id {
            src_match.0.driver.move_to(&src_match.1, &dst_match.1).await
        } else {
            // Cross-storage move: copy then remove
            self.copy_to(src_path, dst_path).await?;
            self.remove(src_path).await?;
            Ok(())
        }
    }

    /// Copy file or directory
    pub async fn copy_to(&self, src_path: &str, dst_path: &str) -> Result<()> {
        let src_match = self
            .find_storage(src_path)
            .ok_or_else(|| anyhow!("src storage not found"))?;
        let dst_match = self
            .find_storage(dst_path)
            .ok_or_else(|| anyhow!("dst storage not found"))?;

        if src_match.0.storage.id == dst_match.0.storage.id {
            src_match.0.driver.copy_to(&src_match.1, &dst_match.1).await
        } else {
            let src_full = src_match.0.driver.safe_resolve(&src_match.1)?;
            let dst_full = dst_match.0.driver.safe_resolve(&dst_match.1)?;
            if let Some(parent) = dst_full.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::copy(&src_full, &dst_full).await?;
            Ok(())
        }
    }
}

pub type SharedStorageManager = Arc<StorageManager>;
