use crate::{
    activation::{
        Activation, ActivationState, activation, activation_entry, investigation_identity,
    },
    layout::kind_for_path,
    store::StoreError,
    validation::cross_validate,
};
use casefile_core::{
    CasefileSnapshot, Classification, Diagnostic, EntrySnapshot, Kind, RecordDraft, RecordSummary,
    Revision, parse_decision, parse_metadata_arrays, parse_project_map, parse_request,
    parse_strategy, parse_strategy_binding, parse_strategy_projection, stable,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanResult {
    pub activation: ActivationState,
    pub investigation_roots: BTreeMap<String, Vec<String>>,
    pub snapshot: CasefileSnapshot,
    pub diagnostics: Vec<Diagnostic>,
}

impl ScanResult {
    pub fn scope_for_path<'a>(&'a self, path: &'a str) -> Option<(&'a str, Option<&'a str>)> {
        let (project, _) = path.strip_prefix("projects/")?.split_once('/')?;
        let investigation = self
            .investigation_roots
            .get(project)?
            .iter()
            .filter(|investigation| {
                path.starts_with(&format!(
                    "projects/{project}/investigations/{investigation}/"
                ))
            })
            .max_by_key(|investigation| investigation.len())
            .map(String::as_str);
        Some((project, investigation))
    }
}

pub(super) fn scan(
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
    diagnostics.extend(binding_diagnostics(&entries));
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
        investigation_roots: active
            .projects
            .iter()
            .map(|(project, value)| {
                (
                    project.clone(),
                    value
                        .investigations
                        .iter()
                        .filter_map(|path| investigation_identity(project, path).map(Into::into))
                        .collect(),
                )
            })
            .collect(),
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
        Kind::Request => parse_request(path, text).map(|summary| (None, Some(summary))),
        Kind::Decision => parse_decision(path, text),
        Kind::Evidence | Kind::Review => casefile_core::validate_markdown(path, text, &[], None)
            .and_then(|summary| parse_metadata_arrays(path, text).map(|_| summary))
            .map(|summary| (None, Some(summary))),
        Kind::Plan => casefile_core::validate_markdown(path, text, &["Objective"], None)
            .map(|summary| (None, Some(summary))),
        Kind::Closeout => {
            casefile_core::validate_markdown(path, text, &["Scope disposition"], None)
                .map(|summary| (None, Some(summary)))
        }
        Kind::Strategy => parse_strategy(path, text).map(|summary| (None, Some(summary))),
        Kind::StrategyBinding => {
            parse_strategy_binding(path, text).map(|summary| (None, Some(summary)))
        }
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
    let governed_projects = active
        .projects
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    match parse_project_map(path, bytes, &governed_projects) {
        Ok(summary) => (
            Classification::Governed,
            Some(Kind::ProjectMap),
            None,
            Some(summary),
            Vec::new(),
        ),
        Err(diagnostics) => (
            Classification::Invalid,
            Some(Kind::ProjectMap),
            None,
            None,
            diagnostics,
        ),
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

fn in_active(path: &str, active: &Activation) -> bool {
    active
        .projects
        .values()
        .flat_map(|project| &project.investigations)
        .any(|base| path == base || path.starts_with(&(base.to_owned() + "/")))
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

fn binding_diagnostics(entries: &[EntrySnapshot]) -> Vec<Diagnostic> {
    let scope = |entry: &EntrySnapshot| {
        entry
            .path
            .rsplit_once("/strategy/")
            .map(|(root, _)| root.to_owned())
    };
    let mut diagnostics = Vec::new();
    for binding in entries
        .iter()
        .filter(|entry| entry.kind == Some(Kind::StrategyBinding))
    {
        let Some(RecordSummary::StrategyBinding {
            binding: binding_value,
        }) = &binding.summary
        else {
            continue;
        };
        let binding_scope = scope(binding);
        let implementation = entries.iter().find(|entry| {
            entry.classification == Classification::Governed && scope(entry) == binding_scope
                && matches!(&entry.summary, Some(RecordSummary::Strategy { phase, .. }) if phase == "implementation")
        });
        let Some(implementation) = implementation else {
            continue;
        };
        let Some(RecordSummary::Strategy { adapter, .. }) = &implementation.summary else {
            continue;
        };
        if binding_value.adapter != *adapter {
            diagnostics.push(
                Diagnostic::new(
                    &binding.path,
                    "binding_adapter",
                    "binding adapter does not match implementation strategy",
                )
                .field("adapter"),
            );
            continue;
        }
        let Ok(text) = std::str::from_utf8(&implementation.original_bytes) else {
            continue;
        };
        let Ok(Some(projection)) = parse_strategy_projection(&implementation.path, text) else {
            diagnostics.push(
                Diagnostic::new(
                    &binding.path,
                    "binding_writer_match",
                    "implementation strategy has no graphable implementation-writer match",
                )
                .field("role"),
            );
            continue;
        };
        if projection
            .workers
            .iter()
            .filter(|worker| worker.role == "implementation-writer")
            .count()
            != 1
        {
            diagnostics.push(
                Diagnostic::new(
                    &binding.path,
                    "binding_writer_match",
                    "implementation strategy must declare exactly one implementation-writer",
                )
                .field("role"),
            );
        }
    }
    diagnostics
}
