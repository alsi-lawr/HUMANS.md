use crate::{
    activation::{Activation, ActivationState, ScopeIndex, activation_content},
    layout::checked_path,
    mutation_projection::Projection,
    revision::{metadata_revision, target_revision},
    scanning::{ScanResult, classification::ParsedFacts, classify_facts},
    store::{StoreError, require_safe_target_parent},
};
use casefile_core::{EntrySnapshot, Revision};
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
    parsed: BTreeMap<String, ParsedFacts>,
    local_diagnostics: BTreeMap<String, Vec<casefile_core::Diagnostic>>,
    progress_operations: BTreeMap<String, Vec<(String, String)>>,
    _locks: Vec<File>,
}

impl MutationContext {
    pub(super) fn capture(
        root: &Path,
        changes: &Overlay,
        extra: &[String],
        applying: bool,
    ) -> Result<Self, StoreError> {
        Self::capture_seeded(root, changes, extra, applying, None, None)
    }

    pub(super) fn capture_seeded(
        root: &Path,
        changes: &Overlay,
        extra: &[String],
        applying: bool,
        initial_progress: Option<(String, super::mutation_dependencies::ProgressInput)>,
        proposed_progress: Option<(&str, &casefile_core::ProgressLog)>,
    ) -> Result<Self, StoreError> {
        let selected = super::mutation_dependencies::discover(
            root,
            changes,
            extra,
            initial_progress,
            proposed_progress,
        )?;
        #[cfg(test)]
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Attempt, root, "");
        let locks = super::mutation_locks::acquire(root, &selected.locks(changes, applying))?;
        #[cfg(test)]
        crate::mutation_hooks::event(crate::mutation_hooks::Boundary::Locked, root, "");
        let mut confirmed =
            super::mutation_dependencies::discover(root, changes, extra, None, proposed_progress)?;
        if !confirmed.paths.is_subset(&selected.paths)
            || confirmed.locks(changes, applying) != selected.locks(changes, applying)
        {
            return Err(StoreError::StaleTargetRevision);
        }
        let mut files = BTreeMap::new();
        let mut progress_logs = BTreeMap::new();
        for path in &confirmed.paths {
            let entry = match confirmed.progress_inputs.remove(path) {
                Some(input) => {
                    progress_logs.insert(path.clone(), input.log);
                    input.entry
                }
                None => read_input(root, path, !confirmed.existence.contains(path))?,
            };
            files.insert(path.clone(), entry);
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
        let mut progress_operations = BTreeMap::new();
        let mut parsed = BTreeMap::new();
        let mut local_diagnostics = BTreeMap::new();
        let scopes = ScopeIndex::new(&active);
        for (path, entry) in &mut files {
            if let Some(entry) = entry {
                let classified = match progress_logs.remove(path) {
                    Some(Ok(Some(super::mutation_dependencies::ProgressFacts::Full(log)))) => {
                        crate::scanning::classification::Classified {
                            classification: (
                                casefile_core::Classification::Governed,
                                Some(casefile_core::Kind::Progress),
                                None,
                                Some(casefile_core::RecordSummary::Progress),
                                Vec::new(),
                            ),
                            facts: ParsedFacts {
                                progress: Some(
                                    std::sync::Arc::try_unwrap(log)
                                        .expect("confirmed progress fact has one owner"),
                                ),
                                ..ParsedFacts::default()
                            },
                        }
                    }
                    Some(Ok(Some(super::mutation_dependencies::ProgressFacts::Operations(
                        operations,
                    )))) => {
                        progress_operations.insert(path.clone(), operations);
                        crate::scanning::classification::Classified {
                            classification: (
                                casefile_core::Classification::Governed,
                                Some(casefile_core::Kind::Progress),
                                None,
                                Some(casefile_core::RecordSummary::Progress),
                                Vec::new(),
                            ),
                            facts: ParsedFacts::default(),
                        }
                    }
                    Some(Err(diagnostics)) => crate::scanning::classification::Classified {
                        classification: (
                            casefile_core::Classification::Invalid,
                            Some(casefile_core::Kind::Progress),
                            None,
                            None,
                            diagnostics,
                        ),
                        facts: ParsedFacts::default(),
                    },
                    _ => classify_facts(
                        path,
                        &entry.original_bytes,
                        &active,
                        scopes.resolve(path).kind,
                    ),
                };
                (
                    entry.classification,
                    entry.kind,
                    entry.identity,
                    entry.summary,
                    _,
                ) = classified.classification.clone();
                local_diagnostics.insert(path.clone(), classified.classification.4);
                parsed.insert(path.clone(), classified.facts);
            }
        }
        let before = Projection {
            active: &active,
            validation_paths: &confirmed.validation_paths,
            parsed: &parsed,
            local_diagnostics: &local_diagnostics,
            progress_operations: &progress_operations,
        }
        .run(
            files
                .iter()
                .map(|(path, entry)| (path.as_str(), entry.as_ref())),
            &Overlay::new(),
            &BTreeSet::new(),
            None,
            None,
        );
        let result = Self {
            before,
            active,
            root: root.into(),
            files,
            validation_paths: confirmed.validation_paths,
            parsed,
            local_diagnostics,
            progress_operations,
            _locks: locks,
        };
        result.require_unchanged()?;
        Ok(result)
    }

    fn projection(&self) -> Projection<'_> {
        Projection {
            active: &self.active,
            validation_paths: &self.validation_paths,
            parsed: &self.parsed,
            local_diagnostics: &self.local_diagnostics,
            progress_operations: &self.progress_operations,
        }
    }

    pub(super) fn overlay(&self, changes: &Overlay) -> ScanResult {
        self.projection().run(
            self.files
                .iter()
                .map(|(path, entry)| (path.as_str(), entry.as_ref())),
            changes,
            &BTreeSet::new(),
            None,
            None,
        )
    }

    pub(super) fn overlay_strategy(
        &self,
        changes: &Overlay,
        path: &str,
        selected: &casefile_core::SelectedStrategyMatrix,
    ) -> ScanResult {
        self.projection().run(
            self.files
                .iter()
                .map(|(path, entry)| (path.as_str(), entry.as_ref())),
            changes,
            &BTreeSet::new(),
            Some((path, selected)),
            None,
        )
    }

    pub(super) fn overlay_progress(
        &self,
        changes: &Overlay,
        path: &str,
        log: &casefile_core::ProgressLog,
    ) -> ScanResult {
        self.projection().run(
            self.files
                .iter()
                .map(|(path, entry)| (path.as_str(), entry.as_ref())),
            changes,
            &BTreeSet::new(),
            None,
            Some((path, log)),
        )
    }

    pub(super) fn entry(&self, path: &str) -> Option<&EntrySnapshot> {
        self.files.get(path).and_then(Option::as_ref)
    }

    pub(super) fn facts(&self, path: &str) -> Option<&ParsedFacts> {
        self.parsed.get(path)
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
        crate::mutation_hooks::resulting(&self.root)?;
        let mut changed = BTreeMap::new();
        for path in changes.keys() {
            changed.insert(path.clone(), read_entry(&self.root, path)?);
        }
        let files = self
            .files
            .iter()
            .filter(|(path, _)| !changed.contains_key(*path))
            .chain(changed.iter());
        let mut result = self.projection().run(
            files.map(|(path, entry)| (path.as_str(), entry.as_ref())),
            &Overlay::new(),
            &changes.keys().cloned().collect(),
            None,
            None,
        );
        for entry in &mut result.snapshot.entries {
            if let Some(Some(actual)) = changed.remove(&entry.path) {
                entry.original_bytes = actual.original_bytes;
            }
        }
        Ok(result)
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
