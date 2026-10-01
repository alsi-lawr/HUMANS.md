use crate::{layout::safe_relative, store::StoreError};
use casefile_core::{Classification, Diagnostic, Kind, RecordSummary, SCHEMA_VERSION, stable};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationState {
    Unactivated,
    Active,
    Invalid,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activation {
    pub(super) schema_version: Option<i64>,
    #[serde(default)]
    pub(super) projects: BTreeMap<String, Project>,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Project {
    pub(super) prefix: String,
    pub(super) investigations: Vec<String>,
}

pub(super) fn activation(
    root: &Path,
) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
    let path = root.join("casefile.toml");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(activation_content(None));
        }
        Err(error) => return Err(error.into()),
    };
    Ok(activation_content(Some(&bytes)))
}

pub(super) fn activation_content(
    bytes: Option<&[u8]>,
) -> (ActivationState, Activation, Vec<Diagnostic>) {
    let Some(bytes) = bytes else {
        return (
            ActivationState::Unactivated,
            Activation::default(),
            Vec::new(),
        );
    };
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            return (
                ActivationState::Invalid,
                Activation::default(),
                vec![Diagnostic::new(
                    "casefile.toml",
                    "invalid_activation",
                    error.to_string(),
                )],
            );
        }
    };
    let activation: Activation = match toml::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return (
                ActivationState::Invalid,
                Activation::default(),
                vec![Diagnostic::new(
                    "casefile.toml",
                    "invalid_activation",
                    error.to_string(),
                )],
            );
        }
    };
    let mut diagnostics = Vec::new();
    let mut prefixes = BTreeSet::new();
    if activation.schema_version != Some(i64::from(SCHEMA_VERSION)) {
        diagnostics.push(Diagnostic::new(
            "casefile.toml",
            "invalid_schema_version",
            "schema_version must be 1",
        ));
    }
    static PREFIX_PATTERN: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9_]*$").expect("fixed regex"));
    for (slug, project) in &activation.projects {
        if !PREFIX_PATTERN.is_match(&project.prefix) || !prefixes.insert(&project.prefix) {
            diagnostics.push(
                Diagnostic::new(
                    "casefile.toml",
                    "invalid_project_prefix",
                    "project prefixes must be unique uppercase identifiers",
                )
                .field(slug),
            );
        }
        for investigation in &project.investigations {
            let expected = format!("projects/{slug}/investigations/");
            if !investigation.starts_with(&expected) || !safe_relative(investigation) {
                diagnostics.push(Diagnostic::new("casefile.toml", "invalid_investigation_path", "governed investigation paths must be contained beneath the project investigations directory").field(slug));
            }
        }
    }
    let state = if diagnostics.is_empty() {
        ActivationState::Active
    } else {
        ActivationState::Invalid
    };
    (state, activation, stable(diagnostics))
}

pub(super) fn activation_entry(
    path: &str,
    bytes: &[u8],
    active: &Activation,
) -> (
    Classification,
    Option<Kind>,
    Option<String>,
    Option<RecordSummary>,
    Vec<Diagnostic>,
) {
    let (state, _, mut diagnostics) = activation_content(Some(bytes));
    if state == ActivationState::Active {
        (
            Classification::Governed,
            Some(Kind::Activation),
            None,
            Some(RecordSummary::Activation {
                projects: active.projects.keys().cloned().collect(),
            }),
            diagnostics,
        )
    } else {
        for diagnostic in &mut diagnostics {
            diagnostic.path = path.into();
        }
        (
            Classification::Invalid,
            Some(Kind::Activation),
            None,
            None,
            diagnostics,
        )
    }
}

pub(super) fn scope_for<'a>(path: &str, active: &'a Activation) -> Option<&'a str> {
    active
        .projects
        .values()
        .flat_map(|project| &project.investigations)
        .filter(|base| {
            path.strip_prefix(base.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|base| base.len())
        .map(String::as_str)
}

pub(super) fn investigation_identity<'a>(project: &str, investigation: &'a str) -> Option<&'a str> {
    investigation
        .strip_prefix("projects/")?
        .strip_prefix(project)?
        .strip_prefix("/investigations/")
}

pub(super) fn project_for<'a>(path: &str, active: &'a Activation) -> Option<&'a str> {
    active
        .projects
        .keys()
        .find(|slug| {
            path.strip_prefix("projects/")
                .and_then(|rest| rest.strip_prefix(slug.as_str()))
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .map(String::as_str)
}

pub(super) fn contains_path(scope: &str, path: &str) -> bool {
    path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(super) struct ScopeIndex<'a> {
    roots: BTreeMap<&'a str, &'a str>,
    active: &'a Activation,
}

#[derive(Clone, Copy)]
pub(super) struct PathFacts<'a> {
    pub project: Option<&'a str>,
    pub scope: Option<&'a str>,
    pub kind: Option<Kind>,
}

impl<'a> ScopeIndex<'a> {
    pub fn new(active: &'a Activation) -> Self {
        Self {
            roots: active
                .projects
                .iter()
                .flat_map(|(project, config)| {
                    config
                        .investigations
                        .iter()
                        .map(move |root| (root.as_str(), project.as_str()))
                })
                .collect(),
            active,
        }
    }

    pub fn resolve(&self, path: &str) -> PathFacts<'a> {
        let project = path
            .strip_prefix("projects/")
            .and_then(|rest| rest.split_once('/'))
            .and_then(|(project, _)| {
                self.active
                    .projects
                    .get_key_value(project)
                    .map(|(project, _)| project.as_str())
            });
        let mut ancestor = path;
        let mut scope = self.roots.get_key_value(path).map(|(root, _)| *root);
        while scope.is_none() {
            let Some((parent, _)) = ancestor.rsplit_once('/') else {
                break;
            };
            if let Some((root, _)) = self.roots.get_key_value(parent) {
                scope = Some(*root);
                break;
            }
            ancestor = parent;
        }
        let kind = if path == "casefile.toml" {
            Some(Kind::Activation)
        } else if path == "projects.toml" {
            Some(Kind::ProjectMap)
        } else if project.is_some_and(|project| crate::layout::project_decision(path, project)) {
            Some(Kind::Decision)
        } else {
            scope.and_then(|scope| crate::layout::kind_in_scope(path, scope))
        };
        PathFacts {
            project,
            scope,
            kind,
        }
    }
}
