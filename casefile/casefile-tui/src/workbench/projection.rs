use super::*;

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

    pub(super) fn merge_projection(&mut self, projection: UiProjection) {
        let changed = projection
            .scan
            .snapshot
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .chain(projection.removed.iter().map(String::as_str))
            .collect::<std::collections::BTreeSet<_>>();
        let identities = changed
            .iter()
            .filter_map(|path| self.record_indices.get(*path))
            .filter_map(|index| self.derived.records[*index].identity.as_ref())
            .collect::<Vec<_>>();
        self.derived
            .boards
            .retain(|board| !identities.contains(&&board.identity));
        self.derived.relationships.retain(|relationship| {
            !projection
                .relationship_updates
                .contains_key(&relationship.source)
        });
        self.scan
            .diagnostics
            .retain(|diagnostic| !changed.contains(diagnostic.path.as_str()));
        for path in &changed {
            self.unavailable.remove(*path);
        }
        for path in &projection.availability_changed {
            self.unavailable.remove(path);
        }
        let new_records = projection
            .derived
            .records
            .iter()
            .map(|record| record.path.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let record_count = self.derived.records.len();
        if changed
            .iter()
            .any(|path| self.record_indices.contains_key(*path) && !new_records.contains(path))
        {
            self.derived.records.retain(|record| {
                !changed.contains(record.path.as_str())
                    || new_records.contains(record.path.as_str())
            });
        }
        if !projection.removed.is_empty() {
            self.scan
                .snapshot
                .entries
                .retain(|entry| !projection.removed.contains(&entry.path));
        }
        if !projection.removed.is_empty() || record_count != self.derived.records.len() {
            self.reindex();
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
        self.scan.activation = projection.scan.activation;
        self.scan.investigation_roots = projection.scan.investigation_roots;
        self.scan.diagnostics.extend(projection.scan.diagnostics);
        self.scan
            .diagnostics
            .sort_by(|a, b| (&a.path, &a.code, &a.message).cmp(&(&b.path, &b.code, &b.message)));
        self.scan.diagnostics.dedup();
        self.derived.diagnostics = self.scan.diagnostics.clone();
        self.derived.boards.extend(projection.derived.boards);
        self.derived
            .relationships
            .extend(projection.relationship_updates.into_values().flatten());
        self.derived.relationships.sort_by(|left, right| {
            (&left.source, left.kind as u8, &left.target).cmp(&(
                &right.source,
                right.kind as u8,
                &right.target,
            ))
        });
        self.unavailable.extend(projection.unavailable);
    }
}
