use super::types::{PreviewStrategy, PreviewType};

pub fn resolve_strategy(preview_type: &PreviewType) -> PreviewStrategy {
    match preview_type {
        PreviewType::Image
        | PreviewType::Video
        | PreviewType::Audio
        | PreviewType::Pdf
        | PreviewType::Html => PreviewStrategy::Direct,

        PreviewType::Markdown
        | PreviewType::Text
        | PreviewType::Code
        | PreviewType::Json
        | PreviewType::Xml
        | PreviewType::Csv
        | PreviewType::Archive => PreviewStrategy::Processed,

        PreviewType::Unknown => PreviewStrategy::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_strategy() {
        assert_eq!(
            resolve_strategy(&PreviewType::Image),
            PreviewStrategy::Direct
        );
        assert_eq!(
            resolve_strategy(&PreviewType::Video),
            PreviewStrategy::Direct
        );
        assert_eq!(resolve_strategy(&PreviewType::Pdf), PreviewStrategy::Direct);
        assert_eq!(
            resolve_strategy(&PreviewType::Markdown),
            PreviewStrategy::Processed
        );
        assert_eq!(
            resolve_strategy(&PreviewType::Unknown),
            PreviewStrategy::Unsupported
        );
    }
}
