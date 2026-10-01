use super::*;
use casefile_core::Kind;
use casefile_store::{DerivedRecord, PresentationProject, derive_relationships};

type TargetKey = (String, bool, String);

#[derive(Default)]
pub(super) struct RelationshipState {
    sources: BTreeMap<ScopedIdentity, BTreeSet<String>>,
    targets: BTreeMap<TargetKey, BTreeSet<String>>,
    reverse: BTreeMap<TargetKey, BTreeSet<String>>,
    covered: BTreeMap<String, PresentationProject>,
    resolved: BTreeMap<ScopedIdentity, Vec<DerivedRelationship>>,
    dirty: BTreeSet<ScopedIdentity>,
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
        for record in [old, new].into_iter().flatten() {
            let identity = record.identity.as_ref().expect("reference identity");
            self.dirty.insert(identity.clone());
            let key = target_key(record);
            if let Some(paths) = self.reverse.get(&key) {
                self.availability_changed.extend(paths.iter().cloned());
            }
        }
        if let Some(record) = old {
            let identity = record.identity.as_ref().expect("reference identity");
            remove_path(&mut self.sources, identity, path);
            remove_path(&mut self.targets, &target_key(record), path);
            for key in reference_keys(record) {
                remove_path(&mut self.reverse, &key, path);
            }
        }
        if let Some(record) = new {
            let identity = record.identity.as_ref().expect("reference identity");
            self.sources
                .entry(identity.clone())
                .or_default()
                .insert(path.into());
            self.targets
                .entry(target_key(record))
                .or_default()
                .insert(path.into());
            for key in reference_keys(record) {
                self.reverse.entry(key).or_default().insert(path.into());
            }
            if self
                .covered
                .get(&identity.scope.project)
                .is_some_and(|covered| {
                    identity.scope.investigation.as_ref().is_some_and(|scope| {
                        !covered
                            .investigations
                            .iter()
                            .any(|candidate| &candidate.identity == scope)
                    })
                })
            {
                self.covered.remove(&identity.scope.project);
                self.dirty.extend(
                    self.sources
                        .keys()
                        .filter(|candidate| candidate.scope.project == identity.scope.project)
                        .cloned(),
                );
                self.availability_changed.extend(
                    self.sources
                        .iter()
                        .filter(|(candidate, _)| candidate.scope.project == identity.scope.project)
                        .flat_map(|(_, paths)| paths.iter().cloned()),
                );
            }
        }
        for record in [old, new].into_iter().flatten() {
            if let Some(paths) = self.reverse.get(&target_key(record)) {
                self.availability_changed.extend(paths.iter().cloned());
            }
        }
    }

    pub(super) fn promote(
        &mut self,
        target: &PresentationTarget,
        catalogue: Option<&PresentationCatalogue>,
        entries: &BTreeMap<String, Arc<PresentationEntry>>,
    ) {
        if let Some(catalogue) = catalogue {
            let projects = catalogue
                .projects
                .iter()
                .map(|project| (project.slug.as_str(), project))
                .collect::<BTreeMap<_, _>>();
            let removed = self
                .covered
                .iter()
                .filter(|(slug, covered)| projects.get(slug.as_str()).copied() != Some(*covered))
                .map(|(slug, _)| slug.clone())
                .collect::<Vec<_>>();
            for slug in removed {
                self.covered.remove(&slug);
                self.invalidate_project(&slug);
            }
            for project in &catalogue.projects {
                if (matches!(target, PresentationTarget::Store)
                    || matches!(target,PresentationTarget::Project {project: slug} if slug == &project.slug))
                    && !self.covered.contains_key(&project.slug)
                {
                    self.covered.insert(project.slug.clone(), project.clone());
                    self.invalidate_project(&project.slug);
                }
            }
        }
        self.resolve(entries);
    }

    fn invalidate_project(&mut self, project: &str) {
        for (identity, paths) in &self.sources {
            if identity.scope.project == project {
                self.dirty.insert(identity.clone());
                self.availability_changed.extend(paths.iter().cloned());
            }
        }
    }

    pub(super) fn resolve(&mut self, entries: &BTreeMap<String, Arc<PresentationEntry>>) {
        for path in &self.availability_changed {
            if let Some(identity) = entries
                .get(path)
                .and_then(|entry| entry.derived.as_ref())
                .and_then(|record| record.identity.as_ref())
            {
                self.dirty.insert(identity.clone());
            }
        }
        for identity in std::mem::take(&mut self.dirty) {
            let mut paths = self.sources.get(&identity).cloned().unwrap_or_default();
            if self.covered.contains_key(&identity.scope.project) {
                for source in self
                    .sources
                    .get(&identity)
                    .into_iter()
                    .flatten()
                    .filter_map(|path| entries.get(path))
                    .filter_map(|entry| entry.derived.as_ref())
                {
                    for key in reference_keys(source) {
                        paths.extend(self.targets.get(&key).into_iter().flatten().cloned());
                    }
                }
                let edges = derive_relationships(
                    paths
                        .iter()
                        .filter_map(|path| entries.get(path))
                        .filter_map(|entry| entry.derived.as_ref()),
                )
                .into_iter()
                .filter(|edge| edge.source == identity)
                .collect::<Vec<_>>();
                if self
                    .resolved
                    .get(&identity)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    != edges.as_slice()
                {
                    self.updates.insert(identity.clone(), edges.clone());
                }
                if edges.is_empty() {
                    self.resolved.remove(&identity);
                } else {
                    self.resolved.insert(identity, edges);
                }
            } else if self.resolved.remove(&identity).is_some() {
                self.updates.insert(identity, Vec::new());
            }
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
        self.resolved.values().flatten()
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

fn remove_path<K: Ord>(map: &mut BTreeMap<K, BTreeSet<String>>, key: &K, path: &str) {
    if let Some(paths) = map.get_mut(key) {
        paths.remove(path);
        if paths.is_empty() {
            map.remove(key);
        }
    }
}
fn target_key(record: &DerivedRecord) -> TargetKey {
    let identity = record.identity.as_ref().expect("reference identity");
    (
        identity.scope.project.clone(),
        record.kind == Some(Kind::Decision),
        identity.identity.clone(),
    )
}
fn reference_keys(record: &DerivedRecord) -> impl Iterator<Item = TargetKey> + '_ {
    references(record)
        .into_iter()
        .flatten()
        .enumerate()
        .flat_map(move |(index, references)| {
            references.iter().map(move |reference| {
                (
                    record
                        .identity
                        .as_ref()
                        .expect("reference identity")
                        .scope
                        .project
                        .clone(),
                    index == 0,
                    reference.clone(),
                )
            })
        })
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
