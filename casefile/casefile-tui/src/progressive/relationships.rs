use super::*;
use casefile_core::Kind;
use casefile_store::{DerivedRecord, PresentationProject, derive_relationships};

#[derive(Default)]
pub(super) struct RelationshipState {
    records: BTreeMap<String, BTreeSet<String>>,
    covered: BTreeMap<String, PresentationProject>,
    resolved: BTreeMap<String, BTreeMap<ScopedIdentity, Vec<DerivedRelationship>>>,
    dirty: BTreeSet<String>,
    updates: BTreeMap<ScopedIdentity, Vec<DerivedRelationship>>,
    availability_changed: BTreeSet<String>,
}

impl RelationshipState {
    pub(super) fn entry_changed(
        &mut self,
        path: &str,
        old: Option<&PresentationEntry>,
        new: Option<&PresentationEntry>,
    ) {
        let old = old.and_then(reference_record);
        let new = new.and_then(reference_record);
        if old.map(signature) == new.map(signature)
            && old.and_then(references) == new.and_then(references)
        {
            return;
        }
        if let Some(identity) = old.and_then(|record| record.identity.as_ref()) {
            let project = &identity.scope.project;
            if let Some(paths) = self.records.get_mut(project) {
                paths.remove(path);
            }
            self.dirty.insert(project.clone());
        }
        if let Some(identity) = new.and_then(|record| record.identity.as_ref()) {
            let project = &identity.scope.project;
            self.records
                .entry(project.clone())
                .or_default()
                .insert(path.into());
            if self.covered.get(project).is_some_and(|covered| {
                identity
                    .scope
                    .investigation
                    .as_ref()
                    .is_some_and(|identity| {
                        !covered
                            .investigations
                            .iter()
                            .any(|scope| &scope.identity == identity)
                    })
            }) {
                self.covered.remove(project);
                self.availability_changed
                    .extend(self.records.get(project).into_iter().flatten().cloned());
            }
            self.dirty.insert(project.clone());
        }
    }

    pub(super) fn promote(
        &mut self,
        target: &PresentationTarget,
        catalogue: Option<&PresentationCatalogue>,
        entries: &BTreeMap<String, Arc<PresentationEntry>>,
    ) {
        let projects = catalogue
            .into_iter()
            .flat_map(|catalogue| &catalogue.projects)
            .map(|project| {
                let mut project = project.clone();
                project
                    .investigations
                    .sort_by(|a, b| (&a.path, &a.identity).cmp(&(&b.path, &b.identity)));
                project.investigations.dedup();
                (project.slug.clone(), project)
            })
            .collect::<BTreeMap<_, _>>();
        self.covered.retain(|slug, covered| {
            let keep = projects.get(slug) == Some(covered);
            if !keep {
                self.dirty.insert(slug.clone());
                self.availability_changed
                    .extend(self.records.get(slug).into_iter().flatten().cloned());
            }
            keep
        });
        for (slug, project) in projects {
            let covered = match target {
                PresentationTarget::Store => true,
                PresentationTarget::Project { project } => project == &slug,
                PresentationTarget::Investigation { .. } => false,
            };
            if covered && !self.covered.contains_key(&slug) {
                self.covered.insert(slug.clone(), project);
                self.dirty.insert(slug.clone());
                self.availability_changed
                    .extend(self.records.get(&slug).into_iter().flatten().cloned());
            }
        }
        self.resolve(entries);
    }

    pub(super) fn resolve(&mut self, entries: &BTreeMap<String, Arc<PresentationEntry>>) {
        for project in std::mem::take(&mut self.dirty) {
            let mut resolved: BTreeMap<ScopedIdentity, Vec<DerivedRelationship>> = BTreeMap::new();
            if self.covered.contains_key(&project) {
                let records = self
                    .records
                    .get(&project)
                    .into_iter()
                    .flatten()
                    .filter_map(|path| entries.get(path))
                    .filter_map(|entry| entry.derived.as_ref());
                for edge in derive_relationships(records) {
                    resolved.entry(edge.source.clone()).or_default().push(edge);
                }
            }
            let previous = self.resolved.entry(project).or_default();
            for identity in previous
                .keys()
                .filter(|identity| !resolved.contains_key(*identity))
            {
                self.updates.insert(identity.clone(), Vec::new());
            }
            for (identity, edges) in &resolved {
                if previous.get(identity) != Some(edges) {
                    self.updates.insert(identity.clone(), edges.clone());
                }
            }
            *previous = resolved;
        }
    }

    pub(super) fn is_available(&self, entry: &PresentationEntry) -> bool {
        let Some(record) = reference_record(entry) else {
            return true;
        };
        if references(record)
            .is_none_or(|references| references.iter().all(|references| references.is_empty()))
        {
            return true;
        }
        record
            .identity
            .as_ref()
            .is_some_and(|identity| self.covered.contains_key(&identity.scope.project))
    }

    pub(super) fn edges(&self) -> impl Iterator<Item = &DerivedRelationship> {
        self.resolved
            .values()
            .flat_map(|sources| sources.values())
            .flatten()
    }

    pub(super) fn take_updates(&mut self) -> BTreeMap<ScopedIdentity, Vec<DerivedRelationship>> {
        std::mem::take(&mut self.updates)
    }
    pub(super) fn take_availability_changes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.availability_changed)
            .into_iter()
            .collect()
    }
}

fn reference_record(entry: &PresentationEntry) -> Option<&DerivedRecord> {
    entry.derived.as_ref().filter(|record| {
        record.identity.is_some()
            && matches!(
                record.kind,
                Some(Kind::Ticket | Kind::Epic | Kind::Decision)
            )
    })
}

fn references(record: &DerivedRecord) -> Option<[&[String]; 4]> {
    record.work_item.as_ref().map(|item| {
        [
            item.decision_refs.as_slice(),
            item.related_tickets.as_slice(),
            item.supersedes.as_slice(),
            item.superseded_by.as_slice(),
        ]
    })
}

fn signature(record: &DerivedRecord) -> (Option<&ScopedIdentity>, Option<Kind>) {
    (record.identity.as_ref(), record.kind)
}
