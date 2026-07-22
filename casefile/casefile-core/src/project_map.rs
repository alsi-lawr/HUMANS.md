use crate::{diagnostic::Diagnostic, record::RecordSummary};

pub fn parse(
    path: &str,
    bytes: &[u8],
    governed_projects: &[&str],
) -> Result<RecordSummary, Vec<Diagnostic>> {
    let projects = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(text).ok())
        .and_then(|value| {
            value
                .get("projects")
                .and_then(toml::Value::as_table)
                .cloned()
        });
    match projects {
        Some(projects)
            if projects.values().all(toml::Value::is_str)
                && governed_projects
                    .iter()
                    .all(|key| projects.contains_key(*key)) =>
        {
            Ok(RecordSummary::ProjectMap {
                projects: projects.keys().cloned().collect(),
            })
        }
        _ => Err(vec![Diagnostic::new(
            path,
            "invalid_project_map",
            "projects.toml must contain strings for governed project keys",
        )]),
    }
}
