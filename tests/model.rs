use rulist::model::{FileObj, natural_cmp, sort_files_by, sorted_file_page};

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
    let files: Vec<FileObj> = (0..300)
        .map(|index| FileObj::new(format!("file{index}.txt"), index, false, ""))
        .collect();
    let mut sorted = files.clone();
    sort_files_by(&mut sorted, Some("name"), true);

    let mut paged = files;
    let page = sorted_file_page(&mut paged, Some("name"), true, 3, 50);
    let names: Vec<&str> = page.iter().map(|file| file.name.as_str()).collect();
    let expected: Vec<&str> = sorted[100..150]
        .iter()
        .map(|file| file.name.as_str())
        .collect();
    assert_eq!(names, expected);
}
