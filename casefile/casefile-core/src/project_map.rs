use crate::{diagnostic::Diagnostic, record::RecordSummary};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

pub type ProjectMap = BTreeMap<String, String>;

pub fn parse_map(path: &str, bytes: &[u8]) -> Result<ProjectMap, Vec<Diagnostic>> {
    #[derive(Deserialize)]
    struct MapWire {
        projects: ProjectMap,
    }

    let projects = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| toml::from_str::<MapWire>(text).ok())
        .map(|wire| wire.projects)
        .filter(|projects| projects.values().all(|root| Path::new(root).is_absolute()));
    projects.ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_project_map",
            "projects.toml must contain platform-absolute string project source roots",
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
