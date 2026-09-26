use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewType {
    Image,
    Video,
    Audio,
    Pdf,
    Html,
    Markdown,
    Text,
    Code,
    Json,
    Xml,
    Unknown,
}

impl PreviewType {
    pub fn strategy(&self) -> PreviewStrategy {
        match self {
            PreviewType::Image
            | PreviewType::Video
            | PreviewType::Audio
            | PreviewType::Pdf
            | PreviewType::Html => PreviewStrategy::Direct,

            PreviewType::Markdown
            | PreviewType::Text
            | PreviewType::Code
            | PreviewType::Json
            | PreviewType::Xml => PreviewStrategy::Processed,

            PreviewType::Unknown => PreviewStrategy::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStrategy {
    Direct,
    Processed,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedContent {
    pub kind: String,
    pub value: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewResponse {
    pub meta: PreviewMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ProcessedContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewReq {
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy() {
        assert_eq!(PreviewType::Image.strategy(), PreviewStrategy::Direct);
        assert_eq!(PreviewType::Video.strategy(), PreviewStrategy::Direct);
        assert_eq!(PreviewType::Pdf.strategy(), PreviewStrategy::Direct);
        assert_eq!(PreviewType::Html.strategy(), PreviewStrategy::Direct);
        assert_eq!(PreviewType::Markdown.strategy(), PreviewStrategy::Processed);
        assert_eq!(PreviewType::Text.strategy(), PreviewStrategy::Processed);
        assert_eq!(PreviewType::Code.strategy(), PreviewStrategy::Processed);
        assert_eq!(PreviewType::Json.strategy(), PreviewStrategy::Processed);
        assert_eq!(PreviewType::Xml.strategy(), PreviewStrategy::Processed);
        assert_eq!(
            PreviewType::Unknown.strategy(),
            PreviewStrategy::Unsupported
        );
    }
}
