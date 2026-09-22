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
    #[serde(default = "default_mkdir_perm")]
    pub mkdir_perm: String,
}

fn default_mkdir_perm() -> String {
    "0755".to_string()
}

#[derive(Debug, Clone)]
pub struct LocalDriver {
    pub root_path: PathBuf,
    pub show_hidden: bool,
    pub mkdir_perm: u32,
}

impl LocalDriver {
    pub fn new(addition_json: &str) -> Result<Self> {
        let addition: LocalAddition = if addition_json.is_empty() {
            LocalAddition::default()
        } else {
            serde_json::from_str(addition_json).unwrap_or_default()
        };

        let root_str = if addition.root_folder_path.is_empty() {
            "."
        } else {
            &addition.root_folder_path
        };

        let root_path = fs_canonical_or_abs(root_str)?;
        let mkdir_perm =
            u32::from_str_radix(&addition.mkdir_perm.trim_start_matches("0o"), 8).unwrap_or(0o755);

        Ok(Self {
            root_path,
            show_hidden: addition.show_hidden,
            mkdir_perm,
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

    /// List directory contents
    pub async fn list(&self, subpath: &str) -> Result<Vec<FileObj>> {
        let full_path = self.safe_resolve(subpath)?;
        let mut read_dir = fs::read_dir(&full_path)
            .await
            .with_context(|| format!("failed to read directory: {:?}", full_path))?;

        let mut items = Vec::new();

        while let Some(entry) = read_dir.next_entry().await? {
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files if show_hidden is false
            if !self.show_hidden && file_name.starts_with('.') {
                continue;
            }

            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue, // Skip unreadable entries
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
        let meta = fs::metadata(&full_path)
            .await
            .with_context(|| format!("target does not exist: {:?}", full_path))?;

        if meta.is_dir() {
            fs::remove_dir_all(&full_path)
                .await
                .with_context(|| format!("failed to remove directory: {:?}", full_path))?;
        } else {
            fs::remove_file(&full_path)
                .await
                .with_context(|| format!("failed to remove file: {:?}", full_path))?;
        }

        Ok(())
    }

    /// Rename an item within the same directory
    pub async fn rename(&self, subpath: &str, new_name: &str) -> Result<()> {
        if subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot rename storage root"));
        }
        let src_path = self.safe_resolve(subpath)?;
        if new_name.contains('/') || new_name.contains('\\') || new_name == ".." || new_name == "."
        {
            return Err(anyhow!("invalid new name: {}", new_name));
        }

        let parent = src_path
            .parent()
            .ok_or_else(|| anyhow!("cannot rename root"))?;
        let dst_path = parent.join(new_name);

        fs::rename(&src_path, &dst_path)
            .await
            .with_context(|| format!("failed to rename {:?} to {:?}", src_path, dst_path))?;
        Ok(())
    }

    /// Move file or directory
    pub async fn move_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot move storage root"));
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;

        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Try direct rename first (fast atomic path)
        if fs::rename(&src_path, &dst_path).await.is_ok() {
            return Ok(());
        }

        // Cross-device fallback: copy then remove
        fs::copy(&src_path, &dst_path).await?;
        let meta = fs::metadata(&src_path).await?;
        if meta.is_dir() {
            fs::remove_dir_all(&src_path).await?;
        } else {
            fs::remove_file(&src_path).await?;
        }

        Ok(())
    }

    /// Copy file
    pub async fn copy_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot copy storage root"));
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;

        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::copy(&src_path, &dst_path)
            .await
            .with_context(|| format!("failed to copy {:?} to {:?}", src_path, dst_path))?;
        Ok(())
    }
}

fn fs_canonical_or_abs(p: &str) -> Result<PathBuf> {
    let path = Path::new(p);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(abs)
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
            .rename("folder1/test.txt", "renamed.txt")
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
}
