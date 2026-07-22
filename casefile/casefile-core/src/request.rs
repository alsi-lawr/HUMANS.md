use crate::{diagnostic::Diagnostic, markdown::markdown_headings, record::RecordSummary};

pub fn parse(path: &str, text: &str) -> Result<RecordSummary, Vec<Diagnostic>> {
    let (h1, h2) = markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
    if h1[0] != "Request" || !h2.iter().any(|heading| heading == "Boundary") {
        return Err(vec![Diagnostic::new(
            path,
            "request_shape",
            "request needs H1 Request and H2 Boundary",
        )]);
    }
    Ok(RecordSummary::Markdown {
        title: h1[0].clone(),
    })
}
