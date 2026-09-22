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
        let mkdir_perm =
            u32::from_str_radix(addition.mkdir_perm.trim_start_matches("0o"), 8).unwrap_or(0o755);

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
        remove_path_recursive(&full_path).await
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

        if src_path == dst_path {
            return Ok(());
        }

        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Try direct rename first (fast atomic path)
        if fs::rename(&src_path, &dst_path).await.is_ok() {
            return Ok(());
        }

        // Cross-device fallback: copy then remove
        copy_path_recursive(&src_path, &dst_path).await?;
        remove_path_recursive(&src_path).await?;

        Ok(())
    }

    /// Copy file or directory
    pub async fn copy_to(&self, src_subpath: &str, dst_subpath: &str) -> Result<()> {
        if src_subpath.trim_matches('/').is_empty() || dst_subpath.trim_matches('/').is_empty() {
            return Err(anyhow!("cannot copy storage root"));
        }
        let src_path = self.safe_resolve(src_subpath)?;
        let dst_path = self.safe_resolve(dst_subpath)?;

        copy_path_recursive(&src_path, &dst_path).await
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
}
