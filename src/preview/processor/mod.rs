use tokio::io::AsyncReadExt;

use crate::driver::StorageManager;
use crate::preview::types::{PreviewType, ProcessedContent};

pub const MAX_DOCUMENT_PREVIEW_SIZE: u64 = 4 * 1024 * 1024; // 4MB
static SEMAPHORE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);

pub async fn process_file(
    storage: &StorageManager,
    path: &str,
    preview_type: PreviewType,
    file_size: i64,
) -> Result<ProcessedContent, &'static str> {
    let max_size = MAX_DOCUMENT_PREVIEW_SIZE;

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

    let text = std::str::from_utf8(&bytes).map_err(|_| "unsupported_encoding")?;

    match preview_type {
        PreviewType::Markdown => {
            let mut html = String::new();
            let parser = pulldown_cmark::Parser::new_ext(text, pulldown_cmark::Options::all());
            pulldown_cmark::html::push_html(&mut html, parser);
            Ok(ProcessedContent {
                kind: "html".to_string(),
                value: ammonia::clean(&html),
            })
        }
        PreviewType::Json => {
            let value = match serde_json::from_str::<serde_json::Value>(text) {
                Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| text.to_string()),
                Err(_) => text.to_string(),
            };
            Ok(ProcessedContent {
                kind: "json".to_string(),
                value,
            })
        }
        PreviewType::Xml => Ok(ProcessedContent {
            kind: "xml".to_string(),
            value: text.to_string(),
        }),
        PreviewType::Code => Ok(ProcessedContent {
            kind: "code".to_string(),
            value: text.to_string(),
        }),
        _ => Ok(ProcessedContent {
            kind: "text".to_string(),
            value: text.to_string(),
        }),
    }
}
