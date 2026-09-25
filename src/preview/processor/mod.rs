pub mod archive;
pub mod csv;
pub mod json;
pub mod markdown;
pub mod text;
pub mod xml;

use tokio::io::AsyncReadExt;

use crate::driver::StorageManager;
use crate::preview::limits::{
    MAX_ARCHIVE_FILE_SIZE, MAX_CSV_SIZE, MAX_TEXT_PREVIEW_SIZE, PROCESSED_PREVIEW_CONCURRENCY,
};
use crate::preview::types::{PreviewType, ProcessedContent};

static SEMAPHORE: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(PROCESSED_PREVIEW_CONCURRENCY);

pub async fn process_file(
    storage: &StorageManager,
    path: &str,
    preview_type: PreviewType,
    file_size: i64,
) -> Result<ProcessedContent, &'static str> {
    let max_size = match preview_type {
        PreviewType::Archive => MAX_ARCHIVE_FILE_SIZE,
        PreviewType::Csv => MAX_CSV_SIZE,
        _ => MAX_TEXT_PREVIEW_SIZE,
    };

    if file_size > max_size as i64 {
        return Err("too_large");
    }

    let _permit = SEMAPHORE.acquire().await.map_err(|_| "concurrency_limit")?;

    let file = storage.open(path).await.map_err(|_| "read_failed")?;
    let mut bytes = Vec::with_capacity(file_size.max(0).min(max_size as i64) as usize);
    file.take(max_size + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "read_failed")?;

    if bytes.len() as u64 > max_size {
        return Err("too_large");
    }

    if preview_type == PreviewType::Archive {
        return archive::process(&bytes, path);
    }

    let text = std::str::from_utf8(&bytes).map_err(|_| "unsupported_encoding")?;

    match preview_type {
        PreviewType::Markdown => markdown::process(text),
        PreviewType::Text | PreviewType::Code => text::process(text),
        PreviewType::Json => json::process(text),
        PreviewType::Xml => xml::process(text),
        PreviewType::Csv => csv::process(text, path),
        _ => text::process(text),
    }
}
