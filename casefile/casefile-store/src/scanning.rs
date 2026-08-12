use crate::{
    activation::{
        Activation, ActivationState, activation_content, activation_entry, investigation_identity,
    },
    layout::kind_for_path,
    revision::{metadata_revision, open_file_revision, store_revision, synthetic_revision},
    store::StoreError,
    validation::cross_validate,
};
use casefile_core::{
    CasefileSnapshot, Classification, Diagnostic, EntrySnapshot, Kind, ProjectMap, RecordDraft,
    RecordSummary, Revision, parse_decision, parse_metadata_arrays, parse_progress_log,
    parse_project_map, parse_project_map_values, parse_request, parse_strategy,
    parse_strategy_binding, parse_strategy_projection, parse_strategy_transition, stable,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
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

/// Returns whether a root-relative filesystem path is outside the canonical Store input.
///
/// Only direct-root `.git` and `.agent-workspace` trees are excluded. A same-named component
/// anywhere below another root-relative component remains visible to the Store.
pub fn is_store_path_excluded(root_relative: &Path) -> bool {
    matches!(
        root_relative.components().next(),
        Some(Component::Normal(component))
            if component == OsStr::new(".git") || component == OsStr::new(".agent-workspace")
    )
}

pub(super) fn scan(
    root: &Path,
    overlay: &BTreeMap<String, Option<Vec<u8>>>,
) -> Result<ScanResult, StoreError> {
    let inventory = metadata_inventory(root)?;
    let mut files = inventory
        .entries
        .iter()
        .map(|(path, entry)| {
            let bytes = if entry.kind == InventoryKind::Regular {
                read_inventory_entry(entry)?
            } else {
                Vec::new()
            };
            Ok((
                path.clone(),
                CollectedFile {
                    bytes,
                    revision: entry.revision.clone(),
                    unsafe_path: entry.kind != InventoryKind::Regular,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
    for (path, bytes) in overlay {
        if is_store_path_excluded(Path::new(path)) {
            continue;
        }
        match bytes {
            Some(bytes) => {
                files.insert(
                    path.clone(),
                    CollectedFile {
                        bytes: bytes.clone(),
                        revision: synthetic_revision(path, true),
                        unsafe_path: false,
                    },
                );
            }
            None => {
                files.remove(path);
            }
        }
    }
    let (activation, active, mut diagnostics) =
        activation_content(files.get("casefile.toml").map(|file| file.bytes.as_slice()));
    let mut entries = Vec::new();
    for (path, file) in files {
        let bytes = file.bytes;
        let (classification, kind, identity, summary, mut found) =
            if activation == ActivationState::Unactivated {
                (Classification::Ungoverned, None, None, None, Vec::new())
            } else if file.unsafe_path {
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
            content_revision: file.revision,
            summary,
            original_bytes: bytes,
        });
    }
    diagnostics.extend(cross_validate(&entries, &active));
    diagnostics.extend(binding_diagnostics(&entries));
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let verified_inventory = metadata_inventory(root)?;
    if verified_inventory.revision != inventory.revision {
        return Err(StoreError::Invalid(
            "Store contents changed while they were scanned".into(),
        ));
    }
    let revision = if overlay.is_empty() {
        inventory.revision
    } else {
        store_revision(
            entries
                .iter()
                .map(|entry| (entry.path.as_str(), &entry.content_revision)),
            true,
        )
    };
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
        snapshot: CasefileSnapshot { revision, entries },
        diagnostics: stable(diagnostics),
    })
}

struct CollectedFile {
    bytes: Vec<u8>,
    revision: Revision,
    unsafe_path: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InventoryKind {
    Regular,
    Symlink,
    Other,
}

pub(super) struct InventoryEntry {
    pub(super) path: PathBuf,
    pub(super) kind: InventoryKind,
    pub(super) revision: Revision,
}

pub(super) struct MetadataInventory {
    pub(super) entries: BTreeMap<String, InventoryEntry>,
    pub(super) revision: Revision,
}

pub(super) struct CatalogueBaseline {
    pub(super) revision: Revision,
    pub(super) activation: ActivationState,
    pub(super) active: Activation,
    pub(super) projects: ProjectMap,
    pub(super) diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy)]
pub(super) enum ScopedRead {
    RecordIndex,
    Boards,
    StrategyTransitions,
}

pub(super) fn metadata_inventory(root: &Path) -> Result<MetadataInventory, StoreError> {
    let mut entries = BTreeMap::new();
    collect_inventory(root, root, &mut entries)?;
    let revision = store_revision(
        entries
            .iter()
            .map(|(path, entry)| (path.as_str(), &entry.revision)),
        false,
    );
    Ok(MetadataInventory { entries, revision })
}

fn collect_inventory(
    root: &Path,
    directory: &Path,
    entries: &mut BTreeMap<String, InventoryEntry>,
) -> Result<(), StoreError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = relative(root, &path)?;
        if is_store_path_excluded(Path::new(&relative)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        let revision = metadata_revision(&path, &metadata)?;
        if metadata.file_type().is_symlink() {
            entries.insert(
                relative,
                InventoryEntry {
                    path,
                    kind: InventoryKind::Symlink,
                    revision,
                },
            );
            continue;
        }
        if metadata.is_dir() {
            collect_inventory(root, &path, entries)?;
        } else if metadata.is_file() {
            entries.insert(
                relative,
                InventoryEntry {
                    path,
                    kind: InventoryKind::Regular,
                    revision,
                },
            );
        } else {
            entries.insert(
                relative,
                InventoryEntry {
                    path,
                    kind: InventoryKind::Other,
                    revision,
                },
            );
        }
    }
    Ok(())
}

pub(super) fn read_inventory_entry(entry: &InventoryEntry) -> Result<Vec<u8>, StoreError> {
    let mut file = File::open(&entry.path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(StoreError::Invalid(
            "Store content changed before it was opened".into(),
        ));
    }
    let opened_revision = open_file_revision(&file, &opened_metadata)?;
    if opened_revision != entry.revision {
        return Err(StoreError::Invalid(
            "Store content changed before it was read".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let after_open_revision = open_file_revision(&file, &file.metadata()?)?;
    let after_path_metadata = fs::symlink_metadata(&entry.path)?;
    let after_path_revision = metadata_revision(&entry.path, &after_path_metadata)?;
    if opened_revision != after_open_revision || opened_revision != after_path_revision {
        return Err(StoreError::Invalid(
            "Store content changed while it was read".into(),
        ));
    }
    Ok(bytes)
}

pub(super) fn catalogue_baseline(root: &Path) -> Result<CatalogueBaseline, StoreError> {
    let inventory = metadata_inventory(root)?;
    let activation_bytes = read_optional_regular(&inventory, "casefile.toml")?;
    let projects_bytes = read_optional_regular(&inventory, "projects.toml")?;
    let (activation, active, mut diagnostics) = activation_content(activation_bytes.as_deref());
    let projects = match projects_bytes {
        Some(bytes) => match parse_project_map_values("projects.toml", &bytes) {
            Ok(projects) => projects,
            Err(mut found) => {
                diagnostics.append(&mut found);
                ProjectMap::new()
            }
        },
        None => {
            diagnostics.push(Diagnostic::new(
                "projects.toml",
                "missing_project_map",
                "projects.toml is required for project source-root mappings",
            ));
            ProjectMap::new()
        }
    };
    for project in active.projects.keys() {
        if !projects.contains_key(project) {
            diagnostics.push(
                Diagnostic::new(
                    "projects.toml",
                    "missing_governed_project",
                    "projects.toml must map every governed project",
                )
                .field(project),
            );
        }
    }
    require_inventory_unchanged(root, &inventory)?;
    Ok(CatalogueBaseline {
        revision: inventory.revision,
        activation,
        active,
        projects,
        diagnostics: stable(diagnostics),
    })
}

pub(super) fn scoped_scan(
    root: &Path,
    project: &str,
    investigation: &str,
    read: ScopedRead,
) -> Result<(Revision, String, ScanResult), StoreError> {
    let inventory = metadata_inventory(root)?;
    let activation_bytes = read_optional_regular(&inventory, "casefile.toml")?;
    let (activation, active, mut diagnostics) = activation_content(activation_bytes.as_deref());
    if activation != ActivationState::Active {
        return Err(StoreError::Invalid(
            "investigation reads require active Casefile configuration".into(),
        ));
    }
    let path = active
        .projects
        .get(project)
        .into_iter()
        .flat_map(|configuration| &configuration.investigations)
        .filter(|path| investigation_identity(project, path) == Some(investigation))
        .cloned()
        .collect::<Vec<_>>();
    let [path] = path.as_slice() else {
        return Err(StoreError::Invalid(
            "investigation scope must resolve to exactly one activated path".into(),
        ));
    };
    let prefix = format!("{path}/");
    let selected = inventory
        .entries
        .iter()
        .filter_map(|(relative, entry)| {
            let kind = kind_for_path(relative, &active)?;
            let included = match read {
                ScopedRead::RecordIndex => {
                    matches!(kind, Kind::Ticket | Kind::Epic | Kind::Progress)
                }
                ScopedRead::Boards => {
                    matches!(
                        kind,
                        Kind::Ticket | Kind::Epic | Kind::Progress | Kind::Board
                    )
                }
                ScopedRead::StrategyTransitions => kind == Kind::StrategyTransition,
            };
            (relative.starts_with(&prefix) && included).then_some((relative, entry, kind))
        })
        .collect::<Vec<_>>();
    let mut entries = Vec::new();
    for (relative, inventory_entry, expected_kind) in selected {
        let bytes = if inventory_entry.kind == InventoryKind::Regular {
            read_inventory_entry(inventory_entry)?
        } else {
            Vec::new()
        };
        let (classification, kind, identity, summary, mut found) =
            if inventory_entry.kind == InventoryKind::Regular {
                classify(relative, &bytes, &active)
            } else {
                invalid(
                    relative,
                    Some(expected_kind),
                    "unsafe_path",
                    "governed paths must be regular non-symlink files",
                )
            };
        diagnostics.append(&mut found);
        entries.push(EntrySnapshot {
            path: relative.clone(),
            classification,
            kind,
            identity,
            content_revision: inventory_entry.revision.clone(),
            summary,
            original_bytes: bytes,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    diagnostics.extend(binding_diagnostics(&entries));
    require_inventory_unchanged(root, &inventory)?;
    let revision = inventory.revision;
    Ok((
        revision.clone(),
        path.clone(),
        ScanResult {
            activation,
            investigation_roots: BTreeMap::from([(project.into(), vec![investigation.into()])]),
            snapshot: CasefileSnapshot { revision, entries },
            diagnostics: stable(diagnostics),
        },
    ))
}

pub(super) fn scoped_detail_scan(
    root: &Path,
    project: &str,
    investigation: &str,
    identity: &str,
) -> Result<(Revision, String, ScanResult), StoreError> {
    let inventory = metadata_inventory(root)?;
    let activation_bytes = read_optional_regular(&inventory, "casefile.toml")?;
    let (activation, active, mut diagnostics) = activation_content(activation_bytes.as_deref());
    if activation != ActivationState::Active {
        return Err(StoreError::Invalid(
            "record detail requires active Casefile configuration".into(),
        ));
    }
    let path = resolve_investigation_path(&active, project, investigation)?;
    let progress_path = format!("{path}/progress/log.toml");
    let prefixes = [format!("{path}/tickets/"), format!("{path}/epics/")];
    let file_name = format!("/{identity}.md");
    let candidates = inventory
        .entries
        .iter()
        .filter(|(relative, _)| {
            **relative == progress_path
                || (relative.ends_with(&file_name)
                    && prefixes.iter().any(|prefix| relative.starts_with(prefix)))
        })
        .collect::<Vec<_>>();
    let mut entries = Vec::new();
    for (relative, inventory_entry) in candidates {
        let bytes = if inventory_entry.kind == InventoryKind::Regular {
            read_inventory_entry(inventory_entry)?
        } else {
            Vec::new()
        };
        let expected_kind = kind_for_path(relative, &active);
        let (classification, kind, found_identity, summary, mut found) =
            if inventory_entry.kind == InventoryKind::Regular {
                classify(relative, &bytes, &active)
            } else {
                invalid(
                    relative,
                    expected_kind,
                    "unsafe_path",
                    "governed paths must be regular non-symlink files",
                )
            };
        diagnostics.append(&mut found);
        if kind == Some(Kind::Progress) || found_identity.as_deref() == Some(identity) {
            entries.push(EntrySnapshot {
                path: relative.clone(),
                classification,
                kind,
                identity: found_identity,
                content_revision: inventory_entry.revision.clone(),
                summary,
                original_bytes: bytes,
            });
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    require_inventory_unchanged(root, &inventory)?;
    let revision = inventory.revision;
    Ok((
        revision.clone(),
        path,
        ScanResult {
            activation,
            investigation_roots: BTreeMap::from([(project.into(), vec![investigation.into()])]),
            snapshot: CasefileSnapshot { revision, entries },
            diagnostics: stable(diagnostics),
        },
    ))
}

fn resolve_investigation_path(
    active: &Activation,
    project: &str,
    investigation: &str,
) -> Result<String, StoreError> {
    let paths = active
        .projects
        .get(project)
        .into_iter()
        .flat_map(|configuration| &configuration.investigations)
        .filter(|path| investigation_identity(project, path) == Some(investigation))
        .cloned()
        .collect::<Vec<_>>();
    let [path] = paths.as_slice() else {
        return Err(StoreError::Invalid(
            "investigation scope must resolve to exactly one activated path".into(),
        ));
    };
    Ok(path.clone())
}

fn read_optional_regular(
    inventory: &MetadataInventory,
    path: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    match inventory.entries.get(path) {
        Some(entry) if entry.kind == InventoryKind::Regular => {
            read_inventory_entry(entry).map(Some)
        }
        Some(_) => Err(StoreError::Invalid(format!(
            "{path} must be a regular non-symlink file"
        ))),
        None => Ok(None),
    }
}

fn require_inventory_unchanged(
    root: &Path,
    baseline: &MetadataInventory,
) -> Result<(), StoreError> {
    if metadata_inventory(root)?.revision == baseline.revision {
        Ok(())
    } else {
        Err(StoreError::Invalid(
            "Store contents changed during selective read".into(),
        ))
    }
}

pub(super) fn classify(
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
    if kind == Kind::StrategyTransition && is_legacy_strategy_transition(text) {
        return (Classification::Raw, None, None, None, Vec::new());
    }
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
        Kind::StrategyTransition => parse_strategy_transition(path, text).map(|record| {
            (
                Some(format!(
                    "strategy-transition:{path}:{}",
                    record.operation_id
                )),
                Some(RecordSummary::StrategyTransition {
                    record: Box::new(record),
                }),
            )
        }),
        Kind::Progress => {
            parse_progress_log(path, text).map(|_| (None, Some(RecordSummary::Progress)))
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

fn is_legacy_strategy_transition(text: &str) -> bool {
    let Ok(value) = toml::from_str::<toml::Value>(text) else {
        return false;
    };
    value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        == Some(1)
        && value.get("operation_id").is_none()
        && value
            .get("timestamp")
            .and_then(toml::Value::as_str)
            .is_some()
        && value.get("mode").and_then(toml::Value::as_str).is_some()
        && value
            .get("selected_matrix")
            .and_then(toml::Value::as_str)
            .is_some()
        && value
            .get("backup_path")
            .and_then(toml::Value::as_str)
            .is_some()
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

pub(super) fn binding_diagnostics(entries: &[EntrySnapshot]) -> Vec<Diagnostic> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn store_path_exclusion_is_exactly_direct_root_implementation_metadata() {
        for path in [
            ".git",
            ".git/config",
            ".git/objects/ab/cd",
            ".agent-workspace",
            ".agent-workspace/session/log.txt",
        ] {
            assert!(
                is_store_path_excluded(Path::new(path)),
                "unexpectedly included {path:?}"
            );
        }
        for path in [
            "git/config",
            ".gitignore",
            ".github/workflows/ci.yml",
            "agent-workspace/session/log.txt",
            ".agent-workspace-old/session/log.txt",
            "projects/demo/.git/config",
            "projects/demo/.agent-workspace/session/log.txt",
            "projects/.git",
            "projects/.agent-workspace",
        ] {
            assert!(
                !is_store_path_excluded(Path::new(path)),
                "unexpectedly excluded {path:?}"
            );
        }
    }

    #[test]
    fn selected_read_and_second_inventory_barriers_refuse_races() {
        let selected = TempDir::new().expect("selected root");
        fs::write(selected.path().join("selected.txt"), "before").expect("selected");
        let inventory = metadata_inventory(selected.path()).expect("inventory");
        fs::write(selected.path().join("selected.txt"), "after!").expect("selected race");
        assert!(matches!(
            read_inventory_entry(inventory.entries.get("selected.txt").expect("entry")),
            Err(StoreError::Invalid(message)) if message.contains("changed before")
        ));

        let tree = TempDir::new().expect("tree root");
        fs::write(tree.path().join("selected.txt"), "stable").expect("selected");
        let inventory = metadata_inventory(tree.path()).expect("inventory");
        fs::write(tree.path().join("unrelated.txt"), "appeared").expect("tree race");
        assert!(matches!(
            require_inventory_unchanged(tree.path(), &inventory),
            Err(StoreError::Invalid(message)) if message.contains("changed during selective read")
        ));
    }
}
