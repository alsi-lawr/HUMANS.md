use serde::Deserialize;

use crate::diagnostic::Diagnostic;

#[derive(Deserialize)]
struct Metadata {
    refs: Option<Vec<String>>,
    attachments: Option<Vec<String>>,
    status: Option<String>,
    decision: Option<String>,
}

pub fn arrays(path: &str, text: &str) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>> {
    let Some(frontmatter) = text.strip_prefix("---\n").and_then(|rest| {
        rest.split_once("\n---\n")
            .map(|(frontmatter, _)| frontmatter)
    }) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let value: Metadata = serde_saphyr::from_str(frontmatter).map_err(|error| {
        vec![Diagnostic::new(
            path,
            "invalid_frontmatter",
            error.to_string(),
        )]
    })?;
    Ok((
        value.refs.unwrap_or_default(),
        value.attachments.unwrap_or_default(),
    ))
}

pub(crate) fn value(text: &str, key: &str) -> Option<String> {
    let frontmatter = text.strip_prefix("---\n")?.split_once("\n---\n")?.0;
    let value: Metadata = serde_saphyr::from_str(frontmatter).ok()?;
    match key {
        "status" => value.status,
        "decision" => value.decision,
        _ => None,
    }
}
