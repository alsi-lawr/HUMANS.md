use casefile_core::{Classification, Diagnostic, EntrySnapshot, Kind, Revision, stable};
use serde::{Deserialize, Serialize};

use crate::{
    ActivationState, Store, StoreError,
    activation::{Activation, activation_content, scope_for},
    layout::{checked_path, kind_for_path},
    scanning::{
        InventoryKind, MetadataInventory, binding_diagnostics, classify, metadata_inventory,
        read_inventory_entry,
    },
    validation::{ValidationFacts, cross_validate_facts},
};

/// Public metadata-only inventory. File contents are never opened for this response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanSummary {
    pub revision: Revision,
    pub files: usize,
    pub regular_files: usize,
    pub symlinks: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub activation: ActivationState,
    pub valid: Option<bool>,
    pub revision: Revision,
    pub diagnostics: Vec<Diagnostic>,
}

impl Store {
    pub fn scan_summary(&self) -> Result<ScanSummary, StoreError> {
        let inventory = metadata_inventory(self.observation_root())?;
        Ok(ScanSummary {
            revision: inventory.revision,
            files: inventory.entries.len(),
            regular_files: inventory
                .entries
                .values()
                .filter(|entry| entry.kind == InventoryKind::Regular)
                .count(),
            symlinks: inventory
                .entries
                .values()
                .filter(|entry| entry.kind == InventoryKind::Symlink)
                .count(),
        })
    }

    /// Validates canonical records one body at a time; opaque attachments are metadata only.
    /// Scoped checks canonically read project work-item/decision/board support one record at a time,
    /// retaining only parsed facts, never unrelated evidence or opaque bodies.
    pub fn check(&self, investigation: Option<&str>) -> Result<CheckResult, StoreError> {
        let root = self.observation_root();
        let investigation = investigation.map(checked_path).transpose()?;
        let inventory = metadata_inventory(root)?;
        let bytes = inventory
            .entries
            .get("casefile.toml")
            .map(|entry| {
                if entry.kind == InventoryKind::Regular {
                    read_inventory_entry(entry)
                } else {
                    Ok(Vec::new())
                }
            })
            .transpose()?;
        let (activation, active, mut diagnostics) = activation_content(bytes.as_deref());
        if let Some(path) = &investigation {
            if activation != ActivationState::Active
                || !active
                    .projects
                    .values()
                    .any(|project| project.investigations.contains(path))
            {
                return Err(StoreError::Invalid("investigation is not activated".into()));
            }
        }
        if activation == ActivationState::Active {
            diagnostics.extend(check_scope(&inventory, &active, investigation.as_deref())?);
        }
        if metadata_inventory(root)?.revision != inventory.revision {
            return Err(StoreError::Invalid(
                "Store contents changed during check; retry the exact scope".into(),
            ));
        }
        let diagnostics = stable(diagnostics);
        let valid = match activation {
            ActivationState::Unactivated => None,
            ActivationState::Active => Some(diagnostics.is_empty()),
            ActivationState::Invalid => Some(false),
        };
        Ok(CheckResult {
            activation,
            valid,
            revision: inventory.revision,
            diagnostics,
        })
    }
}

fn check_scope(
    inventory: &MetadataInventory,
    active: &Activation,
    scope: Option<&str>,
) -> Result<Vec<Diagnostic>, StoreError> {
    let project_prefix = scope.map(|scope| {
        scope
            .split_once("/investigations/")
            .expect("validated investigation")
            .0
    });
    let mut entries = Vec::new();
    let mut facts = ValidationFacts::default();
    let mut diagnostics = Vec::new();
    let mut strategies = Vec::new();
    let mut strategy_scope = None;
    for (path, file) in &inventory.entries {
        let kind = if path == "projects.toml" {
            Some(Kind::ProjectMap)
        } else {
            kind_for_path(path, active)
        };
        let selected = scope.is_none_or(|scope| scope_for(path, active) == Some(scope));
        let support = project_prefix
            .is_some_and(|project| path.starts_with(&format!("{project}/")))
            && matches!(
                kind,
                Some(Kind::Ticket | Kind::Epic | Kind::Decision | Kind::Board)
            );
        let mut entry = EntrySnapshot {
            path: path.clone(),
            classification: Classification::Raw,
            kind: None,
            identity: None,
            summary: None,
            content_revision: file.revision.clone(),
            original_bytes: Vec::new(),
        };
        if file.kind != InventoryKind::Regular && selected {
            diagnostics.push(Diagnostic::new(
                path,
                "unsafe_path",
                "governed paths cannot be symlinks",
            ));
        }
        if kind.is_some() && (selected || support) {
            if file.kind == InventoryKind::Regular {
                entry.original_bytes = read_inventory_entry(file)?;
                #[cfg(test)]
                BODY_PEAK.with(|peak| {
                    peak.set(
                        peak.get().max(
                            entry.original_bytes.capacity()
                                + entries
                                    .iter()
                                    .map(|entry: &EntrySnapshot| entry.original_bytes.capacity())
                                    .sum::<usize>()
                                + strategies
                                    .iter()
                                    .map(|entry: &EntrySnapshot| entry.original_bytes.capacity())
                                    .sum::<usize>(),
                        ),
                    )
                });
                let (classification, kind, identity, summary, found) =
                    classify(path, &entry.original_bytes, active);
                entry.classification = classification;
                entry.kind = kind;
                entry.identity = identity;
                entry.summary = summary;
                if selected {
                    diagnostics.extend(found);
                }
                facts.insert(&entry);
            } else {
                entry.classification = Classification::Invalid;
                entry.kind = kind;
            }
        }
        if selected && matches!(kind, Some(Kind::Strategy | Kind::StrategyBinding)) {
            let current_scope = scope_for(path, active);
            if strategy_scope != current_scope {
                diagnostics.extend(binding_diagnostics(&strategies));
                strategies.clear();
                strategy_scope = current_scope;
            }
            strategies.push(entry.clone());
        }
        entry.original_bytes = Vec::new();
        if !matches!(kind, Some(Kind::Ticket | Kind::Epic | Kind::Progress)) {
            entry.summary = None;
        }
        // Unsafe attachments must not count as contained regular files.
        if file.kind == InventoryKind::Regular || entry.kind.is_some() {
            entries.push(entry);
        }
    }
    diagnostics.extend(
        cross_validate_facts(&entries, active, &facts)
            .into_iter()
            .filter(|diagnostic| {
                scope.is_none_or(|scope| scope_for(&diagnostic.path, active) == Some(scope))
            }),
    );
    diagnostics.extend(binding_diagnostics(&strategies));
    Ok(diagnostics)
}

#[cfg(test)]
thread_local! { static BODY_PEAK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
thread_local! { static OPENED: std::cell::RefCell<Option<Vec<std::path::PathBuf>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(super) fn observe_open(path: &std::path::Path) {
    OPENED.with(|paths| {
        if let Some(paths) = paths.borrow_mut().as_mut() {
            paths.push(path.into());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};
    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &to.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }
    #[test]
    fn scoped_check_never_opens_foreign_evidence_and_root_matches_canonical_validation() {
        let root = tempfile::tempdir().unwrap();
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
            root.path(),
        );
        let scope = "projects/demo/investigations/sample";
        let other = "projects/demo/investigations/other";
        let config = fs::read_to_string(root.path().join("casefile.toml"))
            .unwrap()
            .replace(
                &format!("investigations = [\"{scope}\"]"),
                &format!("investigations = [\"{scope}\", \"{other}\"]"),
            );
        fs::write(root.path().join("casefile.toml"), config).unwrap();
        fs::create_dir_all(root.path().join(format!("{other}/evidence"))).unwrap();
        let foreign = root.path().join(format!("{other}/evidence/large.md"));
        fs::write(
            &foreign,
            format!(
                "# Foreign evidence\n{}",
                "opaque sentinel ".repeat(1_000_000)
            ),
        )
        .unwrap();
        fs::create_dir_all(root.path().join(format!("{scope}/progress"))).unwrap();
        fs::write(
            root.path().join(format!("{scope}/progress/log.toml")),
            "schema_version = 1\n[[entries]]\nid = 'bad'\n",
        )
        .unwrap();
        let store = Store::open(root.path()).unwrap();
        OPENED.with(|paths| *paths.borrow_mut() = Some(Vec::new()));
        let checked = store.check(Some(scope)).unwrap();
        let opened = OPENED.with(|paths| paths.borrow_mut().take().unwrap());
        assert!(!opened.contains(&foreign));
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "invalid_progress_log")
        );
        let canonical = store.scan().unwrap().diagnostics;
        BODY_PEAK.with(|peak| peak.set(0));
        assert_eq!(canonical, store.check(None).unwrap().diagnostics);
        assert!(BODY_PEAK.with(|peak| peak.get()) < 32 * 1024 * 1024);
        OPENED.with(|paths| *paths.borrow_mut() = Some(Vec::new()));
        let scan = store.scan_summary().unwrap();
        assert!(scan.files > 0);
        assert!(
            OPENED
                .with(|paths| paths.borrow_mut().take().unwrap())
                .is_empty()
        );
    }
    #[test]
    fn root_check_retains_one_evidence_body_as_scope_count_grows() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        fs::write(
            root.path().join("projects.toml"),
            "[projects]\ndemo = '/source/demo'\n",
        )
        .unwrap();
        let body = format!("# Evidence\n{}", "x".repeat(4 * 1024 * 1024));
        let mut scopes = Vec::new();
        for count in 1..=8 {
            let scope = format!("projects/demo/investigations/scope-{count}");
            fs::create_dir_all(root.path().join(format!("{scope}/evidence"))).unwrap();
            fs::write(
                root.path().join(format!("{scope}/evidence/large.md")),
                &body,
            )
            .unwrap();
            scopes.push(format!("'{scope}'"));
            fs::write(
                root.path().join("casefile.toml"),
                format!(
                    "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = [{}]\n",
                    scopes.join(",")
                ),
            )
            .unwrap();
            BODY_PEAK.with(|peak| peak.set(0));
            assert_eq!(store.check(None).unwrap().valid, Some(true));
            let retained = BODY_PEAK.with(|peak| peak.get());
            assert!(retained >= body.len());
            assert!(
                retained < 8 * 1024 * 1024,
                "{count} scopes retained {retained} body bytes"
            );
        }
    }
    #[test]
    fn scoped_check_discards_each_canonical_support_body_before_opening_the_next() {
        let root = tempfile::tempdir().unwrap();
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
            root.path(),
        );
        let scope = "projects/demo/investigations/sample";
        let other = "projects/demo/investigations/other";
        let config = fs::read_to_string(root.path().join("casefile.toml"))
            .unwrap()
            .replace(
                &format!("investigations = [\"{scope}\"]"),
                &format!("investigations = [\"{scope}\", \"{other}\"]"),
            );
        fs::write(root.path().join("casefile.toml"), config).unwrap();
        let source = fs::read_to_string(
            root.path()
                .join(format!("{scope}/tickets/accepted/HMD-011.md")),
        )
        .unwrap();
        let mut support_paths = Vec::new();
        for index in 100..112 {
            let path = root
                .path()
                .join(format!("{other}/tickets/accepted/HMD-{index}.md"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                &path,
                source
                    .replace("HMD-011", &format!("HMD-{index}"))
                    .replace("investigation: \"sample\"", "investigation: \"other\"")
                    .replace("Required.", &"x".repeat(512 * 1024)),
            )
            .unwrap();
            support_paths.push(path);
        }
        let store = Store::open(root.path()).unwrap();
        OPENED.with(|paths| *paths.borrow_mut() = Some(Vec::new()));
        BODY_PEAK.with(|peak| peak.set(0));
        assert_eq!(store.check(Some(scope)).unwrap().valid, Some(true));
        let opened = OPENED.with(|paths| paths.borrow_mut().take().unwrap());
        assert!(
            support_paths
                .iter()
                .all(|path| opened.iter().filter(|opened| *opened == path).count() == 1)
        );
        assert!(BODY_PEAK.with(|peak| peak.get()) < 2 * 1024 * 1024);
    }
    #[test]
    fn parent_checks_and_provider_diagnostics_exclude_activated_nested_scope_bodies() {
        use crate::{InvestigationScope, Provider, ProviderQuery, ProviderQueryResult};

        let root = tempfile::tempdir().unwrap();
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
            root.path(),
        );
        let parent = "projects/demo/investigations/sample";
        let nested = "projects/demo/investigations/sample/nested";
        let config = fs::read_to_string(root.path().join("casefile.toml"))
            .unwrap()
            .replace(
                &format!("investigations = [\"{parent}\"]"),
                &format!("investigations = [\"{parent}\", \"{nested}\"]"),
            );
        fs::write(root.path().join("casefile.toml"), config).unwrap();
        let evidence = root.path().join(format!("{nested}/evidence/sentinel.md"));
        let progress = root.path().join(format!("{nested}/progress/log.toml"));
        let ticket = root
            .path()
            .join(format!("{nested}/tickets/accepted/HMD-012.md"));
        for path in [&evidence, &progress, &ticket] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        fs::write(
            &evidence,
            format!(
                "# Nested evidence\n{}",
                "nested sentinel ".repeat(128 * 1024)
            ),
        )
        .unwrap();
        fs::write(
            &progress,
            "schema_version = 1\n[[entries]]\nid = 'malformed'\n",
        )
        .unwrap();
        let source = fs::read_to_string(
            root.path()
                .join(format!("{parent}/tickets/accepted/HMD-011.md")),
        )
        .unwrap();
        fs::write(
            &ticket,
            source
                .replace("HMD-011", "HMD-012")
                .replace(
                    "investigation: \"sample\"",
                    "investigation: \"sample/nested\"",
                )
                .replace("related_tickets: []", "related_tickets: [HMD-404]"),
        )
        .unwrap();
        let store = Store::open(root.path()).unwrap();
        let provider = Provider::without_cache(Store::open(root.path()).unwrap());
        for (path, identity, is_parent) in
            [(parent, "sample", true), (nested, "sample/nested", false)]
        {
            for through_provider in [false, true] {
                OPENED.with(|paths| *paths.borrow_mut() = Some(Vec::new()));
                let diagnostics = if through_provider {
                    let result = provider
                        .query(ProviderQuery::Diagnostics {
                            scope: InvestigationScope {
                                project: "demo".into(),
                                investigation: identity.into(),
                            },
                        })
                        .unwrap();
                    let ProviderQueryResult::Diagnostics {
                        scope,
                        diagnostics,
                        total_count,
                        ..
                    } = result
                    else {
                        panic!("diagnostics")
                    };
                    assert_eq!(scope.investigation, identity);
                    assert_eq!(total_count, diagnostics.len());
                    diagnostics
                } else {
                    let result = store.check(Some(path)).unwrap();
                    assert_eq!(result.valid, Some(is_parent));
                    result.diagnostics
                };
                let opened = OPENED.with(|paths| paths.borrow_mut().take().unwrap());
                assert!(
                    opened.contains(&ticket),
                    "canonical same-project support remains available"
                );
                assert_eq!(opened.contains(&evidence), !is_parent);
                assert_eq!(opened.contains(&progress), !is_parent);
                if is_parent {
                    assert!(diagnostics.is_empty(), "{diagnostics:?}");
                } else {
                    assert!(diagnostics.iter().any(|diagnostic| diagnostic.path
                        == format!("{nested}/progress/log.toml")
                        && diagnostic.code == "invalid_progress_log"));
                    assert!(diagnostics.iter().any(|diagnostic| diagnostic.path
                        == format!("{nested}/tickets/accepted/HMD-012.md")
                        && diagnostic.code == "unresolved_reference"));
                }
            }
        }
    }
}
