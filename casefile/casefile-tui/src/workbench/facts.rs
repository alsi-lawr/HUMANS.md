use super::*;
use casefile_core::Diagnostic;
use casefile_store::{RecordScope, ScopedIdentity};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Facts {
    identities: BTreeMap<ScopedIdentity, BTreeSet<String>>,
    identity_by_path: BTreeMap<String, ScopedIdentity>,
    diagnostics: BTreeMap<String, Vec<Diagnostic>>,
    entry_diagnostics: BTreeMap<String, Vec<Diagnostic>>,
    catalogue_diagnostics: BTreeMap<String, Vec<Diagnostic>>,
    diagnostic_count: usize,
    board_paths: BTreeMap<(String, String), BTreeSet<String>>,
    board_scope_by_path: BTreeMap<String, (String, String)>,
    board_diagnostics: BTreeMap<(String, String), Vec<Diagnostic>>,
}

impl Facts {
    pub(super) fn rebuild(&mut self, scan: &ScanResult) {
        *self = Self::default();
        for entry in &scan.snapshot.entries {
            self.insert_identity(scan, entry);
        }
        let paths = scan
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.path.clone())
            .collect();
        self.replace_entry_diagnostics(scan, &paths, scan.diagnostics.clone());
    }

    pub(super) fn reindex_scopes(&mut self, scan: &ScanResult) {
        self.identities.clear();
        self.identity_by_path.clear();
        for entry in &scan.snapshot.entries {
            self.insert_identity(scan, entry);
        }
        self.board_paths.clear();
        self.board_scope_by_path.clear();
        self.board_diagnostics.clear();
        let paths = self.diagnostics.keys().cloned().collect();
        self.update_diagnostics(scan, &paths);
    }

    pub(super) fn update(
        &mut self,
        scan: &ScanResult,
        indices: &BTreeMap<String, usize>,
        changed: &BTreeSet<String>,
    ) {
        for path in changed {
            if let Some(identity) = self.identity_by_path.remove(path)
                && let Some(paths) = self.identities.get_mut(&identity)
            {
                paths.remove(path);
                if paths.is_empty() {
                    self.identities.remove(&identity);
                }
            }
            if let Some(index) = indices.get(path) {
                self.insert_identity(scan, &scan.snapshot.entries[*index]);
            }
        }
    }

    pub(super) fn replace_entry_diagnostics(
        &mut self,
        scan: &ScanResult,
        changed: &BTreeSet<String>,
        diagnostics: Vec<Diagnostic>,
    ) {
        for path in changed {
            self.entry_diagnostics.remove(path);
        }
        for diagnostic in diagnostics {
            self.entry_diagnostics
                .entry(diagnostic.path.clone())
                .or_default()
                .push(diagnostic);
        }
        self.update_diagnostics(scan, changed);
    }

    pub(super) fn replace_catalogue_diagnostics(
        &mut self,
        scan: &ScanResult,
        diagnostics: Vec<Diagnostic>,
    ) {
        let mut changed = self
            .catalogue_diagnostics
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.catalogue_diagnostics.clear();
        for diagnostic in diagnostics {
            changed.insert(diagnostic.path.clone());
            self.catalogue_diagnostics
                .entry(diagnostic.path.clone())
                .or_default()
                .push(diagnostic);
        }
        self.update_diagnostics(scan, &changed);
    }

    fn update_diagnostics(&mut self, scan: &ScanResult, changed: &BTreeSet<String>) {
        let mut scopes = BTreeSet::new();
        for path in changed {
            let mut values = self
                .entry_diagnostics
                .get(path)
                .into_iter()
                .flatten()
                .chain(self.catalogue_diagnostics.get(path).into_iter().flatten())
                .cloned()
                .collect::<Vec<_>>();
            values.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
            values.dedup();
            self.diagnostic_count -= self.diagnostics.get(path).map_or(0, Vec::len);
            self.diagnostic_count += values.len();
            if values.is_empty() {
                self.diagnostics.remove(path);
            } else {
                self.diagnostics.insert(path.clone(), values);
            }
            if let Some(scope) = self.board_scope_by_path.remove(path) {
                if let Some(paths) = self.board_paths.get_mut(&scope) {
                    paths.remove(path);
                }
                scopes.insert(scope);
            }
            if self.diagnostics.contains_key(path)
                && let Some((project, Some(investigation))) = scan.scope_for_path(path)
            {
                let prefix = format!("projects/{project}/investigations/{investigation}/");
                if path.strip_prefix(&prefix).is_some_and(|relative| {
                    relative.starts_with("boards/") || relative == "progress/log.toml"
                }) {
                    let scope = (project.to_owned(), investigation.to_owned());
                    self.board_scope_by_path.insert(path.clone(), scope.clone());
                    self.board_paths
                        .entry(scope.clone())
                        .or_default()
                        .insert(path.clone());
                    scopes.insert(scope);
                }
            }
        }
        for scope in scopes {
            let values = self
                .board_paths
                .get(&scope)
                .into_iter()
                .flatten()
                .filter_map(|path| self.diagnostics.get(path))
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            if values.is_empty() {
                self.board_diagnostics.remove(&scope);
            } else {
                self.board_diagnostics.insert(scope, values);
            }
        }
    }

    pub(super) fn diagnostic_count(&self) -> usize {
        self.diagnostic_count
    }

    fn insert_identity(&mut self, scan: &ScanResult, entry: &casefile_core::EntrySnapshot) {
        let Some(identity) = &entry.identity else {
            return;
        };
        let Some((project, investigation)) = scan.scope_for_path(&entry.path) else {
            return;
        };
        let identity = ScopedIdentity {
            scope: RecordScope {
                project: project.into(),
                investigation: investigation.map(Into::into),
            },
            identity: identity.clone(),
        };
        self.identities
            .entry(identity.clone())
            .or_default()
            .insert(entry.path.clone());
        self.identity_by_path.insert(entry.path.clone(), identity);
    }

    pub(super) fn card_path(&self, card: &casefile_store::DerivedCard) -> CardPathResolution {
        match self.identities.get(&card.identity) {
            Some(paths) if paths.len() == 1 => {
                CardPathResolution::Resolved(paths.first().expect("single identity").clone())
            }
            Some(_) => CardPathResolution::Ambiguous,
            None => CardPathResolution::Missing,
        }
    }

    pub(super) fn diagnostics(&self, path: Option<&str>) -> &[Diagnostic] {
        path.and_then(|path| self.diagnostics.get(path))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(super) fn board_diagnostics(&self, project: &str, investigation: &str) -> &[Diagnostic] {
        self.board_diagnostics
            .get(&(project.into(), investigation.into()))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}
