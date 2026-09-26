use super::types::PreviewType;
use std::path::Path;

pub fn detect_from_path(path: &str) -> (PreviewType, &'static str) {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        // Images (Browser native)
        "jpg" | "jpeg" => (PreviewType::Image, "image/jpeg"),
        "png" => (PreviewType::Image, "image/png"),
        "gif" => (PreviewType::Image, "image/gif"),
        "webp" => (PreviewType::Image, "image/webp"),
        "avif" => (PreviewType::Image, "image/avif"),
        "svg" => (PreviewType::Image, "image/svg+xml"),
        "bmp" => (PreviewType::Image, "image/bmp"),
        "ico" => (PreviewType::Image, "image/x-icon"),

        // Video (Browser native)
        "mp4" => (PreviewType::Video, "video/mp4"),
        "webm" => (PreviewType::Video, "video/webm"),
        "mov" => (PreviewType::Video, "video/quicktime"),
        "mkv" => (PreviewType::Video, "video/x-matroska"),
        "ogg" | "ogv" => (PreviewType::Video, "video/ogg"),
        "m4v" => (PreviewType::Video, "video/x-m4v"),

        // Audio (Browser native)
        "mp3" => (PreviewType::Audio, "audio/mpeg"),
        "flac" => (PreviewType::Audio, "audio/flac"),
        "oga" => (PreviewType::Audio, "audio/ogg"),
        "opus" => (PreviewType::Audio, "audio/opus"),
        "wav" => (PreviewType::Audio, "audio/wav"),
        "m4a" => (PreviewType::Audio, "audio/mp4"),
        "aac" => (PreviewType::Audio, "audio/aac"),

        // PDF (Browser native PDF viewer)
        "pdf" => (PreviewType::Pdf, "application/pdf"),

        // HTML (Browser native HTML)
        "html" | "htm" => (PreviewType::Html, "text/html"),

        // Markdown document
        "md" | "markdown" => (PreviewType::Markdown, "text/markdown"),

        // Structured documents
        "json" | "jsonc" => (PreviewType::Json, "application/json"),
        "xml" => (PreviewType::Xml, "application/xml"),

        // Code
        "rs" | "go" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" | "java"
        | "css" | "scss" | "sh" | "bash" | "zsh" | "sql" | "lua" | "php" | "rb" | "swift"
        | "kt" | "dart" | "vue" | "svelte" | "yaml" | "yml" | "toml" => {
            (PreviewType::Code, "text/plain")
        }

        // Plain Text
        "txt" | "log" | "conf" | "ini" | "properties" | "env" | "text" | "csv" | "tsv" => {
            (PreviewType::Text, "text/plain")
        }

        // Unsupported (Office files, Archives, Executables, Unknown)
        // Explicitly, office files (.doc, .docx, .xls, .xlsx, .ppt, .pptx) are unsupported
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
        assert_eq!(detect_from_path("readme.md").0, PreviewType::Markdown);
        assert_eq!(detect_from_path("main.rs").0, PreviewType::Code);
        assert_eq!(detect_from_path("app.log").0, PreviewType::Text);
        assert_eq!(detect_from_path("data.csv").0, PreviewType::Text);
        assert_eq!(detect_from_path("config.json").0, PreviewType::Json);
        assert_eq!(detect_from_path("data.xml").0, PreviewType::Xml);
        assert_eq!(detect_from_path("archive.zip").0, PreviewType::Unknown);
        assert_eq!(detect_from_path("archive.tar.gz").0, PreviewType::Unknown);
        assert_eq!(detect_from_path("sheet.xlsx").0, PreviewType::Unknown);
        assert_eq!(detect_from_path("doc.docx").0, PreviewType::Unknown);
        assert_eq!(detect_from_path("slides.pptx").0, PreviewType::Unknown);
        assert_eq!(detect_from_path("no_ext").0, PreviewType::Unknown);
    }
}
