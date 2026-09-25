use std::io::Cursor;

use crate::preview::limits::{
    MAX_ARCHIVE_COMPRESSION_RATIO, MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_NAME_LENGTH,
    MAX_ARCHIVE_TOTAL_UNCOMPRESSED,
};
use crate::preview::types::ProcessedContent;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<String>,
}

pub fn process(bytes: &[u8], path: &str) -> Result<ProcessedContent, &'static str> {
    let lower_path = path.to_lowercase();
    let is_zip = lower_path.ends_with(".zip")
        || bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06");
    let is_tar_gz = lower_path.ends_with(".tar.gz")
        || lower_path.ends_with(".tgz")
        || lower_path.ends_with(".gz")
        || bytes.starts_with(&[0x1f, 0x8b]);

    let (entries, total_size) = if is_zip {
        process_zip(bytes)?
    } else if is_tar_gz {
        process_tar_gz(bytes)?
    } else {
        process_tar(bytes)?
    };

    Ok(ProcessedContent {
        kind: "archive_tree".to_string(),
        value: serde_json::json!({
            "entries": entries,
            "total_entries": entries.len(),
            "total_size": total_size
        })
        .to_string(),
    })
}

fn process_zip(bytes: &[u8]) -> Result<(Vec<ArchiveEntry>, u64), &'static str> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|_| "archive_error")?;

    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("resource_limit");
    }

    let mut entries = Vec::with_capacity(archive.len());
    let mut total_size: u64 = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|_| "archive_error")?;
        let name = file.name().to_string();

        if name.len() > MAX_ARCHIVE_NAME_LENGTH {
            return Err("resource_limit");
        }

        let uncompressed_size = file.size();
        let compressed_size = file.compressed_size();

        if compressed_size > 0
            && uncompressed_size / compressed_size > MAX_ARCHIVE_COMPRESSION_RATIO
        {
            return Err("suspicious_compression_ratio");
        }

        total_size = total_size.saturating_add(uncompressed_size);
        if total_size > MAX_ARCHIVE_TOTAL_UNCOMPRESSED {
            return Err("resource_limit");
        }

        let is_dir = file.is_dir();
        let modified = file.last_modified().map(|dt| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                dt.year(),
                dt.month(),
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second()
            )
        });

        entries.push(ArchiveEntry {
            name,
            size: uncompressed_size,
            is_dir,
            modified,
        });
    }

    Ok((entries, total_size))
}

fn process_tar(bytes: &[u8]) -> Result<(Vec<ArchiveEntry>, u64), &'static str> {
    let reader = Cursor::new(bytes);
    let mut archive = tar::Archive::new(reader);
    read_tar_entries(&mut archive, bytes.len() as u64, false)
}

fn process_tar_gz(bytes: &[u8]) -> Result<(Vec<ArchiveEntry>, u64), &'static str> {
    let reader = Cursor::new(bytes);
    let decoder = flate2::read::GzDecoder::new(reader);
    let mut archive = tar::Archive::new(decoder);
    read_tar_entries(&mut archive, bytes.len() as u64, true)
}

fn read_tar_entries<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    compressed_bytes_len: u64,
    check_compression_ratio: bool,
) -> Result<(Vec<ArchiveEntry>, u64), &'static str> {
    let tar_entries = archive.entries().map_err(|_| "archive_error")?;
    let mut entries = Vec::new();
    let mut total_size: u64 = 0;

    for entry_result in tar_entries {
        let entry = entry_result.map_err(|_| "archive_error")?;

        if entries.len() >= MAX_ARCHIVE_ENTRIES {
            return Err("resource_limit");
        }

        let path = entry.path().map_err(|_| "archive_error")?;
        let name = path.to_string_lossy().to_string();

        if name.len() > MAX_ARCHIVE_NAME_LENGTH {
            return Err("resource_limit");
        }

        let size = entry.size();
        let is_dir = entry.header().entry_type().is_dir();

        total_size = total_size.saturating_add(size);
        if total_size > MAX_ARCHIVE_TOTAL_UNCOMPRESSED {
            return Err("resource_limit");
        }

        if check_compression_ratio
            && compressed_bytes_len > 0
            && total_size / compressed_bytes_len > MAX_ARCHIVE_COMPRESSION_RATIO
        {
            return Err("suspicious_compression_ratio");
        }

        let modified = entry.header().mtime().ok().and_then(|mtime| {
            chrono::DateTime::from_timestamp(mtime as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        });

        entries.push(ArchiveEntry {
            name,
            size,
            is_dir,
            modified,
        });
    }

    Ok((entries, total_size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_zip_processing() {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("hello.txt", options).unwrap();
            zip.write_all(b"Hello, world!").unwrap();
            zip.add_directory("subfolder/", options).unwrap();
            zip.finish().unwrap();
        }

        let res = process(&buf, "test.zip").unwrap();
        assert_eq!(res.kind, "archive_tree");
        let v: serde_json::Value = serde_json::from_str(&res.value).unwrap();
        assert_eq!(v["total_entries"], 2);
        assert_eq!(v["total_size"], 13);
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries[0]["name"], "hello.txt");
        assert_eq!(entries[0]["is_dir"], false);
        assert_eq!(entries[1]["name"], "subfolder/");
        assert_eq!(entries[1]["is_dir"], true);
    }

    #[test]
    fn test_corrupt_archive() {
        let corrupt = b"not a valid zip or tar";
        assert_eq!(process(corrupt, "corrupt.zip"), Err("archive_error"));
    }

    #[test]
    fn test_tar_processing() {
        let mut buf = Vec::new();
        {
            let mut tar = tar::Builder::new(&mut buf);
            let data = b"Tar content";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "test.txt", &data[..]).unwrap();
            tar.finish().unwrap();
        }

        let res = process(&buf, "test.tar").unwrap();
        assert_eq!(res.kind, "archive_tree");
        let v: serde_json::Value = serde_json::from_str(&res.value).unwrap();
        assert_eq!(v["total_entries"], 1);
        assert_eq!(v["total_size"], 11);
    }
}
