pub const MAX_TEXT_PREVIEW_SIZE: u64 = 4 * 1024 * 1024; // 4 MiB
pub const PROCESSED_PREVIEW_CONCURRENCY: usize = 8;

pub const MAX_CSV_SIZE: u64 = 4 * 1024 * 1024; // 4 MiB
pub const MAX_CSV_ROWS: usize = 500;
pub const MAX_CSV_COLUMNS: usize = 200;

pub const MAX_ARCHIVE_FILE_SIZE: u64 = 100 * 1024 * 1024; // 100 MiB
pub const MAX_ARCHIVE_ENTRIES: usize = 10_000;
pub const MAX_ARCHIVE_NAME_LENGTH: usize = 1_024;
pub const MAX_ARCHIVE_TOTAL_UNCOMPRESSED: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
pub const MAX_ARCHIVE_COMPRESSION_RATIO: u64 = 1000;
