use super::*;

impl Coordinator {
    pub(super) fn apply_load_event(&mut self, event: PresentationEvent) -> ProjectionChange {
        let Some(active) = self.active.as_ref() else {
            return ProjectionChange::None;
        };
        if event.generation() != active.generation || event.target() != &active.target {
            return ProjectionChange::None;
        }
        match event {
            PresentationEvent::Catalogue {
                catalogue,
                coverage,
                progress,
                ..
            } => {
                let active = self.active.as_mut().expect("active load");

                if !self.has_complete {
                    self.catalogue_facts = Some(CatalogueFacts::new(&catalogue));
                    self.catalogue_generation = self.catalogue_generation.wrapping_add(1);
                    self.catalogue_pending = true;
                }
                active.catalogue = Some(catalogue);
                active.coverage = Some(coverage);
                active.progress = progress;
                self.status = progress_message(&active.target, &active.progress);
                if self.has_complete {
                    ProjectionChange::None
                } else {
                    ProjectionChange::Partial
                }
            }
            PresentationEvent::Entries {
                entries,
                coverage,
                progress,
                ..
            } => {
                let active = self.active.as_mut().expect("active load");
                let selected_scope = target_scope(&active.target, active.catalogue.as_ref());
                for entry in entries {
                    if !entry_in_target(&entry, &active.target, selected_scope) {
                        continue;
                    }
                    if !self.has_complete {
                        self.changed.insert(entry.path.clone());
                    }
                    active.entries.insert(entry.path.clone(), entry);
                }
                active.coverage = Some(coverage);
                active.progress = progress;
                self.status = progress_message(&active.target, &active.progress);
                if self.has_complete {
                    ProjectionChange::None
                } else {
                    ProjectionChange::Partial
                }
            }
            PresentationEvent::Complete { .. } => {
                let active = self.active.take().expect("active load");
                if let Some(catalogue) = active.catalogue {
                    if self.has_complete && self.complete_catalogue.as_ref() != Some(&catalogue) {
                        self.catalogue_facts = Some(CatalogueFacts::new(&catalogue));
                        self.catalogue_generation = self.catalogue_generation.wrapping_add(1);
                        self.catalogue_pending = true;
                    }
                    self.complete_catalogue = Some(catalogue);
                }
                let selected_scope = target_scope(&active.target, self.complete_catalogue.as_ref());
                let previous_paths = target_paths(&self.entries, &active.target, selected_scope);
                for path in previous_paths.difference(&active.entries.keys().cloned().collect()) {
                    if let Some(entry) = self.entries.remove(path) {
                        self.relationships.entry_changed(path, Some(&entry), None);
                        self.complete_entry_targets.remove(path);
                        self.removed.push(path.clone());
                    }
                }
                let target_owner = Arc::new(active.target.clone());
                for (path, mut entry) in active.entries {
                    self.complete_entry_targets
                        .insert(path.clone(), Arc::clone(&target_owner));
                    if let Some(old) = self.entries.get(&path) {
                        if Arc::ptr_eq(old, &entry) {
                            continue;
                        }
                        if old.metadata == entry.metadata
                            && old.scope == entry.scope
                            && old.kind == entry.kind
                            && old.content_handle == entry.content_handle
                            && old.classification == entry.classification
                            && old.identity == entry.identity
                            && old.summary == entry.summary
                            && old.diagnostics == entry.diagnostics
                            && old.progress == entry.progress
                            && old.relationships == entry.relationships
                            && old.boards == entry.boards
                            && old.derived == entry.derived
                            && matches!(entry.body, PresentationFact::Unavailable)
                            && matches!(old.body, PresentationFact::Available(_))
                        {
                            entry = old.clone();
                        }
                        if old.as_ref() == entry.as_ref() {
                            continue;
                        }
                    }
                    self.relationships.entry_changed(
                        &path,
                        self.entries.get(&path).map(AsRef::as_ref),
                        Some(&entry),
                    );
                    self.changed.insert(path.clone());
                    self.entries.insert(path, entry);
                }
                self.relationships.promote(
                    &active.target,
                    self.complete_catalogue.as_ref(),
                    &self.entries,
                );
                self.has_complete = true;
                // Scoped watcher floors, not the global handoff generation, decide
                // whether this completed target remains stale.
                self.status.clear();
                self.report(RefreshReport::Succeeded {
                    generation: active.generation,
                    target: active.target,
                    started_observation_generation: active.started_observation_generation,
                    completed_observation_generation: self.observation.generation,
                });
                ProjectionChange::Complete
            }
            PresentationEvent::Failure { message, .. } => {
                let active = self.active.take().expect("active load");
                self.finish_failure(active, message);
                ProjectionChange::None
            }
        }
    }

    pub(super) fn finish_failure(&mut self, active: ActiveLoad, message: String) {
        self.status = if self.has_complete {
            format!("{} refresh failed: {message}", target_name(&active.target))
        } else {
            format!("Loading failed: {message}")
        };
        self.report(RefreshReport::Failed {
            generation: active.generation,
            target: active.target,
            started_observation_generation: active.started_observation_generation,
            completed_observation_generation: self.observation.generation,
            message,
        });
    }
}

fn target_paths(
    entries: &BTreeMap<String, Arc<PresentationEntry>>,
    target: &PresentationTarget,
    selected_scope: Option<&str>,
) -> BTreeSet<String> {
    let prefix = match target {
        PresentationTarget::Store => return entries.keys().cloned().collect(),
        PresentationTarget::Project { project } => format!("projects/{project}"),
        PresentationTarget::Investigation { path, .. } => path.clone(),
    };
    let descendant = format!("{prefix}/");
    entries
        .get_key_value(&prefix)
        .into_iter()
        .map(|(path, _)| path.clone())
        .chain(
            entries
                .range(descendant.clone()..)
                .take_while(|(path, _)| path.starts_with(&descendant))
                .map(|(path, _)| path.clone()),
        )
        .filter(|path| {
            selected_scope.is_none_or(|identity| {
                entries[path]
                    .scope
                    .as_ref()
                    .is_none_or(|scope| scope.investigation.as_deref() == Some(identity))
            })
        })
        .collect()
}

fn target_scope<'a>(
    target: &PresentationTarget,
    catalogue: Option<&'a PresentationCatalogue>,
) -> Option<&'a str> {
    let PresentationTarget::Investigation { project, path } = target else {
        return None;
    };
    catalogue
        .and_then(|catalogue| {
            catalogue
                .projects
                .iter()
                .find(|candidate| &candidate.slug == project)
        })
        .and_then(|project| {
            project
                .investigations
                .iter()
                .find(|scope| &scope.path == path)
        })
        .map(|scope| scope.identity.as_str())
}

fn entry_in_target(
    entry: &PresentationEntry,
    target: &PresentationTarget,
    selected_scope: Option<&str>,
) -> bool {
    match target {
        PresentationTarget::Store => true,
        PresentationTarget::Project { project } => entry
            .path
            .strip_prefix("projects/")
            .is_some_and(|relative| relative.split('/').next() == Some(project.as_str())),
        PresentationTarget::Investigation { path, .. } => {
            (entry.path == *path
                || entry
                    .path
                    .strip_prefix(path)
                    .is_some_and(|rest| rest.starts_with('/')))
                && selected_scope.is_none_or(|identity| {
                    entry
                        .scope
                        .as_ref()
                        .is_none_or(|scope| scope.investigation.as_deref() == Some(identity))
                })
        }
    }
}
