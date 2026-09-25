use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewType {
    Image,
    Video,
    Audio,
    Pdf,
    Markdown,
    Text,
    Code,
    Json,
    Xml,
    Csv,
    Archive,
    Html,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStrategy {
    Direct,
    Processed,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMeta {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: String,
    pub mime_type: String,
    pub preview_type: PreviewType,
    pub strategy: PreviewStrategy,
    pub raw_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessedContent {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewResponse {
    pub meta: PreviewMeta,
    pub content: Option<ProcessedContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewReq {
    pub path: String,
}
