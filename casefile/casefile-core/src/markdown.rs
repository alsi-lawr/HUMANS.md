use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::{diagnostic::Diagnostic, record::RecordSummary};

#[allow(clippy::result_large_err)]
pub fn markdown_headings(path: &str, text: &str) -> Result<(Vec<String>, Vec<String>), Diagnostic> {
    let mut h1 = Vec::new();
    let mut h2 = Vec::new();
    let mut level = None;
    let mut current = String::new();
    for event in Parser::new_ext(text, Options::all()) {
        match event {
            Event::Start(Tag::Heading { level: heading, .. }) => {
                level = Some(heading);
                current.clear();
            }
            Event::Text(value) | Event::Code(value) if level.is_some() => current.push_str(&value),
            Event::End(TagEnd::Heading(_)) => match level.take() {
                Some(pulldown_cmark::HeadingLevel::H1) => h1.push(current.trim().into()),
                Some(pulldown_cmark::HeadingLevel::H2) => h2.push(current.trim().into()),
                _ => {}
            },
            _ => {}
        }
    }
    if h1.len() != 1 {
        return Err(Diagnostic::new(
            path,
            "h1_count",
            "Markdown record must contain exactly one H1",
        ));
    }
    Ok((h1, h2))
}

pub fn validate_markdown(
    path: &str,
    text: &str,
    required_h2: &[&str],
    title_contains: Option<&str>,
) -> Result<RecordSummary, Vec<Diagnostic>> {
    let (mut h1, h2) = markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
    if title_contains.is_some_and(|value| !h1[0].contains(value)) {
        return Err(vec![Diagnostic::new(
            path,
            "identity_heading",
            "H1 must contain the record ID",
        )]);
    }
    for expected in required_h2 {
        if !h2.iter().any(|actual| actual == expected) {
            return Err(vec![
                Diagnostic::new(path, "missing_section", "required H2 is missing")
                    .section(expected),
            ]);
        }
    }
    Ok(RecordSummary::Markdown {
        title: h1.remove(0),
    })
}
