use crate::{diagnostic::Diagnostic, markdown::markdown_headings, metadata, record::RecordSummary};

pub fn parse(
    path: &str,
    text: &str,
) -> Result<(Option<String>, Option<RecordSummary>), Vec<Diagnostic>> {
    let metadata = metadata::parse(path, text)?;
    let (mut h1, h2) = markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
    let heading_form = h2.iter().any(|heading| heading == "Status")
        && h2
            .iter()
            .any(|heading| heading == "Human decision" || heading == "Decision");
    let frontmatter_form =
        metadata.is_some_and(|value| value.status.is_some() && value.decision.is_some());
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
    let id = stem.split_once("-D-").and_then(|(prefix, rest)| {
        let number = rest.split('-').next().unwrap_or_default();
        (!prefix.is_empty()
            && !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit()))
        .then_some(&stem[..prefix.len() + 3 + number.len()])
    });
    let Some(id) = id.filter(|id| {
        h1[0]
            .split(|character: char| {
                !character.is_alphanumeric() && !matches!(character, '_' | '-')
            })
            .any(|token| token == *id)
    }) else {
        return Err(vec![Diagnostic::new(
            path,
            "decision_filename_identity",
            "decision H1 must contain the complete filename decision ID as a token",
        )]);
    };
    Ok((
        Some(id.to_owned()),
        Some(RecordSummary::Markdown {
            title: h1.remove(0),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn heading_form(title: &str) -> String {
        format!("# {title}\n\n## Status\n\nAccepted.\n\n## Human decision\n\nProceed.\n")
    }

    #[test]
    fn filename_identity_cannot_be_satisfied_by_prefixes_or_longer_heading_tokens() {
        let path = "decision-log/HMD-D-001-scope.md";
        for title in [
            "HMD",
            "HMD-D",
            "HMD-D-0010",
            "HMD-D-001-suffix",
            "preHMD-D-001",
            "HMD-D-001é",
        ] {
            let errors = parse(path, &heading_form(title)).expect_err("lookalike identity");
            assert_eq!(errors[0].code, "decision_filename_identity");
        }
        assert_eq!(
            parse(path, &heading_form("[HMD-D-001]: scope"))
                .unwrap()
                .0
                .as_deref(),
            Some("HMD-D-001")
        );
        assert_eq!(
            parse("decision-log/demo-D-7.md", &heading_form("demo-D-7"))
                .unwrap()
                .0
                .as_deref(),
            Some("demo-D-7")
        );
    }

    #[test]
    fn heading_shape_cannot_bypass_present_malformed_frontmatter() {
        let path = "decision-log/HMD-D-001-scope.md";
        let body = heading_form("HMD-D-001 — Scope");
        assert!(parse(path, &body).is_ok());
        for frontmatter in [
            "---\nrefs: [\n---\n",
            "---\nstatus: [accepted]\n---\n",
            "---\ndecision: {nested: value}\n---\n",
            "---\nstatus: accepted\n",
        ] {
            let errors =
                parse(path, &(frontmatter.to_owned() + &body)).expect_err("malformed metadata");
            assert_eq!(errors[0].code, "invalid_frontmatter");
        }
        let metadata_form =
            "---\nstatus: accepted\ndecision: proceed\n---\n\n# HMD-D-001 — Scope\n";
        assert_eq!(
            parse(path, metadata_form).unwrap().0.as_deref(),
            Some("HMD-D-001")
        );
    }
}
