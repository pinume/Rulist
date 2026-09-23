#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            code: 200,
            message: "success".to_string(),
            data: Some(data),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

pub const ROLE_ADMIN: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub pwd_hash: String,
    #[serde(skip_serializing)]
    pub pwd_ts: i64,
    #[serde(skip_serializing)]
    pub salt: String,
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub base_path: String,
    pub role: i32,
    pub disabled: bool,
    pub permission: i32,
    #[serde(skip_serializing)]
    pub otp_secret: Option<String>,
    pub sso_id: Option<String>,
    #[sqlx(default)]
    #[serde(default)]
    pub otp: bool,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }

    pub fn update_otp(&mut self) {
        self.otp = self
            .otp_secret
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Storage {
    pub id: i64,
    pub mount_path: String,
    pub order: i32,
    pub driver: String,
    pub cache_expiration: i32,
    pub status: Option<String>,
    pub addition: Option<String>,
    pub remark: Option<String>,
    pub disabled: bool,
    pub enable_sign: bool,
    pub order_by: Option<String>,
    pub order_direction: Option<String>,
}

pub const TYPE_UNKNOWN: i32 = 0;
pub const TYPE_FOLDER: i32 = 1;
pub const TYPE_VIDEO: i32 = 2;
pub const TYPE_AUDIO: i32 = 3;
pub const TYPE_TEXT: i32 = 4;
pub const TYPE_IMAGE: i32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileObj {
    pub name: String,
    pub size: i64,
    pub is_dir: bool,
    pub modified: String,
    pub sign: String,
    pub thumb: String,
    pub r#type: i32,
    pub raw_url: String,
    pub readme: String,
    pub header: String,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsListReq {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default)]
    pub per_page: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsListResp {
    pub content: Vec<FileObj>,
    pub total: i64,
    pub readme: String,
    pub header: String,
    pub write: bool,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsGetReq {
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub otp_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FsRenameReq {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsDirNamesReq {
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    #[default]
    Cancel,
    Overwrite,
    Skip,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsMoveCopyReq {
    #[serde(default)]
    pub src_dir: String,
    #[serde(default)]
    pub dst_dir: String,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FsRecursiveMoveReq {
    pub src_dir: String,
    pub dst_dir: String,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FsRemoveEmptyDirsReq {
    pub src_dir: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsDirsReq {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub force_root: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirItem {
    pub name: String,
    pub modified: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCurrentReq {
    pub username: Option<String>,
    pub password: Option<String>,
    pub current_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserWithMount {
    pub id: i64,
    pub username: String,
    pub base_path: String,
    pub role: i32,
    pub disabled: bool,
    pub permission: i32,
    #[serde(default)]
    pub sso_id: Option<String>,
    #[serde(default)]
    pub local_path: String,
    #[serde(default)]
    pub otp: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TwoFaGenerateReq {
    #[serde(default)]
    pub current_password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwoFaVerifyReq {
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminUserSaveReq {
    pub id: Option<i64>,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub role: Option<i32>,
    #[serde(default)]
    pub permission: Option<i32>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchRenameItem {
    pub src_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchRenameReq {
    pub src_dir: String,
    pub rename_objects: Vec<BatchRenameItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FsLinkReq {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FsLinkResp {
    pub url: String,
}

impl FileObj {
    pub fn new(
        name: impl Into<String>,
        size: i64,
        is_dir: bool,
        modified: impl Into<String>,
    ) -> Self {
        let name_str = name.into();
        let file_type = if is_dir {
            TYPE_FOLDER
        } else {
            get_file_type(&name_str)
        };

        Self {
            name: name_str,
            size,
            is_dir,
            modified: modified.into(),
            sign: String::new(),
            thumb: String::new(),
            r#type: file_type,
            raw_url: String::new(),
            readme: String::new(),
            header: String::new(),
            provider: "Local".to_string(),
        }
    }
}

pub fn get_file_type(filename: &str) -> i32 {
    let ext = Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "rmvb" | "ts" => {
            TYPE_VIDEO
        }
        "mp3" | "flac" | "ogg" | "m4a" | "wav" | "opus" | "aac" | "aiff" | "wma" => TYPE_AUDIO,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" | "heic" => {
            TYPE_IMAGE
        }
        "txt" | "md" | "json" | "xml" | "yaml" | "yml" | "go" | "rs" | "py" | "js" | "html"
        | "css" | "c" | "cpp" | "h" | "sh" | "log" | "sql" | "toml" | "ini" | "conf" => TYPE_TEXT,
        _ => TYPE_UNKNOWN,
    }
}

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut a_num: u64 = 0;
                    while let Some(c) = a_chars.peek() {
                        if let Some(d) = c.to_digit(10) {
                            a_num = a_num.saturating_mul(10).saturating_add(d as u64);
                            a_chars.next();
                        } else {
                            break;
                        }
                    }

                    let mut b_num: u64 = 0;
                    while let Some(c) = b_chars.peek() {
                        if let Some(d) = c.to_digit(10) {
                            b_num = b_num.saturating_mul(10).saturating_add(d as u64);
                            b_chars.next();
                        } else {
                            break;
                        }
                    }

                    match a_num.cmp(&b_num) {
                        Ordering::Equal => continue,
                        other => return other,
                    }
                } else {
                    let ca_lower = ca.to_lowercase().next().unwrap_or(*ca);
                    let cb_lower = cb.to_lowercase().next().unwrap_or(*cb);
                    match ca_lower.cmp(&cb_lower) {
                        Ordering::Equal => {
                            a_chars.next();
                            b_chars.next();
                        }
                        other => return other,
                    }
                }
            }
        }
    }
}

pub fn sort_files(files: &mut [FileObj]) {
    files.sort_by(|a, b| {
        // Directories always come first
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        natural_cmp(&a.name, &b.name)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_natural_sort() {
        let mut list = vec!["file10.txt", "file2.txt", "file1.txt", "file20.txt"];
        list.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            list,
            vec!["file1.txt", "file2.txt", "file10.txt", "file20.txt"]
        );
    }

    #[test]
    fn test_get_file_type() {
        assert_eq!(get_file_type("song.mp3"), TYPE_AUDIO);
        assert_eq!(get_file_type("movie.mp4"), TYPE_VIDEO);
        assert_eq!(get_file_type("photo.png"), TYPE_IMAGE);
        assert_eq!(get_file_type("doc.md"), TYPE_TEXT);
        assert_eq!(get_file_type("archive.bin"), TYPE_UNKNOWN);
    }
}
