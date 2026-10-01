use rulist::filesystem::{FileEntry, natural_cmp, sort_files_by, sorted_file_page};

#[test]
fn natural_sort_orders_numeric_suffixes() {
    let mut names = vec!["file10.txt", "file2.txt", "file1.txt", "file20.txt"];
    names.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(
        names,
        vec!["file1.txt", "file2.txt", "file10.txt", "file20.txt"]
    );
}

#[test]
fn paginated_sort_matches_full_sort() {
    let files: Vec<FileEntry> = (0..300)
        .map(|index| FileEntry::new(format!("file{index}.txt"), index, false, ""))
        .collect();
    for order_by in ["name", "size", "modified"] {
        for reverse in [false, true] {
            let mut sorted = files.clone();
            sort_files_by(&mut sorted, Some(order_by), reverse);
            for (page, per_page) in [(1, 50), (3, 50), (6, 50), (7, 50), (2, usize::MAX / 2 + 1)] {
                let mut paged = files.clone();
                let actual = sorted_file_page(&mut paged, Some(order_by), reverse, page, per_page);
                let start = (page - 1) * per_page;
                let expected = &sorted
                    [start.min(sorted.len())..start.saturating_add(per_page).min(sorted.len())];
                assert_eq!(
                    actual.iter().map(|file| &file.name).collect::<Vec<_>>(),
                    expected.iter().map(|file| &file.name).collect::<Vec<_>>()
                );
            }
        }
    }
}
