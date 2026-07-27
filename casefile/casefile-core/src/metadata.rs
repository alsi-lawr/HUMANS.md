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
    let Some(frontmatter) = strip_opening(text)
        .and_then(split_closing)
        .map(|(frontmatter, _)| frontmatter)
    else {
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
    let frontmatter = split_closing(strip_opening(text)?).map(|(frontmatter, _)| frontmatter)?;
    let value: Metadata = serde_saphyr::from_str(frontmatter).ok()?;
    match key {
        "status" => value.status,
        "decision" => value.decision,
        _ => None,
    }
}

pub(crate) fn strip_opening(text: &str) -> Option<&str> {
    text.strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
}

pub(crate) fn split_closing(text: &str) -> Option<(&str, &str)> {
    text.split_once("\n---\n")
        .or_else(|| text.split_once("\r\n---\r\n"))
}

#[cfg(test)]
mod tests {
    use super::{arrays, value};

    #[test]
    fn parses_crlf_frontmatter_without_normalizing_metadata() {
        let text = "---\r\nrefs: [HMD-D-001]\r\nattachments: [observation.txt]\r\nstatus: accepted\r\ndecision: approve\r\n---\r\n\r\n# Evidence\r\n";

        assert_eq!(
            arrays("evidence.md", text).expect("metadata"),
            (
                vec!["HMD-D-001".to_owned()],
                vec!["observation.txt".to_owned()]
            )
        );
        assert_eq!(value(text, "status").as_deref(), Some("accepted"));
        assert_eq!(value(text, "decision").as_deref(), Some("approve"));
    }
}
