use crate::{
    activation::{Activation, ActivationState, activation_content, investigation_identity},
    layout::checked_path,
    revision::{metadata_revision, store_revision, synthetic_revision, target_revision},
    scanning::{ScanResult, binding_diagnostics, classify},
    store::{StoreError, require_safe_target_parent},
    validation::cross_validate,
};
use casefile_core::{CasefileSnapshot, EntrySnapshot, Revision, stable};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::{Path, PathBuf},
};

pub(super) type Overlay = BTreeMap<String, Option<Vec<u8>>>;

pub(super) struct MutationContext {
    pub(super) before: ScanResult,
    pub(super) active: Activation,
    root: PathBuf,
    files: BTreeMap<String, Option<EntrySnapshot>>,
    validation_paths: BTreeSet<String>,
    _locks: Vec<File>,
}

impl MutationContext {
    pub(super) fn capture(
        root: &Path,
        changes: &Overlay,
        extra: &[String],
        applying: bool,
    ) -> Result<Self, StoreError> {
        let selected = super::mutation_dependencies::discover(root, changes, extra)?;
        #[cfg(test)]
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Attempt, root, "");
        let locks = super::mutation_locks::acquire(root, &selected.locks(changes, applying))?;
        #[cfg(test)]
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Locked, root, "");
        let confirmed = super::mutation_dependencies::discover(root, changes, extra)?;
        if !confirmed.paths.is_subset(&selected.paths)
            || confirmed.locks(changes, applying) != selected.locks(changes, applying)
        {
            return Err(StoreError::StaleTargetRevision);
        }
        let mut files = BTreeMap::new();
        for path in &confirmed.paths {
            files.insert(
                path.clone(),
                read_input(root, path, !confirmed.existence.contains(path))?,
            );
        }
        let activation_bytes = files
            .get("casefile.toml")
            .and_then(Option::as_ref)
            .map(|e| e.original_bytes.as_slice());
        let (state, active, _) = activation_content(activation_bytes);
        if state != ActivationState::Active {
            return Err(StoreError::Invalid(
                "mutations require an active Casefile configuration".into(),
            ));
        }
        let before = projection(
            &files,
            &active,
            &Overlay::new(),
            &confirmed.validation_paths,
        );
        let result = Self {
            before,
            active,
            root: root.into(),
            files,
            validation_paths: confirmed.validation_paths,
            _locks: locks,
        };
        result.require_unchanged()?;
        Ok(result)
    }

    pub(super) fn overlay(&self, changes: &Overlay) -> ScanResult {
        projection(&self.files, &self.active, changes, &self.validation_paths)
    }

    pub(super) fn revisions(&self) -> BTreeMap<String, Option<Revision>> {
        self.files
            .iter()
            .map(|(path, entry)| {
                (
                    path.clone(),
                    entry.as_ref().map(|e| e.content_revision.clone()),
                )
            })
            .collect()
    }

    pub(super) fn require_revisions(
        &self,
        expected: &BTreeMap<String, Option<Revision>>,
    ) -> Result<(), StoreError> {
        if expected.iter().any(|(path, revision)| {
            self.files
                .get(path)
                .and_then(Option::as_ref)
                .map(|e| &e.content_revision)
                != revision.as_ref()
        }) {
            return Err(StoreError::StaleTargetRevision);
        }
        Ok(())
    }

    pub(super) fn require_unchanged(&self) -> Result<(), StoreError> {
        #[cfg(test)]
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Commit, &self.root, "");
        for (path, entry) in &self.files {
            if target_revision(&self.root.join(path))?.as_ref()
                != entry.as_ref().map(|e| &e.content_revision)
            {
                return Err(StoreError::StaleTargetRevision);
            }
        }
        Ok(())
    }

    pub(super) fn resulting(&self, changes: &Overlay) -> Result<ScanResult, StoreError> {
        #[cfg(test)]
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Result, &self.root, "");
        let mut files = self.files.clone();
        for path in changes.keys() {
            files.insert(path.clone(), read_entry(&self.root, path)?);
        }
        Ok(projection(
            &files,
            &self.active,
            &Overlay::new(),
            &self.validation_paths,
        ))
    }
}

pub(super) fn read_entry(root: &Path, path: &str) -> Result<Option<EntrySnapshot>, StoreError> {
    read_input(root, path, true)
}

fn read_input(root: &Path, path: &str, body: bool) -> Result<Option<EntrySnapshot>, StoreError> {
    let path = checked_path(path)?;
    require_safe_target_parent(
        root,
        Path::new(&path).parent().unwrap_or(Path::new("")),
        "mutation input",
    )?;
    let target = root.join(&path);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(StoreError::Invalid(format!(
            "{path} must be a regular non-symlink file"
        )));
    }
    let revision = metadata_revision(&target, &metadata)?;
    #[cfg(test)]
    if body {
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Read, root, &path);
    }
    let bytes = if body {
        crate::scanning::read_inventory_entry(&crate::scanning::InventoryEntry {
            path: target,
            kind: crate::scanning::InventoryKind::Regular,
            revision: revision.clone(),
        })?
    } else {
        Vec::new()
    };
    Ok(Some(EntrySnapshot {
        path,
        classification: casefile_core::Classification::Raw,
        kind: None,
        identity: None,
        content_revision: revision,
        summary: None,
        original_bytes: bytes,
    }))
}

fn projection(
    files: &BTreeMap<String, Option<EntrySnapshot>>,
    active: &Activation,
    changes: &Overlay,
    validation_paths: &BTreeSet<String>,
) -> ScanResult {
    let mut files = files.clone();
    for (path, bytes) in changes {
        files.insert(
            path.clone(),
            bytes.as_ref().map(|bytes| EntrySnapshot {
                path: path.clone(),
                classification: casefile_core::Classification::Raw,
                kind: None,
                identity: None,
                content_revision: synthetic_revision(path, true),
                summary: None,
                original_bytes: bytes.clone(),
            }),
        );
    }
    let mut diagnostics = Vec::new();
    let entries = files
        .into_values()
        .flatten()
        .map(|mut entry| {
            let (classification, kind, identity, summary, found) =
                classify(&entry.path, &entry.original_bytes, active);
            entry.classification = classification;
            entry.kind = kind;
            entry.identity = identity;
            entry.summary = summary;
            diagnostics.extend(found);
            entry
        })
        .collect::<Vec<_>>();
    diagnostics.extend(
        cross_validate(&entries, active)
            .into_iter()
            .filter(|diagnostic| {
                validation_paths.contains(&diagnostic.path)
                    || diagnostic.code == "duplicate_identity"
            }),
    );
    diagnostics.extend(binding_diagnostics(&entries));
    // This internal projection token never leaves the mutation context as a Store revision.
    let revision = store_revision(
        entries
            .iter()
            .map(|e| (e.path.as_str(), &e.content_revision)),
        true,
    );
    ScanResult {
        activation: ActivationState::Active,
        investigation_roots: active
            .projects
            .iter()
            .map(|(slug, p)| {
                (
                    slug.clone(),
                    p.investigations
                        .iter()
                        .filter_map(|i| investigation_identity(slug, i).map(str::to_owned))
                        .collect(),
                )
            })
            .collect(),
        snapshot: CasefileSnapshot { revision, entries },
        diagnostics: stable(diagnostics),
    }
}
