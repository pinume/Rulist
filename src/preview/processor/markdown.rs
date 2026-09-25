use crate::preview::types::ProcessedContent;

pub fn process(text: &str) -> Result<ProcessedContent, &'static str> {
    let mut html = String::new();
    let parser = pulldown_cmark::Parser::new_ext(text, pulldown_cmark::Options::all());
    pulldown_cmark::html::push_html(&mut html, parser);
    let sanitized = ammonia::clean(&html);
    Ok(ProcessedContent {
        kind: "html".to_string(),
        value: sanitized,
    })
}
