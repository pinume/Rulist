pub mod local;
mod ops;

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

pub const TYPE_UNKNOWN: i32 = 0;
pub const TYPE_FOLDER: i32 = 1;
pub const TYPE_VIDEO: i32 = 2;
pub const TYPE_AUDIO: i32 = 3;
pub const TYPE_TEXT: i32 = 4;
pub const TYPE_IMAGE: i32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
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

impl FileEntry {
    pub fn new(
        name: impl Into<String>,
        size: i64,
        is_dir: bool,
        modified: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let file_type = if is_dir {
            TYPE_FOLDER
        } else {
            get_file_type(&name)
        };

        Self {
            name,
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
    format!("{d}{r1}{w1}{x1}{r2}{w2}{x2}{r3}{w3}{x3}")
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
                    while let Some(ch) = a_chars.peek() {
                        if let Some(digit) = ch.to_digit(10) {
                            a_num = a_num.saturating_mul(10).saturating_add(digit as u64);
                            a_chars.next();
                        } else {
                            break;
                        }
                    }

                    let mut b_num: u64 = 0;
                    while let Some(ch) = b_chars.peek() {
                        if let Some(digit) = ch.to_digit(10) {
                            b_num = b_num.saturating_mul(10).saturating_add(digit as u64);
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

fn compare_files(a: &FileEntry, b: &FileEntry, order_by: Option<&str>, reverse: bool) -> Ordering {
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

pub fn sort_files_by(files: &mut [FileEntry], order_by: Option<&str>, reverse: bool) {
    files.sort_by(|a, b| compare_files(a, b, order_by, reverse));
}

pub fn sorted_file_page(
    files: &mut [FileEntry],
    order_by: Option<&str>,
    reverse: bool,
    page: usize,
    per_page: usize,
) -> Vec<FileEntry> {
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
