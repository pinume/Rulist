use crate::preview::limits::{MAX_CSV_COLUMNS, MAX_CSV_ROWS};
use crate::preview::processor::text;
use crate::preview::types::ProcessedContent;

pub fn process(text: &str, path: &str) -> Result<ProcessedContent, &'static str> {
    let delimiter = if path.to_lowercase().ends_with(".tsv") {
        b'\t'
    } else {
        b','
    };

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());

    let headers: Vec<String> = match rdr.headers() {
        Ok(h) => h
            .iter()
            .take(MAX_CSV_COLUMNS)
            .map(|s| s.to_string())
            .collect(),
        Err(_) => return text::process(text),
    };

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut truncated = false;

    for result in rdr.records() {
        match result {
            Ok(record) => {
                if rows.len() >= MAX_CSV_ROWS {
                    truncated = true;
                    break;
                }
                let row: Vec<String> = record
                    .iter()
                    .take(MAX_CSV_COLUMNS)
                    .map(|s| s.to_string())
                    .collect();
                rows.push(row);
            }
            Err(_) => return text::process(text),
        }
    }

    Ok(ProcessedContent {
        kind: "csv_table".to_string(),
        value: serde_json::json!({
            "headers": headers,
            "rows": rows,
            "truncated": truncated
        })
        .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_process() {
        let content = "name,age,city\nAlice,30,New York\nBob,25,London";
        let res = process(content, "test.csv").unwrap();
        assert_eq!(res.kind, "csv_table");
        let v: serde_json::Value = serde_json::from_str(&res.value).unwrap();
        assert_eq!(v["headers"], serde_json::json!(["name", "age", "city"]));
        assert_eq!(v["rows"].as_array().unwrap().len(), 2);
        assert_eq!(v["truncated"], false);
    }

    #[test]
    fn test_tsv_process() {
        let content = "a\tb\tc\n1\t2\t3";
        let res = process(content, "test.tsv").unwrap();
        assert_eq!(res.kind, "csv_table");
        let v: serde_json::Value = serde_json::from_str(&res.value).unwrap();
        assert_eq!(v["headers"], serde_json::json!(["a", "b", "c"]));
        assert_eq!(v["rows"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_csv_truncation() {
        let mut lines = vec!["h1,h2".to_string()];
        for i in 0..600 {
            lines.push(format!("val1_{i},val2_{i}"));
        }
        let content = lines.join("\n");
        let res = process(&content, "big.csv").unwrap();
        assert_eq!(res.kind, "csv_table");
        let v: serde_json::Value = serde_json::from_str(&res.value).unwrap();
        assert_eq!(v["rows"].as_array().unwrap().len(), MAX_CSV_ROWS);
        assert_eq!(v["truncated"], true);
    }
}
