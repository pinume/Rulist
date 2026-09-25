use super::types::PreviewType;
use std::path::Path;

pub fn detect_from_path(path: &str) -> (PreviewType, &'static str) {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        // Images
        "jpg" | "jpeg" => (PreviewType::Image, "image/jpeg"),
        "png" => (PreviewType::Image, "image/png"),
        "gif" => (PreviewType::Image, "image/gif"),
        "webp" => (PreviewType::Image, "image/webp"),
        "avif" => (PreviewType::Image, "image/avif"),
        "svg" => (PreviewType::Image, "image/svg+xml"),
        "bmp" => (PreviewType::Image, "image/bmp"),
        "ico" => (PreviewType::Image, "image/x-icon"),

        // Video
        "mp4" => (PreviewType::Video, "video/mp4"),
        "webm" => (PreviewType::Video, "video/webm"),
        "mov" => (PreviewType::Video, "video/quicktime"),
        "mkv" => (PreviewType::Video, "video/x-matroska"),

        // Audio
        "mp3" => (PreviewType::Audio, "audio/mpeg"),
        "flac" => (PreviewType::Audio, "audio/flac"),
        "ogg" => (PreviewType::Audio, "audio/ogg"),
        "opus" => (PreviewType::Audio, "audio/opus"),
        "wav" => (PreviewType::Audio, "audio/wav"),
        "m4a" => (PreviewType::Audio, "audio/mp4"),
        "aac" => (PreviewType::Audio, "audio/aac"),

        // Direct Documents
        "pdf" => (PreviewType::Pdf, "application/pdf"),
        "html" | "htm" => (PreviewType::Html, "text/html"),

        // Text / Markdown / Code / Structured
        "md" | "markdown" => (PreviewType::Markdown, "text/markdown"),
        "txt" | "log" | "conf" | "ini" | "properties" | "env" => (PreviewType::Text, "text/plain"),
        "rs" | "go" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" | "java"
        | "css" | "scss" | "sh" | "bash" | "sql" | "yaml" | "yml" | "toml" => {
            (PreviewType::Code, "text/plain")
        }
        "json" | "jsonc" => (PreviewType::Json, "application/json"),
        "xml" => (PreviewType::Xml, "application/xml"),
        "csv" => (PreviewType::Csv, "text/csv"),
        "tsv" => (PreviewType::Csv, "text/tab-separated-values"),

        // Archives
        "zip" => (PreviewType::Archive, "application/zip"),
        "tar" => (PreviewType::Archive, "application/x-tar"),
        "gz" | "tgz" => (PreviewType::Archive, "application/gzip"),

        _ => (PreviewType::Unknown, "application/octet-stream"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_extensions() {
        assert_eq!(detect_from_path("foo.jpg").0, PreviewType::Image);
        assert_eq!(detect_from_path("foo.MP4").0, PreviewType::Video);
        assert_eq!(detect_from_path("test.pdf").0, PreviewType::Pdf);
        assert_eq!(detect_from_path("index.html").0, PreviewType::Html);
        assert_eq!(detect_from_path("no_ext").0, PreviewType::Unknown);
    }
}
