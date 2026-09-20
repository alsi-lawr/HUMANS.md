use super::*;

impl Coordinator {
    pub(crate) fn request_content(&mut self, path: Option<&str>) -> bool {
        let mut changed = false;
        if self
            .content
            .as_ref()
            .is_some_and(|content| Some(content.path.as_str()) != path)
        {
            self.content = None;
            changed = true;
        }
        if self
            .attempted_content
            .as_ref()
            .is_some_and(|(_, handle)| Some(handle.path()) != path)
        {
            self.attempted_content = None;
        }
        let Some(path) = path else {
            changed |= self.content_status.take().is_some();
            return changed;
        };
        let Some(entry) = self.visible_entry(path) else {
            return changed;
        };
        if matches!(entry.body, PresentationFact::Available(_)) {
            changed |= self.content_status.take().is_some();
            return changed;
        }
        let Some(handle) = entry.content_handle.clone() else {
            changed |= self.content_status.take().is_some();
            return changed;
        };
        let Some(target) = self.entry_target(path) else {
            return changed;
        };
        if self.attempted_content.as_ref() == Some(&(target.clone(), handle.clone())) {
            return changed;
        }
        let generation = self.next_generation();
        let stream = match self.session.fetch_content(PresentationContentRequest {
            generation,
            target: target.clone(),
            selector: PresentationContentSelector::Handle {
                handle: handle.clone(),
            },
        }) {
            Ok(stream) => stream,
            Err(error) => {
                self.content_status = Some(format!("Content load failed for {path}: {error}"));
                return true;
            }
        };
        self.attempted_content = Some((target.clone(), handle));
        self.content = Some(ActiveContent {
            generation,
            target,
            path: path.into(),
            stream,
        });
        self.content_status = Some(format!("Loading {path}…"));
        true
    }

    pub(super) fn apply_content_event(&mut self, event: PresentationContentEvent) -> bool {
        let Some(active) = self.content.as_ref() else {
            return false;
        };
        let matches = match &event {
            PresentationContentEvent::Pending {
                generation,
                target,
                path,
            }
            | PresentationContentEvent::Failure {
                generation,
                target,
                path: Some(path),
                ..
            } => {
                *generation == active.generation && target == &active.target && path == &active.path
            }
            PresentationContentEvent::Loaded {
                generation,
                target,
                entry,
            } => {
                *generation == active.generation
                    && target == &active.target
                    && entry.path == active.path
            }
            PresentationContentEvent::Failure { path: None, .. } => false,
        };
        if !matches {
            return false;
        }
        match &event {
            PresentationContentEvent::Loaded { entry, .. } => {
                self.changed.insert(entry.path.clone());
                let entry = Arc::new(entry.as_ref().clone());
                if let Some(load) = self.active.as_mut()
                    && load.entries.contains_key(&entry.path)
                {
                    load.entries.insert(entry.path.clone(), entry.clone());
                }
                self.relationships.entry_changed(
                    &entry.path,
                    self.entries.get(&entry.path).map(AsRef::as_ref),
                    Some(&entry),
                );
                self.entries.insert(entry.path.clone(), entry);
                self.relationships.resolve(&self.entries);
            }
            PresentationContentEvent::Pending { .. } | PresentationContentEvent::Failure { .. } => {
            }
        }
        match &event {
            PresentationContentEvent::Pending { path, .. } => {
                self.content_status = Some(format!("Loading {path}…"));
            }
            PresentationContentEvent::Loaded { .. } => {
                self.content_status = None;
                self.content = None;
            }
            PresentationContentEvent::Failure { path, message, .. } => {
                self.content_status = Some(format!(
                    "Content load failed{}: {message}",
                    path.as_deref()
                        .map(|path| format!(" for {path}"))
                        .unwrap_or_default()
                ));
                self.content = None;
            }
        }
        true
    }
}
