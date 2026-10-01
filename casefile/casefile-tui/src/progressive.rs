mod relationships;
use relationships::RelationshipState;
mod content;
mod loading;
mod projection;
use projection::{CatalogueFacts, build_projection};

use casefile_core::{CasefileSnapshot, Classification, EntrySnapshot, Revision};
use casefile_store::{
    ActivationState, DerivedRelationship, DerivedSnapshot, PresentationCatalogue,
    PresentationContentEvent, PresentationContentHandle, PresentationContentRequest,
    PresentationContentSelector, PresentationContentStream, PresentationCoverage,
    PresentationEntry, PresentationEvent, PresentationFact, PresentationLoadRequest,
    PresentationProgress, PresentationSession, PresentationStream, PresentationTarget, ScanResult,
    ScopedIdentity, StoreError,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
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
    entries: BTreeMap<String, Arc<PresentationEntry>>,
    started_observation_generation: u64,
    progress: PresentationProgress,
    coverage: Option<PresentationCoverage>,
}

struct ActiveContent {
    generation: u64,
    target: PresentationTarget,
    path: String,
    stream: PresentationContentStream,
}

pub(crate) struct UiProjection {
    pub(crate) body_owners: BTreeMap<String, Arc<PresentationEntry>>,
    pub(crate) catalogue_changed: bool,
    pub(crate) catalogue_diagnostics: Option<Vec<casefile_core::Diagnostic>>,
    pub(crate) relationship_updates: BTreeMap<ScopedIdentity, Vec<DerivedRelationship>>,
    pub(crate) availability_changed: Vec<String>,
    pub(crate) removed: Vec<String>,
    pub(crate) incremental: bool,
    pub(crate) scan: ScanResult,
    pub(crate) derived: DerivedSnapshot,
    pub(crate) provisional: bool,
    pub(crate) unavailable: BTreeMap<String, String>,
}

pub(crate) struct Coordinator {
    session: PresentationSession,
    relationships: RelationshipState,
    entries: BTreeMap<String, Arc<PresentationEntry>>,
    changed: BTreeSet<String>,
    removed: Vec<String>,
    complete_catalogue: Option<PresentationCatalogue>,
    catalogue_facts: Option<CatalogueFacts>,
    catalogue_pending: bool,
    complete_entry_targets: BTreeMap<String, Arc<PresentationTarget>>,
    catalogue_generation: u64,
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
            entries: BTreeMap::new(),
            relationships: RelationshipState::default(),
            changed: BTreeSet::new(),
            removed: Vec::new(),
            complete_catalogue: None,
            catalogue_facts: None,
            catalogue_pending: false,
            complete_entry_targets: BTreeMap::new(),
            catalogue_generation: 0,
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
            started_observation_generation,
            progress: PresentationProgress {
                completed: 0,
                total: None,
            },
            coverage: None,
        });
        self.status = if initial {
            "Loading…".into()
        } else {
            format!("Refreshing {}…", target_name(&target))
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

    pub(crate) fn projection(&self) -> UiProjection {
        let catalogue = self.catalogue_facts.as_ref();
        let entries = self.visible_entries();
        let mut projection =
            build_projection(catalogue, &entries, !self.has_complete, &self.relationships);
        projection.derived.relationships = self.relationships.edges().cloned().collect();
        projection
    }

    pub(crate) fn take_projection(&mut self) -> UiProjection {
        let entries = self
            .changed
            .iter()
            .filter_map(|path| self.visible_entry(path))
            .collect::<Vec<_>>();
        let mut projection = build_projection(
            self.catalogue_pending
                .then_some(self.catalogue_facts.as_ref())
                .flatten(),
            &entries,
            !self.has_complete,
            &self.relationships,
        );
        projection.relationship_updates = self.relationships.take_updates();
        projection.availability_changed = self.relationships.take_availability_changes();
        for path in &projection.availability_changed {
            projection.unavailable.remove(path);
            if let Some(fields) = self
                .visible_entry(path)
                .and_then(|entry| projection::unavailable_fields(entry, &self.relationships))
            {
                projection.unavailable.insert(path.clone(), fields);
            }
        }
        projection.incremental = true;
        projection.removed = std::mem::take(&mut self.removed);
        self.changed.clear();
        self.catalogue_pending = false;
        projection
    }

    pub(crate) fn loading(&self) -> bool {
        self.active.is_some() || self.content.is_some()
    }

    pub(crate) fn status(&self) -> &str {
        self.content_status.as_deref().unwrap_or(&self.status)
    }

    pub(crate) fn catalogue_generation(&self) -> u64 {
        self.catalogue_generation
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

    fn visible_catalogue(&self) -> Option<&PresentationCatalogue> {
        self.complete_catalogue.as_ref().or_else(|| {
            self.active
                .as_ref()
                .and_then(|active| active.catalogue.as_ref())
        })
    }

    fn visible_entries(&self) -> Vec<&Arc<PresentationEntry>> {
        if self.has_complete {
            return self.entries.values().collect();
        }
        self.active
            .as_ref()
            .map(|active| active.entries.values().collect())
            .unwrap_or_default()
    }

    fn visible_entry(&self, path: &str) -> Option<&Arc<PresentationEntry>> {
        self.entries.get(path).or_else(|| {
            self.active
                .as_ref()
                .and_then(|active| active.entries.get(path))
        })
    }

    fn entry_target(&self, path: &str) -> Option<PresentationTarget> {
        if self.has_complete {
            return self
                .complete_entry_targets
                .get(path)
                .map(|target| target.as_ref().clone());
        }
        self.active
            .as_ref()
            .filter(|active| active.entries.contains_key(path))
            .map(|active| active.target.clone())
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

fn progress_message(target: &PresentationTarget, progress: &PresentationProgress) -> String {
    let total = progress
        .total
        .map(|total| total.to_string())
        .unwrap_or_else(|| "?".into());
    format!(
        "Loading {}: {}/{}",
        target_name(target),
        progress.completed,
        total,
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

#[cfg(test)]
mod tests;
