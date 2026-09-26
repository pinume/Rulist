#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

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
pub const PERM_ALLOW_EMPTY_PASSWORD: i32 = 9;

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
    pub base_path: String,
    pub role: i32,
    pub disabled: bool,
    pub permission: i32,
    #[serde(default)]
    pub password_unset: bool,
    #[serde(skip_serializing)]
    pub otp_secret: Option<String>,
    #[serde(skip_serializing)]
    pub last_otp_step: i64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsListReq {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default)]
    pub per_page: Option<usize>,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    pub reverse: Option<bool>,
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
    pub local_path: String,
    #[serde(default)]
    pub directory_path: String,
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
    pub directory_path: Option<String>,
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

#[cfg(unix)]
pub fn format_mode(mode: u32, is_dir: bool) -> String {
    let d = if is_dir { 'd' } else { '-' };
    let r1 = if mode & 0o400 != 0 { 'r' } else { '-' };
    let w1 = if mode & 0o200 != 0 { 'w' } else { '-' };
    let x1 = if mode & 0o100 != 0 { 'x' } else { '-' };
    let r2 = if mode & 0o040 != 0 { 'r' } else { '-' };
    let w2 = if mode & 0o020 != 0 { 'w' } else { '-' };
    let x2 = if mode & 0o010 != 0 { 'x' } else { '-' };
    let r3 = if mode & 0o004 != 0 { 'r' } else { '-' };
    let w3 = if mode & 0o002 != 0 { 'w' } else { '-' };
    let x3 = if mode & 0o001 != 0 { 'x' } else { '-' };
    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        d, r1, w1, x1, r2, w2, x2, r3, w3, x3
    )
}

#[cfg(not(unix))]
pub fn format_mode(_mode: u32, is_dir: bool) -> String {
    if is_dir {
        "drwxr-xr-x".to_string()
    } else {
        "-rw-r--r--".to_string()
    }
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
            permissions: None,
        }
    }
}

pub fn get_file_type(filename: &str) -> i32 {
    match crate::preview::detect_from_path(filename).0 {
        crate::preview::PreviewType::Video => TYPE_VIDEO,
        crate::preview::PreviewType::Audio => TYPE_AUDIO,
        crate::preview::PreviewType::Image => TYPE_IMAGE,
        crate::preview::PreviewType::Text
        | crate::preview::PreviewType::Html
        | crate::preview::PreviewType::Markdown
        | crate::preview::PreviewType::Code
        | crate::preview::PreviewType::Json
        | crate::preview::PreviewType::Xml => TYPE_TEXT,
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

fn compare_files(a: &FileObj, b: &FileObj, order_by: Option<&str>, reverse: bool) -> Ordering {
    // Directories always come first
    if a.is_dir != b.is_dir {
        return if a.is_dir {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }

    let order = match order_by.unwrap_or("name") {
        "size" => a
            .size
            .cmp(&b.size)
            .then_with(|| natural_cmp(&a.name, &b.name))
            .then_with(|| a.name.cmp(&b.name)),
        "modified" => natural_cmp(&a.modified, &b.modified)
            .then_with(|| natural_cmp(&a.name, &b.name))
            .then_with(|| a.name.cmp(&b.name)),
        _ => natural_cmp(&a.name, &b.name).then_with(|| a.name.cmp(&b.name)),
    };

    if reverse { order.reverse() } else { order }
}

pub fn sort_files_by(files: &mut [FileObj], order_by: Option<&str>, reverse: bool) {
    files.sort_by(|a, b| compare_files(a, b, order_by, reverse));
}

pub fn sorted_file_page(
    files: &mut [FileObj],
    order_by: Option<&str>,
    reverse: bool,
    page: usize,
    per_page: usize,
) -> Vec<FileObj> {
    let start = page.saturating_sub(1).saturating_mul(per_page);
    if start >= files.len() {
        return Vec::new();
    }
    let end = (start + per_page).min(files.len());

    if end < files.len() {
        files.select_nth_unstable_by(end, |a, b| compare_files(a, b, order_by, reverse));
    }
    files[..end].sort_unstable_by(|a, b| compare_files(a, b, order_by, reverse));
    files[start..end].to_vec()
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
    fn paginated_sort_matches_full_sort() {
        let files: Vec<FileObj> = (0..300)
            .map(|i| FileObj::new(format!("file{i}.txt"), i, false, ""))
            .collect();
        let mut sorted = files.clone();
        sort_files_by(&mut sorted, Some("name"), true);

        let mut paged = files;
        let page = sorted_file_page(&mut paged, Some("name"), true, 3, 50);
        let names: Vec<&str> = page.iter().map(|file| file.name.as_str()).collect();
        let expected: Vec<&str> = sorted[100..150]
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn test_get_file_type() {
        assert_eq!(get_file_type("song.mp3"), TYPE_AUDIO);
        assert_eq!(get_file_type("movie.mp4"), TYPE_VIDEO);
        assert_eq!(get_file_type("photo.png"), TYPE_IMAGE);
        assert_eq!(get_file_type("doc.md"), TYPE_TEXT);
        assert_eq!(get_file_type("script.py"), TYPE_TEXT);
        assert_eq!(get_file_type("data.json"), TYPE_TEXT);
        assert_eq!(get_file_type("archive.bin"), TYPE_UNKNOWN);
        assert_eq!(get_file_type("word.docx"), TYPE_UNKNOWN);
        assert_eq!(get_file_type("excel.xlsx"), TYPE_UNKNOWN);
        assert_eq!(get_file_type("slide.pptx"), TYPE_UNKNOWN);
    }
}
