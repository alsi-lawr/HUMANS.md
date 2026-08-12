use casefile_core::{
    CasefileSnapshot, Classification, EntrySnapshot, RecordDraft, RecordSummary, Revision,
};
use casefile_store::{
    ActivationState, DerivedRecord, DerivedSnapshot, DerivedStrategy, DerivedStrategyBinding,
    PresentationCache, PresentationCatalogue, PresentationContentEvent, PresentationContentHandle,
    PresentationContentRequest, PresentationContentSelector, PresentationContentStream,
    PresentationCoverage, PresentationEntry, PresentationEvent, PresentationFact,
    PresentationLoadRequest, PresentationProgress, PresentationSession, PresentationStream,
    PresentationTarget, RecordScope, ScanResult, ScopedIdentity, StoreError, StrategyBindingState,
    presentation_revision,
};
use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, Sender, TryRecvError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshMinimumScope {
    Contextual,
    Store { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshObservation {
    pub generation: u64,
    pub minimum_scope: RefreshMinimumScope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshReport {
    Started {
        generation: u64,
        target: PresentationTarget,
        observation_generation: u64,
    },
    Succeeded {
        generation: u64,
        target: PresentationTarget,
        started_observation_generation: u64,
        completed_observation_generation: u64,
    },
    Failed {
        generation: u64,
        target: PresentationTarget,
        started_observation_generation: u64,
        completed_observation_generation: u64,
        message: String,
    },
}

pub struct ObservationHandoff {
    observations: Receiver<RefreshObservation>,
    reports: Sender<RefreshReport>,
}

impl ObservationHandoff {
    pub fn new(observations: Receiver<RefreshObservation>, reports: Sender<RefreshReport>) -> Self {
        Self {
            observations,
            reports,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ProjectionChange {
    #[default]
    None,
    Partial,
    Content,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CoordinatorUpdate {
    pub(crate) dirty: bool,
    pub(crate) projection: ProjectionChange,
}

impl CoordinatorUpdate {
    fn merge(&mut self, projection: ProjectionChange) {
        self.dirty = true;
        self.projection = self.projection.max(projection);
    }
}

struct ActiveLoad {
    generation: u64,
    target: PresentationTarget,
    stream: PresentationStream,
    catalogue: Option<PresentationCatalogue>,
    entries: BTreeMap<String, PresentationEntry>,
    entry_targets: BTreeMap<String, PresentationTarget>,
    started_observation_generation: u64,
    progress: PresentationProgress,
    coverage: Option<PresentationCoverage>,
    initial: bool,
}

struct ActiveContent {
    generation: u64,
    target: PresentationTarget,
    path: String,
    stream: PresentationContentStream,
}

pub(crate) struct UiProjection {
    pub(crate) scan: ScanResult,
    pub(crate) derived: DerivedSnapshot,
    pub(crate) provisional: bool,
    pub(crate) unavailable: BTreeMap<String, String>,
}

pub(crate) struct Coordinator {
    session: PresentationSession,
    cache: PresentationCache,
    complete_catalogue: Option<PresentationCatalogue>,
    complete_entry_targets: BTreeMap<String, PresentationTarget>,
    has_complete: bool,
    active: Option<ActiveLoad>,
    content: Option<ActiveContent>,
    attempted_content: Option<(PresentationTarget, PresentationContentHandle)>,
    next_generation: u64,
    observation: RefreshObservation,
    handoff: Option<ObservationHandoff>,
    status: String,
    content_status: Option<String>,
}

impl Coordinator {
    pub(crate) fn start(
        session: PresentationSession,
        handoff: Option<ObservationHandoff>,
    ) -> Result<Self, StoreError> {
        Self::start_at(
            session,
            handoff,
            RefreshObservation {
                generation: 0,
                minimum_scope: RefreshMinimumScope::Contextual,
            },
        )
    }

    pub(crate) fn start_at(
        session: PresentationSession,
        handoff: Option<ObservationHandoff>,
        observation: RefreshObservation,
    ) -> Result<Self, StoreError> {
        let mut coordinator = Self {
            session,
            cache: PresentationCache::default(),
            complete_catalogue: None,
            complete_entry_targets: BTreeMap::new(),
            has_complete: false,
            active: None,
            content: None,
            attempted_content: None,
            next_generation: 0,
            observation,
            handoff,
            status: String::new(),
            content_status: None,
        };
        coordinator.start_target(PresentationTarget::Store, true)?;
        Ok(coordinator)
    }

    pub(crate) fn refresh(&mut self, target: PresentationTarget) -> Result<(), String> {
        self.start_target(target, false)
            .map_err(|error| format!("Refresh could not start: {error}"))
    }

    fn start_target(
        &mut self,
        target: PresentationTarget,
        initial: bool,
    ) -> Result<(), StoreError> {
        let generation = self.next_generation();
        let stream = self.session.load(PresentationLoadRequest {
            generation,
            target: target.clone(),
        })?;
        let started_observation_generation = self.observation.generation;
        self.report(RefreshReport::Started {
            generation,
            target: target.clone(),
            observation_generation: started_observation_generation,
        });
        self.active = Some(ActiveLoad {
            generation,
            target: target.clone(),
            stream,
            catalogue: None,
            entries: BTreeMap::new(),
            entry_targets: BTreeMap::new(),
            started_observation_generation,
            progress: PresentationProgress {
                completed: 0,
                total: None,
            },
            coverage: None,
            initial,
        });
        self.status = if initial {
            "Loading Store catalogue...".into()
        } else {
            format!("Refreshing {} from disk...", target_name(&target))
        };
        self.content = None;
        self.attempted_content = None;
        self.content_status = None;
        Ok(())
    }

    pub(crate) fn observe(&mut self, observation: RefreshObservation) -> bool {
        if observation.generation < self.observation.generation || observation == self.observation {
            return false;
        }
        self.observation = observation;
        true
    }

    pub(crate) fn drain(&mut self) -> CoordinatorUpdate {
        let mut update = CoordinatorUpdate::default();
        if let Some(handoff) = &self.handoff {
            let mut observations = Vec::new();
            while let Ok(observation) = handoff.observations.try_recv() {
                observations.push(observation);
            }
            for observation in observations {
                if self.observe(observation) {
                    update.merge(ProjectionChange::None);
                }
            }
        }
        loop {
            let event = match self.active.as_ref().map(|active| active.stream.try_recv()) {
                Some(Ok(event)) => event,
                Some(Err(TryRecvError::Disconnected)) => {
                    if let Some(active) = self.active.take() {
                        self.finish_failure(
                            active,
                            "Casefile presentation loading stopped unexpectedly".into(),
                        );
                        update.merge(ProjectionChange::None);
                    }
                    break;
                }
                Some(Err(TryRecvError::Empty)) | None => break,
            };
            update.merge(self.apply_load_event(event));
        }
        loop {
            let event = match self
                .content
                .as_ref()
                .map(|content| content.stream.try_recv())
            {
                Some(Ok(event)) => event,
                Some(Err(TryRecvError::Disconnected)) => {
                    if let Some(content) = self.content.take() {
                        self.content_status = Some(format!(
                            "Content load failed for {}: worker stopped unexpectedly",
                            content.path
                        ));
                        update.merge(ProjectionChange::Content);
                    }
                    break;
                }
                Some(Err(TryRecvError::Empty)) | None => break,
            };
            if self.apply_content_event(event) {
                update.merge(ProjectionChange::Content);
            }
        }
        update
    }

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
        let Some(entry) = self.visible_entry(path).cloned() else {
            return changed;
        };
        if matches!(entry.body, PresentationFact::Available(_)) {
            changed |= self.content_status.take().is_some();
            return changed;
        }
        let Some(handle) = entry.content_handle else {
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
        self.content_status = Some(format!("Loading selected content: {path}"));
        true
    }

    pub(crate) fn projection(&self) -> UiProjection {
        let catalogue = self.visible_catalogue();
        let entries = self.visible_entries();
        build_projection(catalogue, entries, !self.has_complete)
    }

    pub(crate) fn status(&self) -> &str {
        self.content_status.as_deref().unwrap_or(&self.status)
    }

    pub(crate) fn catalogue(&self) -> Option<&PresentationCatalogue> {
        self.visible_catalogue()
    }

    pub(crate) fn investigation_target(
        &self,
        project: &str,
        identity: &str,
    ) -> Option<PresentationTarget> {
        self.visible_catalogue()?
            .projects
            .iter()
            .find(|candidate| candidate.slug == project)?
            .investigations
            .iter()
            .find(|candidate| candidate.identity == identity)
            .map(|investigation| PresentationTarget::Investigation {
                project: project.into(),
                path: investigation.path.clone(),
            })
    }

    fn apply_load_event(&mut self, event: PresentationEvent) -> ProjectionChange {
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
                self.cache.apply(&event);
                let active = self.active.as_mut().expect("active load");
                active.catalogue = Some(catalogue.clone());
                active.coverage = Some(coverage.clone());
                active.progress = progress.clone();
                self.status = format!(
                    "{} catalogue ready; records are loading...",
                    target_name(&active.target)
                );
                if self.has_complete {
                    ProjectionChange::None
                } else {
                    ProjectionChange::Partial
                }
            }
            PresentationEvent::Investigations {
                projects,
                coverage,
                progress,
                ..
            } => {
                self.cache.apply(&event);
                let active = self.active.as_mut().expect("active load");
                let catalogue = active.catalogue.as_mut().expect("projects catalogue");
                catalogue.projects = projects.clone();
                active.coverage = Some(coverage.clone());
                active.progress = progress.clone();
                self.status = format!(
                    "{} investigations ready; records are loading...",
                    target_name(&active.target)
                );
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
                self.cache.apply(&event);
                let active = self.active.as_mut().expect("active load");
                for entry in entries {
                    active
                        .entry_targets
                        .insert(entry.path.clone(), active.target.clone());
                    active.entries.insert(entry.path.clone(), entry.clone());
                }
                active.coverage = Some(coverage.clone());
                active.progress = progress.clone();
                self.status = progress_message(&active.target, progress, coverage);
                if self.has_complete {
                    ProjectionChange::None
                } else {
                    ProjectionChange::Partial
                }
            }
            PresentationEvent::Complete { .. } => {
                self.cache.apply(&event);
                let active = self.active.take().expect("active load");
                self.complete_catalogue =
                    active.catalogue.clone().or(self.complete_catalogue.take());
                self.complete_entry_targets
                    .retain(|path, _| !target_contains(&active.target, path));
                self.complete_entry_targets
                    .extend(active.entry_targets.clone());
                self.has_complete = true;
                let later_observation =
                    self.observation.generation > active.started_observation_generation;
                self.status = if later_observation {
                    format!(
                        "{} refresh complete; a newer observation remains uncovered.",
                        target_name(&active.target)
                    )
                } else if active.initial {
                    "Store presentation complete.".into()
                } else {
                    format!("{} refresh complete.", target_name(&active.target))
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
                self.cache.apply(&event);
                let active = self.active.take().expect("active load");
                self.finish_failure(active, message.clone());
                ProjectionChange::None
            }
        }
    }

    fn finish_failure(&mut self, active: ActiveLoad, message: String) {
        self.status = if self.has_complete {
            format!(
                "{} refresh failed; last complete data retained: {message}",
                target_name(&active.target)
            )
        } else {
            format!("Initial presentation failed: {message}")
        };
        self.report(RefreshReport::Failed {
            generation: active.generation,
            target: active.target,
            started_observation_generation: active.started_observation_generation,
            completed_observation_generation: self.observation.generation,
            message,
        });
    }

    fn apply_content_event(&mut self, event: PresentationContentEvent) -> bool {
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
        self.cache.apply_content(&event);
        if let Some(load) = self.active.as_mut() {
            match &event {
                PresentationContentEvent::Loaded { entry, .. } => {
                    if load.entries.contains_key(&entry.path) {
                        load.entries
                            .insert(entry.path.clone(), entry.as_ref().clone());
                    }
                }
                PresentationContentEvent::Pending { path, .. }
                | PresentationContentEvent::Failure {
                    path: Some(path), ..
                } => {
                    if let Some(entry) = load.entries.get_mut(path) {
                        invalidate_content(entry);
                    }
                }
                PresentationContentEvent::Failure { path: None, .. } => {}
            }
        }
        match &event {
            PresentationContentEvent::Pending { path, .. } => {
                self.content_status = Some(format!("Loading selected content: {path}"));
            }
            PresentationContentEvent::Loaded { entry, .. } => {
                self.content_status = Some(format!("Selected content loaded: {}", entry.path));
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

    fn visible_catalogue(&self) -> Option<&PresentationCatalogue> {
        self.complete_catalogue.as_ref().or_else(|| {
            self.active
                .as_ref()
                .and_then(|active| active.catalogue.as_ref())
        })
    }

    fn visible_entries(&self) -> Vec<PresentationEntry> {
        if self.has_complete {
            return self.cache.entries().cloned().collect();
        }
        let mut entries = self
            .active
            .as_ref()
            .map(|active| active.entries.clone())
            .unwrap_or_default();
        entries.extend(
            self.cache
                .entries()
                .cloned()
                .map(|entry| (entry.path.clone(), entry)),
        );
        entries.into_values().collect()
    }

    fn visible_entry(&self, path: &str) -> Option<&PresentationEntry> {
        if self.has_complete {
            return self.cache.get(path);
        }
        self.cache.get(path).or_else(|| {
            self.active
                .as_ref()
                .and_then(|active| active.entries.get(path))
        })
    }

    fn entry_target(&self, path: &str) -> Option<PresentationTarget> {
        if self.has_complete {
            return self.complete_entry_targets.get(path).cloned();
        }
        self.active
            .as_ref()
            .and_then(|active| active.entry_targets.get(path))
            .cloned()
    }

    fn next_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.saturating_add(1);
        self.next_generation
    }

    fn report(&self, report: RefreshReport) {
        if let Some(handoff) = &self.handoff {
            let _ = handoff.reports.send(report);
        }
    }
}

fn build_projection(
    catalogue: Option<&PresentationCatalogue>,
    entries: Vec<PresentationEntry>,
    provisional: bool,
) -> UiProjection {
    let activation = catalogue
        .map(|catalogue| catalogue.activation)
        .unwrap_or(ActivationState::Unactivated);
    let investigation_roots = catalogue
        .map(|catalogue| {
            catalogue
                .projects
                .iter()
                .map(|project| {
                    (
                        project.slug.clone(),
                        project
                            .investigations
                            .iter()
                            .map(|investigation| investigation.identity.clone())
                            .collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut diagnostics = catalogue
        .map(|catalogue| catalogue.diagnostics.clone())
        .unwrap_or_default();
    for entry in &entries {
        if let PresentationFact::Available(values) = &entry.diagnostics {
            diagnostics.extend(values.clone());
        }
    }
    diagnostics.sort_by(|left, right| {
        (&left.path, &left.code, &left.message).cmp(&(&right.path, &right.code, &right.message))
    });
    diagnostics.dedup();
    let snapshots = entries.iter().map(snapshot_entry).collect::<Vec<_>>();
    let revision = presentation_revision(&entries);
    let scan = ScanResult {
        activation,
        investigation_roots,
        snapshot: CasefileSnapshot {
            revision: revision.clone(),
            entries: snapshots,
        },
        diagnostics: diagnostics.clone(),
    };
    let derived = presentation_derived(&revision, &entries, &diagnostics);
    let unavailable = entries
        .iter()
        .filter_map(|entry| {
            let mut fields = Vec::new();
            if matches!(entry.classification, PresentationFact::Unavailable) {
                fields.push("classification");
            }
            if matches!(entry.summary, PresentationFact::Unavailable) {
                fields.push("summary");
            }
            if matches!(entry.diagnostics, PresentationFact::Unavailable) {
                fields.push("diagnostics");
            }
            if matches!(entry.progress, PresentationFact::Unavailable) {
                fields.push("progress");
            }
            if matches!(entry.relationships, PresentationFact::Unavailable) {
                fields.push("relationships");
            }
            if matches!(entry.boards, PresentationFact::Unavailable) {
                fields.push("boards");
            }
            if matches!(entry.body, PresentationFact::Unavailable) {
                fields.push("content");
            }
            (!fields.is_empty()).then(|| (entry.path.clone(), fields.join(", ")))
        })
        .collect();
    UiProjection {
        scan,
        derived,
        provisional,
        unavailable,
    }
}

fn snapshot_entry(entry: &PresentationEntry) -> EntrySnapshot {
    EntrySnapshot {
        path: entry.path.clone(),
        classification: match entry.classification {
            PresentationFact::Available(value) => value,
            PresentationFact::Unavailable => Classification::Raw,
        },
        kind: entry.kind,
        identity: match &entry.identity {
            PresentationFact::Available(value) => value.clone(),
            PresentationFact::Unavailable => None,
        },
        content_revision: entry.metadata.revision.clone(),
        summary: match &entry.summary {
            PresentationFact::Available(Some(summary)) => Some(summary.record.clone()),
            PresentationFact::Available(None) | PresentationFact::Unavailable => None,
        },
        original_bytes: match &entry.body {
            PresentationFact::Available(bytes) => bytes.clone(),
            PresentationFact::Unavailable => Vec::new(),
        },
    }
}

fn presentation_derived(
    revision: &Revision,
    entries: &[PresentationEntry],
    diagnostics: &[casefile_core::Diagnostic],
) -> DerivedSnapshot {
    let records = entries
        .iter()
        .filter_map(|entry| {
            let classification = match entry.classification {
                PresentationFact::Available(value) => value,
                PresentationFact::Unavailable => return None,
            };
            let summary = match &entry.summary {
                PresentationFact::Available(value) => value.as_ref(),
                PresentationFact::Unavailable => None,
            };
            let body = match &entry.body {
                PresentationFact::Available(value) => Some(value.as_slice()),
                PresentationFact::Unavailable => None,
            };
            let content = body.and_then(|bytes| String::from_utf8(bytes.to_vec()).ok());
            let draft = content.as_deref().and_then(|text| {
                entry
                    .kind
                    .filter(|kind| kind.is_writable())
                    .and_then(|kind| casefile_core::parse_draft(&entry.path, kind, text).ok())
            });
            let (work_item, board) = match draft {
                Some(RecordDraft::Ticket(item) | RecordDraft::Epic(item)) => (Some(item), None),
                Some(RecordDraft::Board(board)) => (None, Some(board)),
                None => (None, None),
            };
            let scope = entry.scope.as_ref().map(|scope| RecordScope {
                project: scope.project.clone(),
                investigation: scope.investigation.clone(),
            });
            let identity = match &entry.identity {
                PresentationFact::Available(Some(identity)) => {
                    scope.clone().map(|scope| ScopedIdentity {
                        scope,
                        identity: identity.clone(),
                    })
                }
                PresentationFact::Available(None) | PresentationFact::Unavailable => None,
            };
            let title = summary.map_or_else(
                || {
                    identity
                        .as_ref()
                        .map(|identity| identity.identity.clone())
                        .unwrap_or_else(|| entry.path.clone())
                },
                |summary| summary.title.clone(),
            );
            let strategy = match (summary.map(|summary| &summary.record), content.as_deref()) {
                (Some(RecordSummary::Strategy { .. }), Some(text)) => {
                    casefile_core::parse_strategy_projection(&entry.path, text)
                        .ok()
                        .flatten()
                        .map(|matrix| DerivedStrategy {
                            matrix,
                            binding: None,
                        })
                }
                _ => None,
            };
            let strategy_binding =
                match (summary.map(|summary| &summary.record), content.as_deref()) {
                    (Some(RecordSummary::StrategyBinding { .. }), Some(text)) => {
                        casefile_core::parse_strategy_binding(&entry.path, text)
                            .ok()
                            .and_then(|summary| match summary {
                                RecordSummary::StrategyBinding { binding } => Some(binding),
                                _ => None,
                            })
                            .map(|binding| DerivedStrategyBinding {
                                binding,
                                state: StrategyBindingState::Pending,
                            })
                    }
                    _ => None,
                };
            Some(DerivedRecord {
                path: entry.path.clone(),
                scope,
                classification,
                kind: entry.kind,
                identity,
                title: title.clone(),
                content: content.clone(),
                rendered_markdown: content
                    .as_deref()
                    .filter(|_| entry.path.ends_with(".md"))
                    .map(casefile_core::render_markdown_html),
                search_text: format!("{title}\n{}", content.as_deref().unwrap_or_default()),
                work_item,
                progress: match &entry.progress {
                    PresentationFact::Available(value) => value.clone(),
                    PresentationFact::Unavailable => None,
                },
                board,
                strategy,
                strategy_binding,
            })
        })
        .collect();
    let mut relationships = Vec::new();
    let mut boards = Vec::new();
    for entry in entries {
        if let PresentationFact::Available(values) = &entry.relationships {
            for value in values {
                if !relationships.contains(value) {
                    relationships.push(value.clone());
                }
            }
        }
        if let PresentationFact::Available(values) = &entry.boards {
            for value in values {
                if !boards.contains(value) {
                    boards.push(value.clone());
                }
            }
        }
    }
    DerivedSnapshot {
        source_revision: revision.clone(),
        records,
        relationships,
        boards,
        diagnostics: diagnostics.to_vec(),
    }
}

fn invalidate_content(entry: &mut PresentationEntry) {
    entry.body = PresentationFact::Unavailable;
    if entry.kind == Some(casefile_core::Kind::Evidence) {
        entry.classification = PresentationFact::Unavailable;
        entry.summary = PresentationFact::Unavailable;
        entry.diagnostics = PresentationFact::Unavailable;
        entry.relationships = PresentationFact::Unavailable;
        entry.boards = PresentationFact::Unavailable;
    }
}

fn progress_message(
    target: &PresentationTarget,
    progress: &PresentationProgress,
    coverage: &PresentationCoverage,
) -> String {
    let total = progress
        .total
        .map(|total| total.to_string())
        .unwrap_or_else(|| "?".into());
    format!(
        "Loading {}: {}/{} entries; payload {:?}, facts {:?} (provisional).",
        target_name(target),
        progress.completed,
        total,
        coverage.payload,
        coverage.facts,
    )
}

fn target_name(target: &PresentationTarget) -> String {
    match target {
        PresentationTarget::Store => "Store".into(),
        PresentationTarget::Project { project } => format!("project {project}"),
        PresentationTarget::Investigation { project, path } => {
            format!("investigation {project} / {path}")
        }
    }
}

fn target_contains(target: &PresentationTarget, path: &str) -> bool {
    let root = match target {
        PresentationTarget::Store => return true,
        PresentationTarget::Project { project } => format!("projects/{project}"),
        PresentationTarget::Investigation { path, .. } => path.clone(),
    };
    path == root || path.starts_with(&(root + "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use casefile_store::Store;
    use std::{
        fs,
        path::Path,
        sync::mpsc,
        time::{Duration, Instant},
    };
    use tempfile::TempDir;

    const EVIDENCE: &str = "projects/demo/investigations/sample/evidence/observation.md";

    #[test]
    fn catalogue_is_navigable_before_completion_and_post_start_observation_is_reported() {
        let root = fixture();
        let store = Store::open(root.path()).expect("store");
        let (observation_sender, observation_receiver) = mpsc::channel();
        let (report_sender, report_receiver) = mpsc::channel();
        let mut coordinator = Coordinator::start(
            store.presentation_session(),
            Some(ObservationHandoff::new(observation_receiver, report_sender)),
        )
        .expect("coordinator");

        let first = coordinator
            .active
            .as_ref()
            .expect("active")
            .stream
            .recv()
            .expect("catalogue");
        assert!(matches!(first, PresentationEvent::Catalogue { .. }));
        assert_eq!(
            coordinator.apply_load_event(first),
            ProjectionChange::Partial
        );
        let projection = coordinator.projection();
        assert!(projection.provisional);
        assert!(
            projection
                .scan
                .investigation_roots
                .get("demo")
                .is_some_and(Vec::is_empty)
        );
        let investigations = coordinator
            .active
            .as_ref()
            .expect("active")
            .stream
            .recv()
            .expect("investigations");
        assert!(matches!(
            investigations,
            PresentationEvent::Investigations { .. }
        ));
        assert_eq!(
            coordinator.apply_load_event(investigations),
            ProjectionChange::Partial
        );
        assert_eq!(
            coordinator
                .projection()
                .scan
                .investigation_roots
                .get("demo"),
            Some(&vec!["sample".into()])
        );
        let tickets = coordinator
            .active
            .as_ref()
            .expect("active")
            .stream
            .recv()
            .expect("tickets");
        assert!(matches!(tickets, PresentationEvent::Entries { .. }));
        assert_eq!(
            coordinator.apply_load_event(tickets),
            ProjectionChange::Partial
        );
        let early = coordinator
            .active
            .as_ref()
            .expect("active")
            .stream
            .recv()
            .expect("early entries");
        assert!(matches!(early, PresentationEvent::Entries { .. }));
        assert_eq!(
            coordinator.apply_load_event(early),
            ProjectionChange::Partial
        );
        let projection = coordinator.projection();
        assert!(
            projection
                .scan
                .snapshot
                .entries
                .iter()
                .any(|entry| entry.path == EVIDENCE)
        );
        assert!(
            projection
                .unavailable
                .get(EVIDENCE)
                .is_some_and(
                    |fields| fields.contains("classification") && fields.contains("content")
                )
        );
        assert!(matches!(
            report_receiver.recv().expect("start report"),
            RefreshReport::Started {
                generation: 1,
                observation_generation: 0,
                ..
            }
        ));

        observation_sender
            .send(RefreshObservation {
                generation: 1,
                minimum_scope: RefreshMinimumScope::Contextual,
            })
            .expect("observation");
        assert!(coordinator.drain().dirty);
        finish_active(&mut coordinator);

        assert!(!coordinator.projection().provisional);
        assert!(
            coordinator
                .status()
                .contains("newer observation remains uncovered")
        );
        assert!(matches!(
            report_receiver.recv().expect("success report"),
            RefreshReport::Succeeded {
                started_observation_generation: 0,
                completed_observation_generation: 1,
                ..
            }
        ));
    }

    #[test]
    fn store_minimum_scope_allows_contextual_refresh_and_reports_its_exact_coverage() {
        let root = fixture();
        let store = Store::open(root.path()).expect("store");
        let (_observation_sender, observation_receiver) = mpsc::channel();
        let (report_sender, report_receiver) = mpsc::channel();
        let mut coordinator = Coordinator::start(
            store.presentation_session(),
            Some(ObservationHandoff::new(observation_receiver, report_sender)),
        )
        .expect("coordinator");
        finish_active(&mut coordinator);
        assert!(matches!(
            report_receiver.recv().expect("initial start report"),
            RefreshReport::Started {
                target: PresentationTarget::Store,
                ..
            }
        ));
        assert!(matches!(
            report_receiver.recv().expect("initial success report"),
            RefreshReport::Succeeded {
                target: PresentationTarget::Store,
                ..
            }
        ));
        assert!(coordinator.observe(RefreshObservation {
            generation: 7,
            minimum_scope: RefreshMinimumScope::Store {
                reason: "activation changed".into(),
            },
        }));
        let generation = coordinator.next_generation;
        let target = PresentationTarget::Project {
            project: "demo".into(),
        };

        coordinator
            .refresh(target.clone())
            .expect("contextual Project refresh");
        assert_eq!(coordinator.next_generation, generation + 1);
        assert_eq!(
            coordinator.active.as_ref().map(|active| &active.target),
            Some(&target)
        );
        assert!(matches!(
            report_receiver.recv().expect("contextual start report"),
            RefreshReport::Started {
                target: PresentationTarget::Project { ref project },
                observation_generation: 7,
                ..
            } if project == "demo"
        ));
        finish_active(&mut coordinator);
        assert!(matches!(
            report_receiver.recv().expect("contextual success report"),
            RefreshReport::Succeeded {
                target: PresentationTarget::Project { ref project },
                started_observation_generation: 7,
                completed_observation_generation: 7,
                ..
            } if project == "demo"
        ));
        assert!(matches!(
            coordinator.observation.minimum_scope,
            RefreshMinimumScope::Store { .. }
        ));
    }

    #[test]
    fn obsolete_generation_is_discarded_and_refresh_failure_keeps_complete_data() {
        let root = fixture();
        let store = Store::open(root.path()).expect("store");
        let mut coordinator =
            Coordinator::start(store.presentation_session(), None).expect("coordinator");
        finish_active(&mut coordinator);
        let complete_paths = coordinator
            .projection()
            .scan
            .snapshot
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();

        coordinator
            .refresh(PresentationTarget::Store)
            .expect("refresh two");
        let obsolete = coordinator.active.as_ref().expect("second").generation;
        coordinator
            .refresh(PresentationTarget::Store)
            .expect("refresh three");
        assert_eq!(
            coordinator.apply_load_event(PresentationEvent::Failure {
                generation: obsolete,
                target: PresentationTarget::Store,
                coverage: pending_coverage(),
                progress: PresentationProgress {
                    completed: 0,
                    total: None,
                },
                message: "obsolete".into(),
            }),
            ProjectionChange::None
        );
        assert!(!coordinator.status().contains("obsolete"));

        let active = coordinator.active.take().expect("current refresh");
        coordinator.finish_failure(active, "current failure".into());
        assert!(coordinator.status().contains("last complete data retained"));
        assert_eq!(
            coordinator
                .projection()
                .scan
                .snapshot
                .entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
            complete_paths
        );
    }

    #[test]
    fn selected_lazy_content_exposes_loaded_and_fresh_failure_states() {
        let root = fixture();
        let store = Store::open(root.path()).expect("store");
        let mut coordinator =
            Coordinator::start(store.presentation_session(), None).expect("coordinator");
        finish_active(&mut coordinator);
        assert!(coordinator.request_content(Some(EVIDENCE)));
        finish_content(&mut coordinator);
        let loaded = coordinator
            .projection()
            .scan
            .snapshot
            .entries
            .into_iter()
            .find(|entry| entry.path == EVIDENCE)
            .expect("evidence");
        assert!(!loaded.original_bytes.is_empty());
        assert!(coordinator.status().contains("Selected content loaded"));

        let root = fixture();
        let store = Store::open(root.path()).expect("store");
        let mut failed =
            Coordinator::start(store.presentation_session(), None).expect("coordinator");
        finish_active(&mut failed);
        fs::write(root.path().join(EVIDENCE), "changed after catalogue").expect("replace");
        assert!(failed.request_content(Some(EVIDENCE)));
        finish_content(&mut failed);
        assert!(failed.status().contains("Content load failed"));
        let evidence = failed
            .projection()
            .scan
            .snapshot
            .entries
            .into_iter()
            .find(|entry| entry.path == EVIDENCE)
            .expect("evidence");
        assert!(evidence.original_bytes.is_empty());
    }

    #[test]
    fn catalogue_resolves_full_nested_investigation_target() {
        let root = fixture();
        let original = root.path().join("projects/demo/investigations/sample");
        let nested = root
            .path()
            .join("projects/demo/investigations/alpha/shared");
        fs::create_dir_all(nested.parent().expect("nested parent")).expect("nested parent");
        fs::rename(original, &nested).expect("nested investigation");
        let activation = fs::read_to_string(root.path().join("casefile.toml"))
            .expect("activation")
            .replace(
                "projects/demo/investigations/sample",
                "projects/demo/investigations/alpha/shared",
            );
        fs::write(root.path().join("casefile.toml"), activation).expect("activation");
        let store = Store::open(root.path()).expect("store");
        let mut coordinator =
            Coordinator::start(store.presentation_session(), None).expect("coordinator");
        let deadline = Instant::now() + Duration::from_secs(5);
        while coordinator
            .investigation_target("demo", "alpha/shared")
            .is_none()
            && Instant::now() < deadline
        {
            coordinator.drain();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            coordinator.investigation_target("demo", "alpha/shared"),
            Some(PresentationTarget::Investigation {
                project: "demo".into(),
                path: "projects/demo/investigations/alpha/shared".into(),
            })
        );
    }

    fn finish_active(coordinator: &mut Coordinator) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while coordinator.active.is_some() && Instant::now() < deadline {
            let update = coordinator.drain();
            if !update.dirty {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(coordinator.active.is_none(), "load did not finish");
    }

    fn finish_content(coordinator: &mut Coordinator) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while coordinator.content.is_some() && Instant::now() < deadline {
            let update = coordinator.drain();
            if !update.dirty {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(coordinator.content.is_none(), "content did not finish");
    }

    fn pending_coverage() -> PresentationCoverage {
        PresentationCoverage {
            catalogue: casefile_store::PresentationCoverageState::Pending,
            payload: casefile_store::PresentationCoverageState::Pending,
            facts: casefile_store::PresentationCoverageState::Pending,
        }
    }

    fn fixture() -> TempDir {
        let temporary = TempDir::new().expect("temporary root");
        copy_tree(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../casefile-store/tests/fixtures/minimum")
                .as_path(),
            temporary.path(),
        );
        temporary
    }

    fn copy_tree(from: &Path, to: &Path) {
        for entry in fs::read_dir(from).expect("fixture entries") {
            let entry = entry.expect("fixture entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("fixture type").is_dir() {
                fs::create_dir_all(&target).expect("fixture directory");
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).expect("fixture file");
            }
        }
    }
}
