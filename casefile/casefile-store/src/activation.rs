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

#[derive(Default, Deserialize)]
pub(super) struct Activation {
    schema_version: Option<i64>,
    #[serde(default)]
    pub(super) projects: BTreeMap<String, Project>,
}
#[derive(Deserialize)]
pub(super) struct Project {
    pub(super) prefix: String,
    pub(super) investigations: Vec<String>,
}

pub(super) fn activation(
    root: &Path,
) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
    let path = root.join("casefile.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((
                ActivationState::Unactivated,
                Activation::default(),
                Vec::new(),
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let activation: Activation = match toml::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            return Ok((
                ActivationState::Invalid,
                Activation::default(),
                vec![Diagnostic::new(
                    "casefile.toml",
                    "invalid_activation",
                    error.to_string(),
                )],
            ));
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
    let prefix_pattern = Regex::new(r"^[A-Z][A-Z0-9_]*$").expect("fixed regex");
    for (slug, project) in &activation.projects {
        if !prefix_pattern.is_match(&project.prefix) || !prefixes.insert(&project.prefix) {
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
    Ok((state, activation, stable(diagnostics)))
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
    let mut diagnostics = activation_from_bytes(bytes);
    if diagnostics.is_empty() {
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
        diagnostics
            .iter_mut()
            .for_each(|item| item.path = path.into());
        (
            Classification::Invalid,
            Some(Kind::Activation),
            None,
            None,
            diagnostics,
        )
    }
}
fn activation_from_bytes(bytes: &[u8]) -> Vec<Diagnostic> {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            return vec![Diagnostic::new(
                "casefile.toml",
                "invalid_activation",
                "activation must be UTF-8 TOML",
            )];
        }
    };
    let activation: Activation = match toml::from_str(text) {
        Ok(activation) => activation,
        Err(error) => {
            return vec![Diagnostic::new(
                "casefile.toml",
                "invalid_activation",
                error.to_string(),
            )];
        }
    };
    let mut prefixes = BTreeSet::new();
    let mut diagnostics = Vec::new();
    if activation.schema_version != Some(i64::from(SCHEMA_VERSION)) {
        diagnostics.push(Diagnostic::new(
            "casefile.toml",
            "invalid_schema_version",
            "schema_version must be 1",
        ));
    }
    let prefix_pattern = Regex::new(r"^[A-Z][A-Z0-9_]*$").expect("fixed regex");
    for (slug, project) in activation.projects {
        if !prefix_pattern.is_match(&project.prefix) || !prefixes.insert(project.prefix) {
            diagnostics.push(
                Diagnostic::new(
                    "casefile.toml",
                    "invalid_project_prefix",
                    "project prefixes must be unique uppercase identifiers",
                )
                .field(&slug),
            );
        }
    }
    diagnostics
}

pub(super) fn scope_for<'a>(path: &str, active: &'a Activation) -> Option<&'a str> {
    active
        .projects
        .values()
        .flat_map(|project| &project.investigations)
        .find_map(|base| {
            path.strip_prefix(&(base.to_owned() + "/"))
                .map(|_| base.as_str())
        })
}

pub(super) fn investigation_identity<'a>(project: &str, investigation: &'a str) -> Option<&'a str> {
    investigation.strip_prefix(&format!("projects/{project}/investigations/"))
}

pub(super) fn project_for<'a>(path: &str, active: &'a Activation) -> Option<&'a str> {
    active
        .projects
        .keys()
        .find(|slug| path.starts_with(&format!("projects/{slug}/")))
        .map(String::as_str)
}
