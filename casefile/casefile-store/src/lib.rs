//! Filesystem and Git boundary for the compact Casefile v1 contract.
#![allow(clippy::collapsible_if)] // Nested validation keeps individual rules readable.

use casefile_core::{
    ApplyResult, CasefileSnapshot, ChangeRequest, Classification, Diagnostic, EntrySnapshot, Kind,
    Preview, RecordDraft, RecordSummary, Revision, SCHEMA_VERSION, stable,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation is invalid: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanResult {
    pub activation: ActivationState,
    pub snapshot: CasefileSnapshot,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationState {
    Unactivated,
    Active,
    Invalid,
}

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err(StoreError::Invalid(
                "planning root must not be a symlink".into(),
            ));
        }
        Ok(Self { root })
    }

    pub fn scan(&self) -> Result<ScanResult, StoreError> {
        scan(&self.root, &BTreeMap::new())
    }

    pub fn preview(&self, request: ChangeRequest) -> Result<Preview, StoreError> {
        ensure_worktree(&self.root)?;
        let before = self.scan()?;
        let path = checked_path(request.path())?;
        let existing = before
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path);
        let proposed_bytes = match request.rendered() {
            Some(Ok(bytes)) => bytes,
            Some(Err(diagnostic)) => {
                return Ok(rejected(request, before.snapshot.revision, diagnostic));
            }
            None => Vec::new(),
        };
        let writable = match &request {
            ChangeRequest::Create { draft, .. } | ChangeRequest::Replace { draft, .. } => {
                Some(draft.kind())
            }
            ChangeRequest::Delete { .. } => existing.and_then(|entry| entry.kind),
        };
        let path_kind = kind_for_path(&path, &activation(&self.root)?.1);
        if !matches!(writable, Some(Kind::Ticket | Kind::Epic | Kind::Board))
            || path_kind != writable
        {
            return Ok(rejected(
                request,
                before.snapshot.revision,
                Diagnostic::new(
                    &path,
                    "read_only_or_wrong_path",
                    "only complete ticket, epic, and board drafts may target their canonical path",
                ),
            ));
        }
        match &request {
            ChangeRequest::Create { .. } if existing.is_some() => {
                return Ok(rejected(
                    request,
                    before.snapshot.revision,
                    Diagnostic::new(&path, "target_exists", "create requires an absent target"),
                ));
            }
            ChangeRequest::Replace { .. } if existing.is_none() => {
                return Ok(rejected(
                    request,
                    before.snapshot.revision,
                    Diagnostic::new(
                        &path,
                        "target_missing",
                        "replace requires an existing target",
                    ),
                ));
            }
            ChangeRequest::Delete { .. } if existing.is_none() => {
                return Ok(rejected(
                    request,
                    before.snapshot.revision,
                    Diagnostic::new(
                        &path,
                        "target_missing",
                        "delete requires an existing target",
                    ),
                ));
            }
            _ => {}
        }
        let mut overlay = BTreeMap::new();
        overlay.insert(
            path.clone(),
            if matches!(request, ChangeRequest::Delete { .. }) {
                None
            } else {
                Some(proposed_bytes.clone())
            },
        );
        let proposed = scan(&self.root, &overlay)?;
        let mut diagnostics = proposed.diagnostics;
        if diagnostics.is_empty() {
            diagnostics = Vec::new();
        }
        let diff = git_diff(
            &self.root,
            &path,
            existing.map(|entry| entry.original_bytes.as_slice()),
            if matches!(request, ChangeRequest::Delete { .. }) {
                None
            } else {
                Some(proposed_bytes.as_slice())
            },
        )?;
        Ok(Preview {
            request,
            expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
            expected_store_revision: before.snapshot.revision,
            proposed_store_revision: proposed.snapshot.revision,
            diagnostics: stable(diagnostics),
            diff,
        })
    }

    pub fn apply(&self, preview: Preview) -> Result<ApplyResult, StoreError> {
        ensure_worktree(&self.root)?;
        if !preview.diagnostics.is_empty() {
            return Err(StoreError::Invalid(
                "preview contains validation diagnostics".into(),
            ));
        }
        let current = self.scan()?;
        if current.snapshot.revision != preview.expected_store_revision {
            return Err(StoreError::Invalid("stale store revision".into()));
        }
        let path = checked_path(preview.request.path())?;
        let current_entry = current
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path);
        if current_entry.map(|entry| &entry.content_revision)
            != preview.expected_target_revision.as_ref()
        {
            return Err(StoreError::Invalid("stale target revision".into()));
        }
        let target = self.root.join(&path);
        match &preview.request {
            ChangeRequest::Create { draft, .. } | ChangeRequest::Replace { draft, .. } => {
                let bytes = casefile_core::render_draft(&path, draft)
                    .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                if target.exists() && fs::symlink_metadata(&target)?.file_type().is_symlink() {
                    return Err(StoreError::Invalid("target must not be a symlink".into()));
                }
                atomic_write(
                    &target,
                    &bytes,
                    matches!(preview.request, ChangeRequest::Create { .. }),
                )?;
            }
            ChangeRequest::Delete { .. } => {
                let metadata = fs::symlink_metadata(&target)?;
                if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                    return Err(StoreError::Invalid(
                        "delete requires a regular non-symlink target".into(),
                    ));
                }
                fs::remove_file(&target)?;
            }
        }
        let resulting = self.scan()?;
        let target_revision = resulting
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.content_revision.clone());
        Ok(ApplyResult {
            path,
            resulting_target_revision: target_revision,
            resulting_store_revision: resulting.snapshot.revision,
            diff: preview.diff,
        })
    }
}

fn rejected(request: ChangeRequest, revision: Revision, diagnostic: Diagnostic) -> Preview {
    Preview {
        request,
        expected_target_revision: None,
        expected_store_revision: revision.clone(),
        proposed_store_revision: revision,
        diagnostics: vec![diagnostic],
        diff: String::new(),
    }
}

#[derive(Default, Deserialize)]
struct Activation {
    schema_version: Option<i64>,
    #[serde(default)]
    projects: BTreeMap<String, Project>,
}
#[derive(Deserialize)]
struct Project {
    prefix: String,
    investigations: Vec<String>,
}

fn activation(root: &Path) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
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

use regex::Regex;

fn scan(
    root: &Path,
    overlay: &BTreeMap<String, Option<Vec<u8>>>,
) -> Result<ScanResult, StoreError> {
    let (activation, active, mut diagnostics) = activation(root)?;
    let mut files = BTreeMap::new();
    let mut unsafe_paths = BTreeSet::new();
    collect(root, root, &mut files, &mut unsafe_paths)?;
    for (path, bytes) in overlay {
        match bytes {
            Some(bytes) => {
                files.insert(path.clone(), bytes.clone());
            }
            None => {
                files.remove(path);
            }
        }
    }
    let mut entries = Vec::new();
    for (path, bytes) in files {
        let (classification, kind, identity, summary, mut found) =
            if activation == ActivationState::Unactivated {
                (Classification::Ungoverned, None, None, None, Vec::new())
            } else if unsafe_paths.contains(&path) {
                invalid(
                    &path,
                    kind_for_path(&path, &active),
                    "unsafe_path",
                    "governed paths cannot be symlinks",
                )
            } else {
                classify(&path, &bytes, &active)
            };
        diagnostics.append(&mut found);
        entries.push(EntrySnapshot {
            path: path.clone(),
            classification,
            kind,
            identity,
            content_revision: digest(&bytes),
            summary,
            original_bytes: bytes,
        });
    }
    diagnostics.extend(cross_validate(&entries, &active));
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut input = Vec::new();
    for entry in &entries {
        input.extend_from_slice(entry.path.as_bytes());
        input.push(0);
        input.extend_from_slice(entry.content_revision.0.as_bytes());
        input.push(0);
    }
    Ok(ScanResult {
        activation,
        snapshot: CasefileSnapshot {
            revision: digest(&input),
            entries,
        },
        diagnostics: stable(diagnostics),
    })
}

fn collect(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
    unsafe_paths: &mut BTreeSet<String>,
) -> Result<(), StoreError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let relative = relative(root, &path)?;
        if metadata.file_type().is_symlink() {
            files.insert(relative.clone(), Vec::new());
            unsafe_paths.insert(relative);
            continue;
        }
        if metadata.is_dir() {
            collect(root, &path, files, unsafe_paths)?;
        } else if metadata.is_file() {
            files.insert(relative, fs::read(path)?);
        }
    }
    Ok(())
}

fn classify(
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
    if path == "casefile.toml" {
        return activation_entry(path, bytes, active);
    }
    if path == "projects.toml" {
        return project_map_entry(path, bytes, active);
    }
    let Some(kind) = kind_for_path(path, active) else {
        return (
            if in_active(path, active) {
                Classification::Raw
            } else {
                Classification::Ungoverned
            },
            None,
            None,
            None,
            Vec::new(),
        );
    };
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            return invalid(
                path,
                Some(kind),
                "invalid_utf8",
                "governed text must be UTF-8",
            );
        }
    };
    let result = match kind {
        Kind::Ticket | Kind::Epic | Kind::Board => casefile_core::parse_draft(path, kind, text)
            .map(|draft| match draft {
                RecordDraft::Ticket(item) | RecordDraft::Epic(item) => (
                    Some(item.id.clone()),
                    Some(RecordSummary::WorkItem {
                        id: item.id,
                        title: item.title,
                        status: item.status,
                        rank: item.rank,
                    }),
                ),
                RecordDraft::Board(board) => (
                    Some(board.id.clone()),
                    Some(RecordSummary::Board {
                        id: board.id,
                        title: board.title,
                        columns: board
                            .columns
                            .into_iter()
                            .map(|column| column.name)
                            .collect(),
                    }),
                ),
            }),
        Kind::Request => request(path, text).map(|summary| (None, Some(summary))),
        Kind::Decision => decision(path, text),
        Kind::Evidence | Kind::Review => casefile_core::validate_markdown(path, text, &[], None)
            .and_then(|summary| metadata_arrays(path, text).map(|_| summary))
            .map(|summary| (None, Some(summary))),
        Kind::Plan => casefile_core::validate_markdown(path, text, &["Objective"], None)
            .map(|summary| (None, Some(summary))),
        Kind::Closeout => {
            casefile_core::validate_markdown(path, text, &["Scope disposition"], None)
                .map(|summary| (None, Some(summary)))
        }
        Kind::Strategy => strategy(path, text).map(|summary| (None, Some(summary))),
        Kind::Activation | Kind::ProjectMap => unreachable!(),
    };
    match result {
        Ok((identity, summary)) => (
            Classification::Governed,
            Some(kind),
            identity,
            summary,
            Vec::new(),
        ),
        Err(diagnostics) => (Classification::Invalid, Some(kind), None, None, diagnostics),
    }
}

fn activation_entry(
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

fn project_map_entry(
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
                && active.projects.keys().all(|key| projects.contains_key(key)) =>
        {
            (
                Classification::Governed,
                Some(Kind::ProjectMap),
                None,
                Some(RecordSummary::ProjectMap {
                    projects: projects.keys().cloned().collect(),
                }),
                Vec::new(),
            )
        }
        _ => invalid(
            path,
            Some(Kind::ProjectMap),
            "invalid_project_map",
            "projects.toml must contain strings for governed project keys",
        ),
    }
}

fn strategy(path: &str, text: &str) -> Result<RecordSummary, Vec<Diagnostic>> {
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = value.as_table().ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_strategy",
            "strategy must be a TOML table",
        )]
    })?;
    let phase = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".toml"))
        .unwrap_or_default();
    let get = |name| {
        table
            .get(name)
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                vec![
                    Diagnostic::new(
                        path,
                        "invalid_strategy",
                        "strategy fields must be non-empty",
                    )
                    .field(name),
                ]
            })
    };
    if table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(i64::from(SCHEMA_VERSION))
    {
        return Err(vec![Diagnostic::new(
            path,
            "invalid_schema_version",
            "schema_version must be 1",
        )]);
    }
    let parsed_phase = get("phase")?;
    if parsed_phase != phase {
        return Err(vec![
            Diagnostic::new(path, "strategy_phase", "phase must match filename").field("phase"),
        ]);
    }
    Ok(RecordSummary::Strategy {
        strategy_id: get("strategy_id")?,
        phase: parsed_phase,
        adapter: get("adapter")?,
    })
}

fn decision(
    path: &str,
    text: &str,
) -> Result<(Option<String>, Option<RecordSummary>), Vec<Diagnostic>> {
    let (h1, h2) =
        casefile_core::markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
    let heading_form = h2.iter().any(|heading| heading == "Status")
        && h2
            .iter()
            .any(|heading| heading == "Human decision" || heading == "Decision");
    let frontmatter_form =
        metadata_value(text, "status").is_some() && metadata_value(text, "decision").is_some();
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
    let parts: Vec<_> = stem.split('-').collect();
    let id = (1..parts.len())
        .map(|count| parts[..count].join("-"))
        .filter(|candidate| h1[0].contains(candidate))
        .max_by_key(String::len);
    let Some(id) = id else {
        return Err(vec![Diagnostic::new(
            path,
            "decision_filename_identity",
            "decision H1 must contain the filename ID prefix",
        )]);
    };
    Ok((
        Some(id),
        Some(RecordSummary::Markdown {
            title: h1[0].clone(),
        }),
    ))
}

#[derive(Deserialize)]
struct Metadata {
    refs: Option<Vec<String>>,
    attachments: Option<Vec<String>>,
    status: Option<String>,
    decision: Option<String>,
}

fn request(path: &str, text: &str) -> Result<RecordSummary, Vec<Diagnostic>> {
    let (h1, h2) =
        casefile_core::markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
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

fn metadata_arrays(path: &str, text: &str) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>> {
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
fn metadata_value(text: &str, key: &str) -> Option<String> {
    let frontmatter = text.strip_prefix("---\n")?.split_once("\n---\n")?.0;
    let value: Metadata = serde_saphyr::from_str(frontmatter).ok()?;
    match key {
        "status" => value.status,
        "decision" => value.decision,
        _ => None,
    }
}

fn invalid(
    path: &str,
    kind: Option<Kind>,
    code: &str,
    message: &str,
) -> (
    Classification,
    Option<Kind>,
    Option<String>,
    Option<RecordSummary>,
    Vec<Diagnostic>,
) {
    (
        Classification::Invalid,
        kind,
        None,
        None,
        vec![Diagnostic::new(path, code, message)],
    )
}

fn kind_for_path(path: &str, active: &Activation) -> Option<Kind> {
    let (_, rest) = active.projects.iter().find_map(|(_, project)| {
        project.investigations.iter().find_map(|base| {
            path.strip_prefix(&(base.to_owned() + "/"))
                .map(|rest| (project, rest))
        })
    })?;
    let segments: Vec<_> = rest.split('/').collect();
    match segments.as_slice() {
        ["request.md"] => Some(Kind::Request),
        ["final-disposition.md"] => Some(Kind::Closeout),
        ["implementation-plan", "PLAN.md"] => Some(Kind::Plan),
        ["strategy", name]
            if matches!(
                *name,
                "investigation.toml" | "review.toml" | "implementation.toml"
            ) =>
        {
            Some(Kind::Strategy)
        }
        ["decision-log", name] if name.ends_with(".md") && name.contains('-') => {
            Some(Kind::Decision)
        }
        ["evidence", name] if name.ends_with(".md") => Some(Kind::Evidence),
        ["review", .., name] if name.ends_with(".md") => Some(Kind::Review),
        [
            "tickets" | "epics",
            "provisional" | "accepted" | "rejected",
            name,
        ] if name.ends_with(".md") => Some(if segments[0] == "tickets" {
            Kind::Ticket
        } else {
            Kind::Epic
        }),
        ["boards", name] if name.ends_with(".toml") => Some(Kind::Board),
        _ => None,
    }
}

fn in_active(path: &str, active: &Activation) -> bool {
    active
        .projects
        .values()
        .flat_map(|project| &project.investigations)
        .any(|base| path == base || path.starts_with(&(base.to_owned() + "/")))
}
fn cross_validate(entries: &[EntrySnapshot], active: &Activation) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut identities: BTreeMap<&str, &EntrySnapshot> = BTreeMap::new();
    let paths: BTreeSet<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
    let mut supersedes = BTreeMap::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.classification == Classification::Governed)
    {
        if let Some(identity) = entry.identity.as_deref() {
            if let Some(previous) = identities.insert(identity, entry) {
                diagnostics.push(Diagnostic::new(
                    &entry.path,
                    "duplicate_identity",
                    format!("identity also appears at {}", previous.path),
                ));
            }
        }
        if matches!(entry.kind, Some(Kind::Ticket | Kind::Epic | Kind::Board)) {
            if let Some(project) = active.projects.iter().find(|(_, project)| {
                project
                    .investigations
                    .iter()
                    .any(|base| entry.path.starts_with(&(base.to_owned() + "/")))
            }) {
                if !entry
                    .identity
                    .as_deref()
                    .is_some_and(|id| id.starts_with(&(project.1.prefix.clone() + "-")))
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "project_prefix",
                        "record identity must use the configured project prefix",
                    ));
                }
            }
        }
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.summary, Some(RecordSummary::WorkItem { .. })))
    {
        let RecordSummary::WorkItem { id, .. } =
            entry.summary.as_ref().expect("filtered work item")
        else {
            unreachable!()
        };
        let text = std::str::from_utf8(&entry.original_bytes).unwrap_or_default();
        if let Ok(draft) =
            casefile_core::parse_draft(&entry.path, entry.kind.expect("work kind"), text)
        {
            let item = match draft {
                RecordDraft::Ticket(item) | RecordDraft::Epic(item) => item,
                _ => unreachable!(),
            };
            let scope = scope_for(&entry.path, active);
            for reference in item
                .decision_refs
                .iter()
                .chain(item.related_tickets.iter())
                .chain(item.supersedes.iter())
                .chain(item.superseded_by.iter())
            {
                if reference == id
                    || identities
                        .get(reference.as_str())
                        .is_none_or(|target| scope_for(&target.path, active) != scope)
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "unresolved_reference",
                        "references must resolve within the governed project/investigation scope",
                    ));
                }
            }
            supersedes.insert(id.clone(), item.supersedes);
        }
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, Some(Kind::Evidence | Kind::Review)))
    {
        if let Ok(text) = std::str::from_utf8(&entry.original_bytes) {
            if let Ok((refs, attachments)) = metadata_arrays(&entry.path, text) {
                let scope = scope_for(&entry.path, active);
                for reference in refs {
                    if identities
                        .get(reference.as_str())
                        .is_none_or(|target| scope_for(&target.path, active) != scope)
                    {
                        diagnostics.push(Diagnostic::new(&entry.path, "unresolved_reference", "references must resolve within the governed project/investigation scope"));
                    }
                }
                for attachment in attachments {
                    let target = Path::new(&entry.path)
                        .parent()
                        .map(|parent| parent.join(&attachment))
                        .and_then(|path| path.to_str().map(str::to_owned));
                    if !target
                        .as_deref()
                        .is_some_and(|path| safe_relative(path) && paths.contains(path))
                    {
                        diagnostics.push(Diagnostic::new(
                            &entry.path,
                            "missing_attachment",
                            "attachments must be contained regular files",
                        ));
                    }
                }
            }
        }
    }
    for start in supersedes.keys() {
        if has_cycle(
            start,
            &supersedes,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        ) {
            diagnostics.push(Diagnostic::new(
                identities[start.as_str()].path.clone(),
                "supersession_cycle",
                "supersession references must not form a cycle",
            ));
        }
    }
    diagnostics
}

fn scope_for<'a>(path: &str, active: &'a Activation) -> Option<&'a str> {
    active
        .projects
        .values()
        .flat_map(|project| &project.investigations)
        .find_map(|base| {
            path.strip_prefix(&(base.to_owned() + "/"))
                .map(|_| base.as_str())
        })
}

fn has_cycle(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    checked: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(node.into()) {
        return true;
    }
    if checked.contains(node) {
        visiting.remove(node);
        return false;
    }
    let result = graph.get(node).is_some_and(|next| {
        next.iter()
            .any(|id| has_cycle(id, graph, visiting, checked))
    });
    visiting.remove(node);
    checked.insert(node.into());
    result
}

fn checked_path(path: &str) -> Result<String, StoreError> {
    if !safe_relative(path) {
        return Err(StoreError::Invalid(
            "path must be a contained relative path".into(),
        ));
    }
    Ok(path.into())
}
fn safe_relative(path: &str) -> bool {
    let value = Path::new(path);
    !value.is_absolute()
        && !path.is_empty()
        && value
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
fn relative(root: &Path, path: &Path) -> Result<String, StoreError> {
    path.strip_prefix(root)
        .map_err(|_| StoreError::Invalid("path escaped root".into()))
        .map(|path| {
            path.components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
}
fn digest(bytes: &[u8]) -> Revision {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Revision(format!("sha256:{}", hex(&hasher.finalize())))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn ensure_worktree(root: &Path) -> Result<(), StoreError> {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()?;
    if status.status.success() && String::from_utf8_lossy(&status.stdout).trim() == "true" {
        Ok(())
    } else {
        Err(StoreError::Invalid(
            "apply and preview require a real Git worktree".into(),
        ))
    }
}
fn git_diff(
    root: &Path,
    path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<String, StoreError> {
    let old = before.map(|bytes| temp(root, bytes)).transpose()?;
    let new = after.map(|bytes| temp(root, bytes)).transpose()?;
    let old_path = old
        .as_ref()
        .map(|file| file.path().as_os_str())
        .unwrap_or_else(|| OsStr::new("/dev/null"));
    let new_path = new
        .as_ref()
        .map(|file| file.path().as_os_str())
        .unwrap_or_else(|| OsStr::new("/dev/null"));
    let output = Command::new("git")
        .current_dir(root)
        .args(["diff", "--no-index", "--"])
        .arg(old_path)
        .arg(new_path)
        .output()?;
    if output.status.code().is_some_and(|code| code > 1) {
        return Err(StoreError::Invalid(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    Ok(canonical_diff(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        path,
        before.is_some(),
        after.is_some(),
    ))
}
fn canonical_diff(diff: &str, path: &str, before: bool, after: bool) -> String {
    diff.lines()
        .map(|line| {
            if line.starts_with("diff --git ") {
                format!("diff --git a/{path} b/{path}")
            } else if line.starts_with("--- ") {
                if before {
                    format!("--- a/{path}")
                } else {
                    "--- /dev/null".into()
                }
            } else if line.starts_with("+++ ") {
                if after {
                    format!("+++ b/{path}")
                } else {
                    "+++ /dev/null".into()
                }
            } else if line.starts_with("Binary files ") {
                let old = if before {
                    format!("a/{path}")
                } else {
                    "/dev/null".into()
                };
                let new = if after {
                    format!("b/{path}")
                } else {
                    "/dev/null".into()
                };
                format!("Binary files {old} and {new} differ")
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if diff.ends_with('\n') { "\n" } else { "" }
}
fn temp(root: &Path, bytes: &[u8]) -> Result<NamedTempFile, StoreError> {
    let mut file = NamedTempFile::new_in(root)?;
    file.write_all(bytes)?;
    Ok(file)
}
fn atomic_write(target: &Path, bytes: &[u8], create: bool) -> Result<(), StoreError> {
    let parent = target
        .parent()
        .ok_or_else(|| StoreError::Invalid("target has no parent".into()))?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    if create {
        temporary
            .persist_noclobber(target)
            .map_err(|error| StoreError::Io(error.error))?;
    } else {
        temporary
            .persist(target)
            .map_err(|error| StoreError::Io(error.error))?;
    }
    Ok(())
}
