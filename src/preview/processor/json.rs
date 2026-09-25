use crate::preview::types::ProcessedContent;

pub fn process(text: &str) -> Result<ProcessedContent, &'static str> {
    let value = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| text.to_string()),
        Err(_) => text.to_string(),
    };
    Ok(ProcessedContent {
        kind: "json".to_string(),
        value,
    })
}
