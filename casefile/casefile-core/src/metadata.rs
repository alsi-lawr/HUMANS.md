use serde::Deserialize;

use crate::diagnostic::Diagnostic;

#[derive(Deserialize)]
pub(crate) struct Metadata {
    refs: Option<Vec<String>>,
    attachments: Option<Vec<String>>,
    pub(crate) status: Option<String>,
    pub(crate) decision: Option<String>,
}

pub fn arrays(path: &str, text: &str) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>> {
    let Some(value) = parse(path, text)? else {
        return Ok((Vec::new(), Vec::new()));
    };
    Ok((
        value.refs.unwrap_or_default(),
        value.attachments.unwrap_or_default(),
    ))
}

pub(crate) fn parse(path: &str, text: &str) -> Result<Option<Metadata>, Vec<Diagnostic>> {
    let Some(rest) = strip_opening(text) else {
        return Ok(None);
    };
    let (frontmatter, _) = split_closing(rest).ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_frontmatter",
            "frontmatter closing delimiter is missing",
        )]
    })?;
    serde_saphyr::from_str(frontmatter)
        .map(Some)
        .map_err(|error| {
            vec![Diagnostic::new(
                path,
                "invalid_frontmatter",
                error.to_string(),
            )]
        })
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
    use super::{arrays, parse};

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
        let metadata = parse("evidence.md", text)
            .expect("metadata")
            .expect("frontmatter");
        assert_eq!(metadata.status.as_deref(), Some("accepted"));
        assert_eq!(metadata.decision.as_deref(), Some("approve"));
    }
}
