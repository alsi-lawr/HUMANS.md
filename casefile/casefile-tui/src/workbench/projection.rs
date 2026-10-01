use super::*;
use std::collections::BTreeSet;

impl App {
    pub(super) fn reindex(&mut self) {
        self.entry_indices = self
            .scan
            .snapshot
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.path.clone(), i))
            .collect();
        self.record_indices = self
            .derived
            .records
            .iter()
            .enumerate()
            .map(|(i, record)| (record.path.clone(), i))
            .collect();
    }

    pub(super) fn boards_affected(&self, projection: &UiProjection) -> bool {
        if projection.catalogue_changed {
            return true;
        }
        let (Some(project), Some(investigation)) = (
            self.browser.selected_project(),
            self.browser.selected_investigation(),
        ) else {
            return false;
        };
        let selected = |identity: &casefile_store::ScopedIdentity| {
            identity.scope.project == project
                && identity.scope.investigation.as_deref() == Some(investigation)
        };
        if projection.derived.boards.iter().any(|board| {
            selected(&board.identity) && self.board_records.get(&board.identity) != Some(board)
        }) {
            return true;
        }
        for path in &projection.removed {
            if let Some(index) = self.entry_indices.get(path) {
                let old = &self.scan.snapshot.entries[*index];
                if old.identity.is_some()
                    && self.scan.scope_for_path(path) == Some((project, Some(investigation)))
                {
                    return true;
                }
            }
        }
        let new_boards = projection
            .derived
            .boards
            .iter()
            .map(|board| &board.identity)
            .collect::<BTreeSet<_>>();
        for entry in &projection.scan.snapshot.entries {
            if self.scan.scope_for_path(&entry.path) != Some((project, Some(investigation))) {
                continue;
            }
            let old = self
                .entry_indices
                .get(&entry.path)
                .map(|index| &self.scan.snapshot.entries[*index]);
            if old
                .and_then(|old| self.record_indices.get(&old.path))
                .map(|index| &self.derived.records[*index])
                .filter(|record| record.kind == Some(casefile_core::Kind::Board))
                .and_then(|record| record.identity.as_ref())
                .is_some_and(|identity| {
                    self.board_records.contains_key(identity) && !new_boards.contains(identity)
                })
            {
                return true;
            }
            if old.is_none_or(|old| {
                old.identity != entry.identity
                    || old.classification != entry.classification
                    || old.kind != entry.kind
            }) {
                return true;
            }
            if (entry.kind == Some(casefile_core::Kind::Board)
                || entry.path.ends_with("/progress/log.toml"))
                && self
                    .facts
                    .diagnostics(Some(&entry.path))
                    .iter()
                    .ne(projection
                        .scan
                        .diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.path == entry.path))
            {
                return true;
            }
        }
        false
    }

    pub(super) fn merge_projection(&mut self, projection: UiProjection) {
        let changed = projection
            .scan
            .snapshot
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .chain(projection.removed.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut index_changes = changed.clone();
        let new_records = projection
            .derived
            .records
            .iter()
            .map(|record| record.path.as_str())
            .collect::<BTreeSet<_>>();
        for path in &changed {
            self.unavailable.remove(path);
            if let Some(index) = self.record_indices.get(path) {
                let old = &self.derived.records[*index];
                if old.kind == Some(casefile_core::Kind::Board)
                    && let Some(identity) = &old.identity
                {
                    self.board_records.remove(identity);
                }
            }
            if !new_records.contains(path.as_str())
                && let Some(index) = self.record_indices.remove(path)
            {
                self.derived.records.swap_remove(index);
                if let Some(moved) = self.derived.records.get(index) {
                    self.record_indices.insert(moved.path.clone(), index);
                }
            }
        }
        for path in &projection.removed {
            if let Some(index) = self.entry_indices.remove(path) {
                self.scan.snapshot.entries.swap_remove(index);
                if let Some(moved) = self.scan.snapshot.entries.get(index) {
                    self.entry_indices.insert(moved.path.clone(), index);
                    index_changes.insert(moved.path.clone());
                }
            }
            self.body_owners.remove(path);
        }
        for path in &projection.availability_changed {
            self.unavailable.remove(path);
        }
        for entry in projection.scan.snapshot.entries {
            if let Some(index) = self.entry_indices.get(&entry.path) {
                self.scan.snapshot.entries[*index] = entry;
            } else {
                self.entry_indices
                    .insert(entry.path.clone(), self.scan.snapshot.entries.len());
                self.scan.snapshot.entries.push(entry);
            }
        }
        for record in projection.derived.records {
            if let Some(index) = self.record_indices.get(&record.path) {
                self.derived.records[*index] = record;
            } else {
                self.record_indices
                    .insert(record.path.clone(), self.derived.records.len());
                self.derived.records.push(record);
            }
        }
        for board in projection.derived.boards {
            self.board_records.insert(board.identity.clone(), board);
        }
        for (identity, edges) in projection.relationship_updates {
            if edges.is_empty() {
                self.relationships.remove(&identity);
            } else {
                self.relationships.insert(identity, edges);
            }
        }
        if projection.catalogue_changed {
            self.scan.activation = projection.scan.activation;
            self.scan.investigation_roots = projection.scan.investigation_roots;
            self.browser.rebuild(&self.scan);
        } else {
            self.browser
                .update(&self.scan, &self.entry_indices, &index_changes);
        }
        if projection.catalogue_changed {
            self.facts.reindex_scopes(&self.scan);
        } else {
            self.facts.update(&self.scan, &self.entry_indices, &changed);
        }
        self.facts
            .replace_entry_diagnostics(&self.scan, &changed, projection.scan.diagnostics);
        if let Some(diagnostics) = projection.catalogue_diagnostics {
            self.facts
                .replace_catalogue_diagnostics(&self.scan, diagnostics);
        }
        self.body_owners.extend(projection.body_owners);
        self.unavailable.extend(projection.unavailable);
        if self
            .browser
            .selected_path()
            .is_some_and(|path| changed.contains(path))
            || projection.catalogue_changed
        {
            self.detail.invalidate();
        }
    }
}
