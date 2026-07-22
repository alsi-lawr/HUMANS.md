use crate::{diagnostic::Diagnostic, markdown::markdown_headings, metadata, record::RecordSummary};

pub fn parse(
    path: &str,
    text: &str,
) -> Result<(Option<String>, Option<RecordSummary>), Vec<Diagnostic>> {
    let (h1, h2) = markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
    let heading_form = h2.iter().any(|heading| heading == "Status")
        && h2
            .iter()
            .any(|heading| heading == "Human decision" || heading == "Decision");
    let frontmatter_form =
        metadata::value(text, "status").is_some() && metadata::value(text, "decision").is_some();
    if !heading_form && !frontmatter_form {
        return Err(vec![Diagnostic::new(
            path,
            "decision_shape",
            "decision needs status and decision in frontmatter or H2 sections",
        )]);
    }
    let stem = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".md"))
        .unwrap_or_default();
    let parts: Vec<_> = stem.split('-').collect();
    let id = (1..parts.len())
        .map(|count| parts[..count].join("-"))
        .filter(|candidate| h1[0].contains(candidate))
        .max_by_key(String::len);
    let Some(id) = id else {
        return Err(vec![Diagnostic::new(
            path,
            "decision_filename_identity",
            "decision H1 must contain the filename ID prefix",
        )]);
    };
    Ok((
        Some(id),
        Some(RecordSummary::Markdown {
            title: h1[0].clone(),
        }),
    ))
}
