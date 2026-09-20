use super::*;

impl Coordinator {
    pub(super) fn apply_load_event(&mut self, event: PresentationEvent) -> ProjectionChange {
        let Some(active) = self.active.as_ref() else {
            return ProjectionChange::None;
        };
        if event.generation() != active.generation || event.target() != &active.target {
            return ProjectionChange::None;
        }
        match &event {
            PresentationEvent::Catalogue {
                catalogue,
                coverage,
                progress,
                ..
            } => {
                let active = self.active.as_mut().expect("active load");
                active.catalogue = Some(catalogue.clone());
                active.coverage = Some(coverage.clone());
                active.progress = progress.clone();
                self.status = progress_message(&active.target, progress);
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
                for entry in entries {
                    active
                        .entry_targets
                        .insert(entry.path.clone(), active.target.clone());
                    if !self.has_complete {
                        self.changed.insert(entry.path.clone());
                    }
                    active.entries.insert(entry.path.clone(), entry.clone());
                }
                active.coverage = Some(coverage.clone());
                active.progress = progress.clone();
                self.status = progress_message(&active.target, progress);
                if self.has_complete {
                    ProjectionChange::None
                } else {
                    ProjectionChange::Partial
                }
            }
            PresentationEvent::Complete { .. } => {
                let active = self.active.take().expect("active load");
                self.complete_catalogue = active.catalogue.or(self.complete_catalogue.take());
                self.entries.retain(|path, entry| {
                    let remove =
                        target_contains(&active.target, path) && !active.entries.contains_key(path);
                    if remove {
                        self.relationships.entry_changed(path, Some(entry), None);
                        self.removed.push(path.clone());
                    }
                    !remove
                });
                for (path, mut entry) in active.entries {
                    if let Some(old) = self.entries.get(&path) {
                        if Arc::ptr_eq(old, &entry) {
                            continue;
                        }
                        if old.metadata == entry.metadata
                            && old.scope == entry.scope
                            && old.kind == entry.kind
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
                self.complete_entry_targets
                    .retain(|path, _| !target_contains(&active.target, path));
                self.complete_entry_targets.extend(active.entry_targets);
                self.relationships.promote(
                    &active.target,
                    self.complete_catalogue.as_ref(),
                    &self.entries,
                );
                self.has_complete = true;
                let later_observation =
                    self.observation.generation > active.started_observation_generation;
                self.status = if later_observation {
                    format!(
                        "{} changed during refresh. Refresh again.",
                        target_name(&active.target)
                    )
                } else {
                    String::new()
                };
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
                self.finish_failure(active, message.clone());
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
