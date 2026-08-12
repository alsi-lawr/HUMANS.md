use crate::{diagnostic::Diagnostic, record::RecordSummary};
use std::collections::BTreeMap;

pub type ProjectMap = BTreeMap<String, String>;

pub fn parse_map(path: &str, bytes: &[u8]) -> Result<ProjectMap, Vec<Diagnostic>> {
    let projects = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(text).ok())
        .and_then(|value| {
            value
                .get("projects")
                .and_then(toml::Value::as_table)
                .cloned()
        })
        .filter(|projects| projects.values().all(toml::Value::is_str))
        .map(|projects| {
            projects
                .into_iter()
                .map(|(name, value)| (name, value.as_str().expect("filtered string").into()))
                .collect()
        });
    projects.ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_project_map",
            "projects.toml must contain string project source roots",
        )]
    })
}

pub fn parse(
    path: &str,
    bytes: &[u8],
    governed_projects: &[&str],
) -> Result<RecordSummary, Vec<Diagnostic>> {
    match parse_map(path, bytes) {
        Ok(projects)
            if governed_projects
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
