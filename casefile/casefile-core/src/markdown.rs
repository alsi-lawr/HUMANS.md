use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::{diagnostic::Diagnostic, record::RecordSummary};

pub(crate) struct HeadingSections {
    pub title: String,
    pub sections: Vec<(String, usize)>,
}

#[allow(clippy::result_large_err)]
pub(crate) fn heading_sections(path: &str, text: &str) -> Result<HeadingSections, Diagnostic> {
    let mut title = None;
    let mut sections = Vec::new();
    let mut level = None;
    let mut start = 0;
    let mut current = String::new();
    for (event, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level: heading, .. }) => {
                level = matches!(heading, HeadingLevel::H1 | HeadingLevel::H2).then_some(heading);
                start = range.start;
                current.clear();
            }
            Event::Text(value) | Event::Code(value) if level.is_some() => current.push_str(&value),
            Event::End(TagEnd::Heading(_)) => match level.take() {
                Some(HeadingLevel::H1) if title.is_some() => {
                    return Err(Diagnostic::new(
                        path,
                        "h1_count",
                        "Markdown record must contain exactly one H1",
                    ));
                }
                Some(HeadingLevel::H1) => title = Some(current.trim().to_owned()),
                Some(HeadingLevel::H2) => sections.push((current.trim().to_owned(), start)),
                _ => {}
            },
            _ => {}
        }
    }
    let title = title.ok_or_else(|| {
        Diagnostic::new(
            path,
            "h1_count",
            "Markdown record must contain exactly one H1",
        )
    })?;
    Ok(HeadingSections { title, sections })
}

#[allow(clippy::result_large_err)]
pub fn markdown_headings(path: &str, text: &str) -> Result<(Vec<String>, Vec<String>), Diagnostic> {
    let headings = heading_sections(path, text)?;
    Ok((
        vec![headings.title],
        headings
            .sections
            .into_iter()
            .map(|(name, _)| name)
            .collect(),
    ))
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
