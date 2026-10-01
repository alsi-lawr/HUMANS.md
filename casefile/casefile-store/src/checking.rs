use casefile_core::{Classification, Diagnostic, EntrySnapshot, Kind, Revision, stable};
use serde::{Deserialize, Serialize};

use crate::{
    ActivationState, Store, StoreError,
    activation::{Activation, activation_content},
    layout::checked_path,
    scanning::{InventoryKind, metadata_inventory},
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
    pub freshness: crate::CheckFreshness,
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
    /// Scoped checks canonically read project work-item/decision support one record at a time,
    /// retaining only parsed facts, never unrelated evidence or opaque bodies.
    pub fn check(&self, investigation: Option<&str>) -> Result<CheckResult, StoreError> {
        let root = self.observation_root();
        if let Some(path) = investigation.map(checked_path).transpose()? {
            let (project_path, identity) = path
                .split_once("/investigations/")
                .ok_or_else(|| StoreError::Invalid("investigation is not activated".into()))?;
            let project = project_path
                .strip_prefix("projects/")
                .ok_or_else(|| StoreError::Invalid("investigation is not activated".into()))?;
            let mut observation = crate::read_context::ScopeObservation::begin(
                root,
                crate::ScopeReadTarget::Diagnostics {
                    scope: crate::InvestigationScope {
                        project: project.into(),
                        investigation: identity.into(),
                    },
                },
            )?;
            let mut diagnostics = observation.mapping_diagnostics.clone();
            let (found, attachments) = check_scope(
                root,
                observation
                    .entries
                    .iter()
                    .chain(observation.support.iter())
                    .map(|(path, entry)| (path, entry)),
                &observation.active,
                Some(&observation.path),
            )?;
            diagnostics.extend(found);
            observation.attachments = attachments;
            let freshness = crate::CheckFreshness::ScopeRead {
                token: observation.verify(root)?,
            };
            let diagnostics = stable(diagnostics);
            return Ok(CheckResult {
                activation: ActivationState::Active,
                valid: Some(diagnostics.is_empty()),
                freshness,
                diagnostics,
            });
        }
        let inventory = metadata_inventory(root)?;
        let bytes = inventory
            .entries
            .get("casefile.toml")
            .map(|entry| {
                if entry.kind == InventoryKind::Regular {
                    crate::scanning::read_observed_entry(root, "casefile.toml", entry)
                } else {
                    Ok(Vec::new())
                }
            })
            .transpose()?;
        let (activation, active, mut diagnostics) = activation_content(bytes.as_deref());
        if inventory
            .entries
            .get("casefile.toml")
            .is_some_and(|entry| entry.kind != InventoryKind::Regular)
        {
            diagnostics = vec![Diagnostic::new(
                "casefile.toml",
                "unsafe_path",
                "activation must be a regular non-symlink file",
            )];
        }
        if activation == ActivationState::Active {
            diagnostics.extend(
                check_scope(
                    root,
                    inventory
                        .entries
                        .iter()
                        .filter(|(path, _)| path.as_str() != "casefile.toml"),
                    &active,
                    None,
                )?
                .0,
            );
        }
        crate::scanning::require_inventory_unchanged(root, &inventory)?;
        let diagnostics = stable(diagnostics);
        let valid = match activation {
            ActivationState::Unactivated => None,
            ActivationState::Active => Some(diagnostics.is_empty()),
            ActivationState::Invalid => Some(false),
        };
        Ok(CheckResult {
            activation,
            valid,
            freshness: crate::CheckFreshness::Store {
                revision: inventory.revision,
            },
            diagnostics,
        })
    }
}

fn check_scope<'a>(
    root: &std::path::Path,
    inventory: impl Iterator<Item = (&'a String, &'a crate::scanning::InventoryEntry)>,
    active: &Activation,
    scope: Option<&str>,
) -> Result<
    (
        Vec<Diagnostic>,
        std::collections::BTreeMap<String, crate::AttachmentState>,
    ),
    StoreError,
> {
    let scopes = crate::activation::ScopeIndex::new(active);
    let mut entries = Vec::new();
    let mut facts = ValidationFacts::default();
    let mut diagnostics = Vec::new();
    for (path, file) in inventory {
        let resolved = scopes.resolve(path);
        let selected = scope.is_none_or(|scope| resolved.scope == Some(scope));
        let support = !selected
            && matches!(
                resolved.kind,
                Some(Kind::Ticket | Kind::Epic | Kind::Decision)
            );
        if !selected && !support {
            continue;
        }
        let mut entry = EntrySnapshot {
            path: path.clone(),
            classification: if resolved.scope.is_some() {
                Classification::Raw
            } else {
                Classification::Ungoverned
            },
            kind: resolved.kind,
            identity: None,
            summary: None,
            content_revision: file.revision.clone(),
            original_bytes: Vec::new(),
        };
        let mut parsed = crate::scanning::classification::ParsedFacts::default();
        let container = resolved
            .scope
            .is_some_and(|scope| crate::layout::scope_container(path, scope));
        if container {
            entry.classification = Classification::Invalid;
            if selected {
                diagnostics.push(Diagnostic::new(
                    path,
                    "unsafe_path",
                    "governed container must be a non-symlink directory",
                ));
            }
        } else if resolved.kind.is_some() {
            if file.kind == InventoryKind::Regular {
                entry.original_bytes = crate::scanning::read_observed_entry(root, path, file)?;
                #[cfg(test)]
                BODY_PEAK.with(|peak| peak.set(peak.get().max(entry.original_bytes.capacity())));
                let classified = crate::scanning::classify_facts(
                    path,
                    &entry.original_bytes,
                    active,
                    resolved.kind,
                );
                let (classification, kind, identity, summary, found) = classified.classification;
                entry.classification = classification;
                entry.kind = kind;
                entry.identity = identity;
                entry.summary = summary;
                if selected {
                    diagnostics.extend(found);
                }
                parsed = classified.facts;
            } else {
                entry.classification = Classification::Invalid;
                if selected {
                    diagnostics.push(Diagnostic::new(
                        path,
                        "unsafe_path",
                        "governed paths must be regular non-symlink files",
                    ));
                }
            }
        }
        facts.insert(&entry, resolved, parsed);
        entry.original_bytes = Vec::new();
        if file.kind == InventoryKind::Regular || entry.kind.is_some() {
            entries.push(entry);
        }
    }
    let mut attachments = std::collections::BTreeMap::new();
    for path in facts
        .attachment_targets()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let state = crate::read_context::observe_attachment(root, &path)?;
        if state == crate::AttachmentState::Regular
            && !entries.iter().any(|entry| entry.path == path)
        {
            let resolved = scopes.resolve(&path);
            let entry = EntrySnapshot {
                path: path.clone(),
                classification: Classification::Raw,
                kind: None,
                identity: None,
                summary: None,
                content_revision: Revision("attachment-existence".into()),
                original_bytes: Vec::new(),
            };
            facts.resolved.insert(path.clone(), resolved);
            entries.push(entry);
        }
        attachments.insert(path, state);
    }
    if scope.is_some() {
        entries.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    }
    diagnostics.extend(
        cross_validate_facts(&entries, active, &facts, |path| {
            attachments.get(path) == Some(&crate::AttachmentState::Regular)
        })
        .into_iter()
        .filter(|diagnostic| {
            scope.is_none_or(|scope| {
                facts
                    .resolved
                    .get(&diagnostic.path)
                    .is_some_and(|resolved| resolved.scope == Some(scope))
            })
        }),
    );
    diagnostics.extend(crate::scanning::binding_diagnostics_facts(&entries, &facts));
    Ok((diagnostics, attachments))
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
        fs::create_dir_all(root.path().join(format!("{other}/boards"))).unwrap();
        let foreign_board = root.path().join(format!("{other}/boards/broken.toml"));
        fs::write(&foreign_board, [0xff, 0xfe]).unwrap();
        let store = Store::open(root.path()).unwrap();
        OPENED.with(|paths| *paths.borrow_mut() = Some(Vec::new()));
        let checked = store.check(Some(scope)).unwrap();
        let opened = OPENED.with(|paths| paths.borrow_mut().take().unwrap());
        assert!(!opened.contains(&foreign));
        assert!(!opened.contains(&foreign_board));
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
            "[projects]\ndemo = '//source/demo'\n",
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
