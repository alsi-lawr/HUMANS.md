use crate::{
    activation::{
        Activation, ActivationState, activation_content, activation_entry, investigation_identity,
    },
    revision::{metadata_revision, open_file_revision, store_revision, synthetic_revision},
    store::StoreError,
    validation::{ValidationFacts, cross_validate_facts},
};
use casefile_core::{
    CasefileSnapshot, Classification, Diagnostic, EntrySnapshot, Kind, ProjectMap, RecordDraft,
    RecordSummary, Revision, parse_decision, parse_metadata_arrays, parse_progress_log,
    parse_project_map, parse_project_map_values, parse_request, parse_strategy_binding, stable,
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanResult {
    pub activation: ActivationState,
    pub investigation_roots: BTreeMap<String, Vec<String>>,
    pub snapshot: CasefileSnapshot,
    pub diagnostics: Vec<Diagnostic>,
}

impl ScanResult {
    pub fn scope_for_path<'a>(&'a self, path: &'a str) -> Option<(&'a str, Option<&'a str>)> {
        let (project, relative) = path.strip_prefix("projects/")?.split_once('/')?;
        let roots = self.investigation_roots.get(project)?;
        let investigation = relative
            .strip_prefix("investigations/")
            .and_then(|relative| {
                roots
                    .iter()
                    .filter(|investigation| {
                        relative
                            .strip_prefix(investigation.as_str())
                            .is_some_and(|rest| rest.starts_with('/'))
                    })
                    .max_by_key(|investigation| investigation.len())
                    .map(String::as_str)
            });
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
    Ok(scan_with_facts(root, overlay, false)?.0)
}

pub(super) fn scan_for_derivation(
    root: &Path,
) -> Result<(ScanResult, BTreeMap<String, classification::ParsedFacts>), StoreError> {
    scan_with_facts(root, &BTreeMap::new(), true)
}

fn scan_with_facts(
    root: &Path,
    overlay: &BTreeMap<String, Option<Vec<u8>>>,
    retain_facts: bool,
) -> Result<(ScanResult, BTreeMap<String, classification::ParsedFacts>), StoreError> {
    let inventory = metadata_inventory(root)?;
    let mut files = inventory
        .entries
        .iter()
        .map(|(path, entry)| {
            let bytes = if entry.kind == InventoryKind::Regular {
                read_observed_entry(root, path, entry)?
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
    if files
        .get("casefile.toml")
        .is_some_and(|file| file.unsafe_path)
    {
        diagnostics = vec![Diagnostic::new(
            "casefile.toml",
            "unsafe_path",
            "activation must be a regular non-symlink file",
        )];
    }
    let mut entries = Vec::new();
    let mut parsed_facts = BTreeMap::new();
    let scopes = crate::activation::ScopeIndex::new(&active);
    let mut facts = ValidationFacts::default();
    let mut nonregular_paths = std::collections::BTreeSet::new();
    for (path, file) in files {
        if file.unsafe_path {
            nonregular_paths.insert(path.clone());
        }
        let bytes = file.bytes;
        let resolved = scopes.resolve(&path);
        let parsed = if path == "casefile.toml" {
            classification::Classified {
                classification: (
                    if activation == ActivationState::Active {
                        Classification::Governed
                    } else {
                        Classification::Invalid
                    },
                    Some(Kind::Activation),
                    None,
                    (activation == ActivationState::Active).then(|| RecordSummary::Activation {
                        projects: active.projects.keys().cloned().collect(),
                    }),
                    Vec::new(),
                ),
                facts: classification::ParsedFacts::default(),
            }
        } else if activation == ActivationState::Unactivated {
            classification::Classified {
                classification: (Classification::Ungoverned, None, None, None, Vec::new()),
                facts: classification::ParsedFacts::default(),
            }
        } else if resolved
            .scope
            .is_some_and(|scope| crate::layout::scope_container(&path, scope))
        {
            classification::Classified {
                classification: invalid(
                    &path,
                    None,
                    "unsafe_path",
                    "governed container must be a non-symlink directory",
                ),
                facts: classification::ParsedFacts::default(),
            }
        } else if file.unsafe_path && resolved.kind.is_some() {
            classification::Classified {
                classification: invalid(
                    &path,
                    resolved.kind,
                    "unsafe_path",
                    "governed paths must be regular non-symlink files",
                ),
                facts: classification::ParsedFacts::default(),
            }
        } else {
            classify_facts(&path, &bytes, &active, resolved.kind)
        };
        let (classification, kind, identity, summary, mut found) = parsed.classification;
        diagnostics.append(&mut found);
        let entry = EntrySnapshot {
            path,
            classification,
            kind,
            identity,
            content_revision: file.revision,
            summary,
            original_bytes: bytes,
        };
        facts.insert(&entry, resolved, &parsed.facts);
        if retain_facts {
            parsed_facts.insert(entry.path.clone(), parsed.facts);
        }
        entries.push(entry);
    }
    diagnostics.extend(cross_validate_facts(&entries, &active, &facts, |path| {
        !nonregular_paths.contains(path)
    }));
    diagnostics.extend(binding_diagnostics_facts(&entries, &facts));
    require_inventory_unchanged(root, &inventory)?;
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
    Ok((
        ScanResult {
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
                            .filter_map(|path| {
                                investigation_identity(project, path).map(Into::into)
                            })
                            .collect(),
                    )
                })
                .collect(),
            snapshot: CasefileSnapshot { revision, entries },
            diagnostics: stable(diagnostics),
        },
        parsed_facts,
    ))
}

struct CollectedFile {
    bytes: Vec<u8>,
    revision: Revision,
    unsafe_path: bool,
}

pub(super) struct CatalogueBaseline {
    pub(super) revision: Revision,
    pub(super) activation: ActivationState,
    pub(super) active: Activation,
    pub(super) projects: ProjectMap,
    pub(super) diagnostics: Vec<Diagnostic>,
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

mod inventory;
pub(super) use inventory::{
    InventoryEntry, InventoryKind, MetadataInventory, collect_selected, inventory_target,
    metadata_inventory, read_inventory_entry, read_observed_entry, require_inventory_unchanged,
};
mod scoped;
pub(super) use scoped::{ScopedRead, scoped_detail_scan, scoped_scan};
pub(super) mod classification;
pub(super) mod selected;
pub(super) use classification::{classify_facts, invalid};

fn in_active(path: &str, active: &Activation) -> bool {
    active
        .projects
        .values()
        .flat_map(|project| &project.investigations)
        .any(|base| crate::activation::contains_path(base, path))
}

pub(super) fn binding_diagnostics_facts(
    entries: &[EntrySnapshot],
    facts: &ValidationFacts<'_>,
) -> Vec<Diagnostic> {
    binding_diagnostics_with(entries, |entry| {
        facts
            .strategies
            .get(&entry.path)
            .and_then(Option::as_ref)
            .map(|projection| {
                projection
                    .workers
                    .iter()
                    .filter(|worker| worker.role == "implementation-writer")
                    .count()
            })
    })
}

fn binding_diagnostics_with(
    entries: &[EntrySnapshot],
    writer_count: impl Fn(&EntrySnapshot) -> Option<usize>,
) -> Vec<Diagnostic> {
    let mut implementations = BTreeMap::new();
    for entry in entries.iter().filter(|entry| entry.classification == Classification::Governed && matches!(&entry.summary, Some(RecordSummary::Strategy {phase, ..}) if phase == "implementation")) {
        if let Some((scope, _)) = entry.path.rsplit_once("/strategy/") { implementations.insert(scope, entry); }
    }
    let mut diagnostics = Vec::new();
    for binding in entries
        .iter()
        .filter(|entry| entry.kind == Some(Kind::StrategyBinding))
    {
        let Some(RecordSummary::StrategyBinding { binding: value }) = &binding.summary else {
            continue;
        };
        let Some((scope, _)) = binding.path.rsplit_once("/strategy/") else {
            continue;
        };
        let Some(implementation) = implementations.get(scope) else {
            continue;
        };
        let Some(RecordSummary::Strategy { adapter, .. }) = &implementation.summary else {
            continue;
        };
        if value.adapter != *adapter {
            diagnostics.push(
                Diagnostic::new(
                    &binding.path,
                    "binding_adapter",
                    "binding adapter does not match implementation strategy",
                )
                .field("adapter"),
            );
        } else if writer_count(implementation) != Some(1) {
            diagnostics.push(Diagnostic::new(&binding.path, "binding_writer_match", "implementation strategy must declare exactly one graphable implementation-writer").field("role"));
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

    #[cfg(unix)]
    #[test]
    fn attachment_overlay_uses_proposed_regular_presence_not_original_opaque_membership() {
        let root = TempDir::new().unwrap();
        let scope = "projects/demo/investigations/sample";
        let evidence = root.path().join(format!("{scope}/evidence"));
        fs::create_dir_all(&evidence).unwrap();
        fs::write(root.path().join("casefile.toml"), format!("schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['{scope}']\n")).unwrap();
        fs::write(
            evidence.join("ref.md"),
            "---\nattachments: [payload.bin]\n---\n# Attachment evidence\n",
        )
        .unwrap();
        let outside = TempDir::new().unwrap();
        fs::write(outside.path().join("secret"), "do not follow").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), evidence.join("payload.bin"))
            .unwrap();
        let path = format!("{scope}/evidence/payload.bin");
        assert_eq!(
            scan(root.path(), &BTreeMap::new()).unwrap().diagnostics[0].code,
            "missing_attachment"
        );
        let replacement = BTreeMap::from([(path.clone(), Some(Vec::new()))]);
        assert!(
            scan(root.path(), &replacement)
                .unwrap()
                .diagnostics
                .is_empty()
        );
        let removal = BTreeMap::from([(path, None)]);
        assert_eq!(
            scan(root.path(), &removal).unwrap().diagnostics[0].code,
            "missing_attachment"
        );
        assert!(
            fs::symlink_metadata(evidence.join("payload.bin"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn selected_descriptor_and_global_membership_observations_refuse_races() {
        let selected = TempDir::new().expect("selected root");
        fs::write(selected.path().join("selected.txt"), "before").expect("selected");
        let inventory = metadata_inventory(selected.path()).expect("inventory");
        fs::write(selected.path().join("selected.txt"), "after, longer").expect("selected race");
        assert!(matches!(
            read_inventory_entry(inventory.entries.get("selected.txt").expect("entry")),
            Err(StoreError::Invalid(_))
        ));

        let tree = TempDir::new().expect("tree root");
        fs::write(tree.path().join("selected.txt"), "stable").expect("selected");
        let inventory = metadata_inventory(tree.path()).expect("inventory");
        fs::write(tree.path().join("unrelated.txt"), "appeared").expect("tree race");
        assert!(matches!(
            require_inventory_unchanged(tree.path(), &inventory),
            Err(StoreError::Invalid(_))
        ));
        #[cfg(unix)]
        {
            let tree = TempDir::new().unwrap();
            fs::create_dir(tree.path().join("empty")).unwrap();
            let inventory = metadata_inventory(tree.path()).unwrap();
            let outside = TempDir::new().unwrap();
            fs::remove_dir(tree.path().join("empty")).unwrap();
            std::os::unix::fs::symlink(outside.path(), tree.path().join("empty")).unwrap();
            assert!(require_inventory_unchanged(tree.path(), &inventory).is_err());
        }
    }
}
