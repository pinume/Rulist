use crate::preview::types::ProcessedContent;

pub fn process(text: &str) -> Result<ProcessedContent, &'static str> {
    Ok(ProcessedContent {
        kind: "xml".to_string(),
        value: text.to_string(),
    })
}
