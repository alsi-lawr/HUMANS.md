//! Bounded, catalogue-first presentation loading kept separate from canonical Store state.
//!
//! Presentation completion covers only the advertised catalogue, entries, and explicit fact
//! availability. It is never a canonical revision and is never consumed by Provider or writer
//! admission. Canonical scans continue to read and preserve every included body.

use crate::{
    activation::{
        Activation, ActivationState, activation, investigation_identity, project_for, scope_for,
    },
    derived::{DerivedBoard, DerivedRelationship, DerivedTicketProgress, derive_snapshot},
    layout::{kind_for_path, normalize_planning_relative},
    scanning::{ScanResult, binding_diagnostics, classify, is_store_path_excluded},
    store::StoreError,
    validation::cross_validate,
};
use casefile_core::{
    CasefileSnapshot, Classification, Diagnostic, EntrySnapshot, Kind, RecordSummary, Revision,
    stable,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, UNIX_EPOCH},
};

pub const PRESENTATION_BATCH_LIMIT: usize = 32;
pub const PRESENTATION_CHANNEL_CAPACITY: usize = 2;
const SEND_RETRY: Duration = Duration::from_millis(5);
static NEXT_PRESENTATION_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum PresentationTarget {
    Store,
    Project { project: String },
    Investigation { project: String, path: String },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PresentationScope {
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investigation: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationCoverageState {
    Pending,
    Partial,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationCoverage {
    pub catalogue: PresentationCoverageState,
    pub payload: PresentationCoverageState,
    pub facts: PresentationCoverageState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationProgress {
    pub completed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactAvailability {
    Unavailable,
    Available,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "availability", content = "value", rename_all = "snake_case")]
pub enum PresentationFact<T> {
    Unavailable,
    Available(T),
}

impl<T> PresentationFact<T> {
    pub fn availability(&self) -> FactAvailability {
        match self {
            Self::Unavailable => FactAvailability::Unavailable,
            Self::Available(_) => FactAvailability::Available,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationFileKind {
    Regular,
    Directory,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationFileMetadata {
    pub kind: PresentationFileKind,
    pub length: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_unix_nanos: Option<u128>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationProject {
    pub slug: String,
    pub prefix: String,
    pub investigations: Vec<PresentationInvestigation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationInvestigation {
    pub identity: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationCatalogue {
    pub activation: ActivationState,
    pub projects: Vec<PresentationProject>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationSummary {
    pub title: String,
    pub record: RecordSummary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationContentHandle {
    session: u64,
    id: u64,
    path: String,
    #[serde(skip)]
    freshness: u128,
}

impl PresentationContentHandle {
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<PresentationScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub metadata: PresentationFileMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_handle: Option<PresentationContentHandle>,
    pub classification: PresentationFact<Classification>,
    pub identity: PresentationFact<Option<String>>,
    pub summary: PresentationFact<Option<PresentationSummary>>,
    pub diagnostics: PresentationFact<Vec<Diagnostic>>,
    pub progress: PresentationFact<Option<DerivedTicketProgress>>,
    pub relationships: PresentationFact<Vec<DerivedRelationship>>,
    pub boards: PresentationFact<Vec<DerivedBoard>>,
    pub body: PresentationFact<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationLoadRequest {
    pub generation: u64,
    pub target: PresentationTarget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PresentationEvent {
    Catalogue {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
        catalogue: PresentationCatalogue,
    },
    Entries {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
        entries: Vec<PresentationEntry>,
    },
    Complete {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
    },
    Failure {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
        message: String,
    },
}

impl PresentationEvent {
    pub fn generation(&self) -> u64 {
        match self {
            Self::Catalogue { generation, .. }
            | Self::Entries { generation, .. }
            | Self::Complete { generation, .. }
            | Self::Failure { generation, .. } => *generation,
        }
    }

    pub fn target(&self) -> &PresentationTarget {
        match self {
            Self::Catalogue { target, .. }
            | Self::Entries { target, .. }
            | Self::Complete { target, .. }
            | Self::Failure { target, .. } => target,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "selector", rename_all = "snake_case")]
pub enum PresentationContentSelector {
    Handle { handle: PresentationContentHandle },
    Path { path: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationContentRequest {
    pub generation: u64,
    pub target: PresentationTarget,
    pub selector: PresentationContentSelector,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PresentationContentEvent {
    Pending {
        generation: u64,
        target: PresentationTarget,
        path: String,
    },
    Loaded {
        generation: u64,
        target: PresentationTarget,
        entry: Box<PresentationEntry>,
    },
    Failure {
        generation: u64,
        target: PresentationTarget,
        path: Option<String>,
        message: String,
    },
}

pub struct PresentationStream {
    receiver: Receiver<PresentationEvent>,
    cancelled: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    inner: Arc<SessionInner>,
}

impl PresentationStream {
    pub fn recv(&self) -> Result<PresentationEvent, mpsc::RecvError> {
        let event = self.receiver.recv()?;
        self.register_received_handles(&event);
        Ok(event)
    }

    pub fn try_recv(&self) -> Result<PresentationEvent, TryRecvError> {
        let event = self.receiver.try_recv()?;
        self.register_received_handles(&event);
        Ok(event)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn register_received_handles(&self, event: &PresentationEvent) {
        if let PresentationEvent::Entries {
            target, entries, ..
        } = event
        {
            register_handles(&self.inner, target, entries);
        }
    }
}

impl Drop for PresentationStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub struct PresentationContentStream {
    receiver: Receiver<PresentationContentEvent>,
    cancelled: Arc<AtomicBool>,
}

impl PresentationContentStream {
    pub fn recv(&self) -> Result<PresentationContentEvent, mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn try_recv(&self) -> Result<PresentationContentEvent, TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Drop for PresentationContentStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Default)]
pub struct PresentationCache {
    catalogue: Option<PresentationCatalogue>,
    entries: BTreeMap<String, PresentationEntry>,
    staged: BTreeMap<(u64, PresentationTarget), BTreeMap<String, PresentationEntry>>,
}

impl PresentationCache {
    pub fn catalogue(&self) -> Option<&PresentationCatalogue> {
        self.catalogue.as_ref()
    }

    pub fn entries(&self) -> impl Iterator<Item = &PresentationEntry> {
        self.entries.values()
    }

    pub fn get(&self, path: &str) -> Option<&PresentationEntry> {
        self.entries.get(path)
    }

    pub fn apply(&mut self, event: &PresentationEvent) {
        let key = (event.generation(), event.target().clone());
        match event {
            PresentationEvent::Catalogue { catalogue, .. } => {
                self.catalogue = Some(catalogue.clone());
                self.staged.insert(key, BTreeMap::new());
            }
            PresentationEvent::Entries { entries, .. } => {
                let staged = self.staged.entry(key).or_default();
                staged.extend(
                    entries
                        .iter()
                        .cloned()
                        .map(|entry| (entry.path.clone(), entry)),
                );
            }
            PresentationEvent::Complete { target, .. } => {
                if let Some(replacement) = self.staged.remove(&key) {
                    self.entries
                        .retain(|path, _| !target_contains(target, path));
                    self.entries.extend(replacement);
                }
            }
            PresentationEvent::Failure { .. } => {
                self.staged.remove(&key);
            }
        }
    }

    pub fn apply_content(&mut self, event: &PresentationContentEvent) {
        match event {
            PresentationContentEvent::Pending { path, .. }
            | PresentationContentEvent::Failure {
                path: Some(path), ..
            } => {
                if let Some(entry) = self.entries.get_mut(path) {
                    entry.body = PresentationFact::Unavailable;
                    if entry.kind == Some(Kind::Evidence) {
                        entry.classification = PresentationFact::Unavailable;
                        entry.summary = PresentationFact::Unavailable;
                        entry.diagnostics = PresentationFact::Unavailable;
                        entry.relationships = PresentationFact::Unavailable;
                        entry.boards = PresentationFact::Unavailable;
                    }
                }
            }
            PresentationContentEvent::Loaded { entry, .. } => {
                self.entries
                    .insert(entry.path.clone(), entry.as_ref().clone());
            }
            PresentationContentEvent::Failure { path: None, .. } => {}
        }
    }
}

#[derive(Clone)]
pub struct PresentationSession {
    inner: Arc<SessionInner>,
}

struct SessionInner {
    session_id: u64,
    reader: Arc<dyn PresentationReader>,
    handles: Mutex<BTreeMap<u64, EmittedContent>>,
    handles_by_path: Mutex<BTreeMap<(PresentationTarget, String), u64>>,
    state: Mutex<BTreeMap<PresentationTarget, LoadedState>>,
    next_handle: AtomicU64,
}

impl PresentationSession {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self::with_reader(Arc::new(FsPresentationReader { root }))
    }

    fn with_reader(reader: Arc<dyn PresentationReader>) -> Self {
        Self {
            inner: Arc::new(SessionInner {
                session_id: NEXT_PRESENTATION_SESSION.fetch_add(1, Ordering::Relaxed),
                reader,
                handles: Mutex::new(BTreeMap::new()),
                handles_by_path: Mutex::new(BTreeMap::new()),
                state: Mutex::new(BTreeMap::new()),
                next_handle: AtomicU64::new(1),
            }),
        }
    }

    pub fn load(&self, request: PresentationLoadRequest) -> Result<PresentationStream, StoreError> {
        let (sender, receiver) = mpsc::sync_channel(PRESENTATION_CHANNEL_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        let inner = self.inner.clone();
        let receiver_inner = inner.clone();
        thread::Builder::new()
            .name("casefile-presentation-loader".into())
            .spawn(move || {
                run_load(inner, request, sender, worker_cancelled);
                worker_finished.store(true, Ordering::Release);
            })?;
        Ok(PresentationStream {
            receiver,
            cancelled,
            finished,
            inner: receiver_inner,
        })
    }

    pub fn fetch_content(
        &self,
        request: PresentationContentRequest,
    ) -> Result<PresentationContentStream, StoreError> {
        let (sender, receiver) = mpsc::sync_channel(PRESENTATION_CHANNEL_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let inner = self.inner.clone();
        thread::Builder::new()
            .name("casefile-presentation-content".into())
            .spawn(move || run_content(inner, request, sender, worker_cancelled))?;
        Ok(PresentationContentStream {
            receiver,
            cancelled,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReaderMetadata {
    public: PresentationFileMetadata,
    freshness: u128,
}

#[derive(Clone)]
struct Descriptor {
    path: String,
    metadata: ReaderMetadata,
    scope: Option<PresentationScope>,
    kind: Option<Kind>,
    lazy: bool,
    handle: Option<PresentationContentHandle>,
}

#[derive(Clone)]
struct EmittedContent {
    descriptor: Descriptor,
    target: PresentationTarget,
}

struct LoadedState {
    target: PresentationTarget,
    activation: Activation,
    scan: ScanResult,
}

trait PresentationReader: Send + Sync {
    fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError>;
    fn read_dir(&self, relative: &str) -> Result<Vec<String>, StoreError>;
    fn metadata(&self, relative: &str) -> Result<ReaderMetadata, StoreError>;
    fn read(&self, relative: &str) -> Result<Vec<u8>, StoreError>;
}

struct FsPresentationReader {
    root: PathBuf,
}

impl FsPresentationReader {
    fn target(&self, relative: &str) -> Result<PathBuf, StoreError> {
        if relative.is_empty() {
            return Ok(self.root.clone());
        }
        let canonical = normalize_planning_relative(relative)
            .map_err(|message| StoreError::Invalid(message.into()))?;
        if canonical != relative || is_store_path_excluded(Path::new(relative)) {
            return Err(StoreError::Invalid(
                "presentation path must be canonical, contained, and included".into(),
            ));
        }
        Ok(self.root.join(relative))
    }

    fn validate_ancestors(&self, relative: &str) -> Result<(), StoreError> {
        let relative = Path::new(relative);
        let mut current = self.root.clone();
        if let Some(parent) = relative.parent() {
            for component in parent.components() {
                current.push(component);
                let metadata = fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(StoreError::Invalid(
                        "presentation path ancestors must remain contained non-symlink directories"
                            .into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl PresentationReader for FsPresentationReader {
    fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
        activation(&self.root)
    }

    fn read_dir(&self, relative: &str) -> Result<Vec<String>, StoreError> {
        let directory = self.target(relative)?;
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        self.validate_ancestors(relative)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(StoreError::Invalid(
                "presentation catalogue directories must remain contained non-symlink directories"
                    .into(),
            ));
        }
        let mut entries = Vec::new();
        match fs::read_dir(directory) {
            Ok(values) => {
                for value in values {
                    let value = value?;
                    let name = value.file_name().to_string_lossy().into_owned();
                    entries.push(if relative.is_empty() {
                        name
                    } else {
                        format!("{relative}/{name}")
                    });
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        }
        entries.sort();
        Ok(entries)
    }

    fn metadata(&self, relative: &str) -> Result<ReaderMetadata, StoreError> {
        self.validate_ancestors(relative)?;
        let metadata = fs::symlink_metadata(self.target(relative)?)?;
        let kind = if metadata.file_type().is_symlink() {
            PresentationFileKind::Symlink
        } else if metadata.is_dir() {
            PresentationFileKind::Directory
        } else if metadata.is_file() {
            PresentationFileKind::Regular
        } else {
            PresentationFileKind::Other
        };
        let modified_unix_nanos = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_nanos());
        let length = metadata.len();
        Ok(ReaderMetadata {
            public: PresentationFileMetadata {
                kind,
                length,
                modified_unix_nanos,
            },
            freshness: metadata_freshness(&metadata, modified_unix_nanos),
        })
    }

    fn read(&self, relative: &str) -> Result<Vec<u8>, StoreError> {
        self.validate_ancestors(relative)?;
        let target = self.target(relative)?;
        let metadata = fs::symlink_metadata(&target)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(StoreError::Invalid(
                "presentation content must remain a regular non-symlink file".into(),
            ));
        }
        let mut file = File::open(target)?;
        if !file.metadata()?.is_file() {
            return Err(StoreError::Invalid(
                "presentation content must remain a regular file".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

fn run_load(
    inner: Arc<SessionInner>,
    request: PresentationLoadRequest,
    sender: SyncSender<PresentationEvent>,
    cancelled: Arc<AtomicBool>,
) {
    let failure_target = request.target.clone();
    let pending = PresentationCoverage {
        catalogue: PresentationCoverageState::Complete,
        payload: PresentationCoverageState::Pending,
        facts: PresentationCoverageState::Pending,
    };
    let result = (|| -> Result<(), StoreError> {
        let (state, activation, diagnostics) = inner.reader.activation()?;
        let catalogue = catalogue(state, &activation, diagnostics);
        if !send_bounded(
            &sender,
            &cancelled,
            PresentationEvent::Catalogue {
                generation: request.generation,
                target: request.target.clone(),
                coverage: pending.clone(),
                progress: PresentationProgress {
                    completed: 0,
                    total: None,
                },
                catalogue,
            },
        ) {
            return Ok(());
        }
        if state != ActivationState::Active {
            send_bounded(
                &sender,
                &cancelled,
                PresentationEvent::Complete {
                    generation: request.generation,
                    target: request.target.clone(),
                    coverage: PresentationCoverage {
                        catalogue: PresentationCoverageState::Complete,
                        payload: PresentationCoverageState::Complete,
                        facts: PresentationCoverageState::Complete,
                    },
                    progress: PresentationProgress {
                        completed: 0,
                        total: Some(0),
                    },
                },
            );
            return Ok(());
        }
        validate_target(&request.target, &activation)?;
        let descriptors = collect_descriptors(&inner, &request.target, &activation, &cancelled)?;
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        let catalogue_entries = descriptors
            .iter()
            .filter(|descriptor| {
                descriptor.lazy || descriptor.metadata.public.kind != PresentationFileKind::Regular
            })
            .map(|descriptor| catalogue_entry(descriptor, &activation))
            .collect::<Vec<_>>();
        let total = descriptors.len();
        let mut completed = 0;
        if !send_entry_batches(
            &sender,
            &cancelled,
            request.generation,
            &request.target,
            &catalogue_entries,
            &mut completed,
            total,
        ) {
            return Ok(());
        }
        let (entries, scan) = load_entries(
            &inner,
            &request.target,
            &activation,
            &descriptors,
            &cancelled,
        )?;
        inner.state.lock().expect("presentation state").insert(
            request.target.clone(),
            LoadedState {
                target: request.target.clone(),
                activation,
                scan,
            },
        );
        let eager_paths = descriptors
            .iter()
            .filter(|descriptor| {
                !descriptor.lazy && descriptor.metadata.public.kind == PresentationFileKind::Regular
            })
            .map(|descriptor| descriptor.path.as_str())
            .collect::<BTreeSet<_>>();
        let eager_entries = entries
            .into_iter()
            .filter(|entry| eager_paths.contains(entry.path.as_str()))
            .collect::<Vec<_>>();
        if !send_entry_batches(
            &sender,
            &cancelled,
            request.generation,
            &request.target,
            &eager_entries,
            &mut completed,
            total,
        ) {
            return Ok(());
        }
        send_bounded(
            &sender,
            &cancelled,
            PresentationEvent::Complete {
                generation: request.generation,
                target: request.target.clone(),
                coverage: PresentationCoverage {
                    catalogue: PresentationCoverageState::Complete,
                    payload: PresentationCoverageState::Complete,
                    facts: PresentationCoverageState::Complete,
                },
                progress: PresentationProgress {
                    completed: total,
                    total: Some(total),
                },
            },
        );
        Ok(())
    })();
    if let Err(error) = result {
        send_bounded(
            &sender,
            &cancelled,
            PresentationEvent::Failure {
                generation: request.generation,
                target: failure_target,
                coverage: pending,
                progress: PresentationProgress {
                    completed: 0,
                    total: None,
                },
                message: error.to_string(),
            },
        );
    }
}

fn run_content(
    inner: Arc<SessionInner>,
    request: PresentationContentRequest,
    sender: SyncSender<PresentationContentEvent>,
    cancelled: Arc<AtomicBool>,
) {
    let selected = select_emitted(&inner, &request.target, &request.selector);
    let selected = match selected {
        Ok(value) if value.target == request.target => value,
        Ok(_) => {
            send_content_failure(
                &sender,
                &cancelled,
                &request,
                None,
                "content target does not match the emitted entry",
            );
            return;
        }
        Err((path, message)) => {
            send_content_failure(&sender, &cancelled, &request, path, &message);
            return;
        }
    };
    if !send_content_bounded(
        &sender,
        &cancelled,
        PresentationContentEvent::Pending {
            generation: request.generation,
            target: request.target.clone(),
            path: selected.descriptor.path.clone(),
        },
    ) {
        return;
    }
    let result = fetch_entry(&inner, &selected);
    let event = match result {
        Ok(entry) => PresentationContentEvent::Loaded {
            generation: request.generation,
            target: request.target,
            entry: Box::new(entry),
        },
        Err(error) => PresentationContentEvent::Failure {
            generation: request.generation,
            target: request.target,
            path: Some(selected.descriptor.path),
            message: error.to_string(),
        },
    };
    send_content_bounded(&sender, &cancelled, event);
}

fn catalogue(
    state: ActivationState,
    activation: &Activation,
    diagnostics: Vec<Diagnostic>,
) -> PresentationCatalogue {
    let projects = if state == ActivationState::Active {
        activation
            .projects
            .iter()
            .map(|(slug, project)| PresentationProject {
                slug: slug.clone(),
                prefix: project.prefix.clone(),
                investigations: project
                    .investigations
                    .iter()
                    .filter_map(|path| {
                        investigation_identity(slug, path).map(|identity| {
                            PresentationInvestigation {
                                identity: identity.into(),
                                path: path.clone(),
                            }
                        })
                    })
                    .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };
    PresentationCatalogue {
        activation: state,
        projects,
        diagnostics,
    }
}

fn validate_target(target: &PresentationTarget, active: &Activation) -> Result<(), StoreError> {
    let valid = match target {
        PresentationTarget::Store => true,
        PresentationTarget::Project { project } => active.projects.contains_key(project),
        PresentationTarget::Investigation { project, path } => active
            .projects
            .get(project)
            .is_some_and(|value| value.investigations.contains(path)),
    };
    valid.then_some(()).ok_or_else(|| {
        StoreError::Invalid("presentation target is not present in the activation catalogue".into())
    })
}

fn collect_descriptors(
    inner: &SessionInner,
    target: &PresentationTarget,
    active: &Activation,
    cancelled: &AtomicBool,
) -> Result<Vec<Descriptor>, StoreError> {
    let start = target_root(target);
    let mut pending = vec![start];
    let mut descriptors = Vec::new();
    while let Some(directory) = pending.pop() {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        let mut children = inner.reader.read_dir(&directory)?;
        children.sort();
        for path in children.into_iter().rev() {
            if is_store_path_excluded(Path::new(&path)) {
                continue;
            }
            let metadata = inner.reader.metadata(&path)?;
            if metadata.public.kind == PresentationFileKind::Directory {
                pending.push(path);
                continue;
            }
            if metadata.public.kind == PresentationFileKind::Other {
                continue;
            }
            let kind = presentation_kind(&path, active);
            let lazy = metadata.public.kind == PresentationFileKind::Regular
                && (kind == Some(Kind::Evidence) || kind.is_none());
            let safe = normalize_planning_relative(&path).is_ok_and(|canonical| canonical == path);
            let handle = (lazy && safe).then(|| PresentationContentHandle {
                session: inner.session_id,
                id: inner.next_handle.fetch_add(1, Ordering::Relaxed),
                path: path.clone(),
                freshness: metadata.freshness,
            });
            descriptors.push(Descriptor {
                scope: presentation_scope(&path, active),
                path,
                metadata,
                kind,
                lazy,
                handle,
            });
        }
    }
    descriptors.sort_by(|left, right| (&left.scope, &left.path).cmp(&(&right.scope, &right.path)));
    Ok(descriptors)
}

fn load_entries(
    inner: &SessionInner,
    target: &PresentationTarget,
    active: &Activation,
    descriptors: &[Descriptor],
    cancelled: &AtomicBool,
) -> Result<(Vec<PresentationEntry>, ScanResult), StoreError> {
    let mut snapshots = Vec::new();
    let mut local_diagnostics = Vec::new();
    for descriptor in descriptors {
        if cancelled.load(Ordering::Acquire) {
            return Err(StoreError::Invalid("presentation load cancelled".into()));
        }
        if descriptor.metadata.public.kind == PresentationFileKind::Symlink {
            local_diagnostics.push(Diagnostic::new(
                &descriptor.path,
                "unsafe_path",
                "governed paths cannot be symlinks",
            ));
            snapshots.push(EntrySnapshot {
                path: descriptor.path.clone(),
                classification: Classification::Invalid,
                kind: descriptor.kind,
                identity: None,
                content_revision: digest(&[]),
                summary: None,
                original_bytes: Vec::new(),
            });
            continue;
        }
        if descriptor.lazy {
            snapshots.push(EntrySnapshot {
                path: descriptor.path.clone(),
                classification: if scope_for(&descriptor.path, active).is_some() {
                    Classification::Raw
                } else {
                    Classification::Ungoverned
                },
                kind: None,
                identity: None,
                content_revision: digest(&[]),
                summary: None,
                original_bytes: Vec::new(),
            });
            continue;
        }
        let bytes = read_fresh(inner.reader.as_ref(), descriptor)?;
        let (classification, kind, identity, summary, mut diagnostics) =
            classify(&descriptor.path, &bytes, active);
        local_diagnostics.append(&mut diagnostics);
        snapshots.push(EntrySnapshot {
            path: descriptor.path.clone(),
            classification,
            kind,
            identity,
            content_revision: digest(&bytes),
            summary,
            original_bytes: bytes,
        });
    }
    local_diagnostics.extend(cross_validate(&snapshots, active));
    local_diagnostics.extend(binding_diagnostics(&snapshots));
    let scan = ScanResult {
        activation: ActivationState::Active,
        investigation_roots: active
            .projects
            .iter()
            .map(|(project, value)| {
                (
                    project.clone(),
                    value
                        .investigations
                        .iter()
                        .filter_map(|path| investigation_identity(project, path).map(Into::into))
                        .collect(),
                )
            })
            .collect(),
        snapshot: CasefileSnapshot {
            revision: digest(b"presentation projection"),
            entries: snapshots,
        },
        diagnostics: stable(local_diagnostics),
    };
    let derived = derive_snapshot(&scan);
    let coherent = matches!(target, PresentationTarget::Store);
    let entries = descriptors
        .iter()
        .map(|descriptor| presentation_entry(descriptor, &scan, &derived, coherent))
        .collect();
    Ok((entries, scan))
}

fn presentation_entry(
    descriptor: &Descriptor,
    scan: &ScanResult,
    derived: &crate::derived::DerivedSnapshot,
    coherent: bool,
) -> PresentationEntry {
    let snapshot = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == descriptor.path)
        .expect("descriptor has a presentation snapshot");
    if descriptor.kind == Some(Kind::Evidence) && descriptor.lazy {
        return PresentationEntry {
            path: descriptor.path.clone(),
            scope: descriptor.scope.clone(),
            kind: descriptor.kind,
            metadata: descriptor.metadata.public.clone(),
            content_handle: descriptor.handle.clone(),
            classification: PresentationFact::Unavailable,
            identity: PresentationFact::Available(None),
            summary: PresentationFact::Unavailable,
            diagnostics: PresentationFact::Unavailable,
            progress: PresentationFact::Available(None),
            relationships: PresentationFact::Unavailable,
            boards: PresentationFact::Unavailable,
            body: PresentationFact::Unavailable,
        };
    }
    let record = derived
        .records
        .iter()
        .find(|record| record.path == descriptor.path);
    let title = record.map(|record| record.title.clone());
    let identity = snapshot.identity.clone();
    let summary = snapshot.summary.clone().map(|record| PresentationSummary {
        title: title.unwrap_or_else(|| descriptor.path.clone()),
        record,
    });
    let diagnostics = scan
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path == descriptor.path)
        .cloned()
        .collect();
    let progress = record.and_then(|record| record.progress.clone());
    let relationships = record
        .and_then(|record| record.identity.as_ref())
        .map(|identity| {
            derived
                .relationships
                .iter()
                .filter(|relationship| &relationship.source == identity)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let boards = record
        .and_then(|record| record.identity.as_ref())
        .map(|identity| {
            derived
                .boards
                .iter()
                .filter(|board| &board.identity == identity)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let body =
        if descriptor.lazy || descriptor.metadata.public.kind != PresentationFileKind::Regular {
            PresentationFact::Unavailable
        } else {
            PresentationFact::Available(snapshot.original_bytes.clone())
        };
    PresentationEntry {
        path: descriptor.path.clone(),
        scope: descriptor.scope.clone(),
        kind: if descriptor.lazy {
            descriptor.kind
        } else {
            snapshot.kind
        },
        metadata: descriptor.metadata.public.clone(),
        content_handle: descriptor.handle.clone(),
        classification: PresentationFact::Available(snapshot.classification),
        identity: PresentationFact::Available(identity),
        summary: PresentationFact::Available(summary),
        diagnostics: available_if(coherent, diagnostics),
        progress: available_if(coherent, progress),
        relationships: available_if(coherent, relationships),
        boards: available_if(coherent, boards),
        body,
    }
}

fn catalogue_entry(descriptor: &Descriptor, active: &Activation) -> PresentationEntry {
    if descriptor.kind == Some(Kind::Evidence) {
        return PresentationEntry {
            path: descriptor.path.clone(),
            scope: descriptor.scope.clone(),
            kind: descriptor.kind,
            metadata: descriptor.metadata.public.clone(),
            content_handle: descriptor.handle.clone(),
            classification: PresentationFact::Unavailable,
            identity: PresentationFact::Available(None),
            summary: PresentationFact::Unavailable,
            diagnostics: PresentationFact::Unavailable,
            progress: PresentationFact::Available(None),
            relationships: PresentationFact::Unavailable,
            boards: PresentationFact::Unavailable,
            body: PresentationFact::Unavailable,
        };
    }
    let symlink = descriptor.metadata.public.kind == PresentationFileKind::Symlink;
    PresentationEntry {
        path: descriptor.path.clone(),
        scope: descriptor.scope.clone(),
        kind: descriptor.kind,
        metadata: descriptor.metadata.public.clone(),
        content_handle: descriptor.handle.clone(),
        classification: PresentationFact::Available(if symlink {
            Classification::Invalid
        } else if scope_for(&descriptor.path, active).is_some() {
            Classification::Raw
        } else {
            Classification::Ungoverned
        }),
        identity: PresentationFact::Available(None),
        summary: PresentationFact::Available(None),
        diagnostics: PresentationFact::Available(if symlink {
            vec![Diagnostic::new(
                &descriptor.path,
                "unsafe_path",
                "governed paths cannot be symlinks",
            )]
        } else {
            Vec::new()
        }),
        progress: PresentationFact::Available(None),
        relationships: PresentationFact::Available(Vec::new()),
        boards: PresentationFact::Available(Vec::new()),
        body: PresentationFact::Unavailable,
    }
}

fn fetch_entry(
    inner: &SessionInner,
    emitted: &EmittedContent,
) -> Result<PresentationEntry, StoreError> {
    let descriptor = &emitted.descriptor;
    let bytes = read_fresh(inner.reader.as_ref(), descriptor)?;
    if bytes.is_empty() {
        return Err(StoreError::Invalid(
            "presentation content is empty and was not promoted".into(),
        ));
    }
    let mut state = inner.state.lock().expect("presentation state");
    let loaded = state
        .get_mut(&emitted.target)
        .ok_or_else(|| StoreError::Invalid("presentation payload has not completed".into()))?;
    if loaded.target != emitted.target {
        return Err(StoreError::Invalid(
            "presentation content target is no longer loaded".into(),
        ));
    }
    if descriptor.kind == Some(Kind::Evidence) {
        let (classification, kind, identity, summary, mut diagnostics) =
            classify(&descriptor.path, &bytes, &loaded.activation);
        let replacement = EntrySnapshot {
            path: descriptor.path.clone(),
            classification,
            kind,
            identity,
            content_revision: digest(&bytes),
            summary,
            original_bytes: bytes,
        };
        if let Some(entry) = loaded
            .scan
            .snapshot
            .entries
            .iter_mut()
            .find(|entry| entry.path == descriptor.path)
        {
            *entry = replacement;
        }
        diagnostics.extend(cross_validate(
            &loaded.scan.snapshot.entries,
            &loaded.activation,
        ));
        diagnostics.extend(binding_diagnostics(&loaded.scan.snapshot.entries));
        loaded.scan.diagnostics = stable(diagnostics);
    } else if let Some(entry) = loaded
        .scan
        .snapshot
        .entries
        .iter_mut()
        .find(|entry| entry.path == descriptor.path)
    {
        entry.original_bytes = bytes;
        entry.content_revision = digest(&entry.original_bytes);
    }
    let derived = derive_snapshot(&loaded.scan);
    let mut loaded_descriptor = descriptor.clone();
    loaded_descriptor.lazy = false;
    Ok(presentation_entry(
        &loaded_descriptor,
        &loaded.scan,
        &derived,
        matches!(loaded.target, PresentationTarget::Store),
    ))
}

fn read_fresh(
    reader: &dyn PresentationReader,
    descriptor: &Descriptor,
) -> Result<Vec<u8>, StoreError> {
    let before = reader.metadata(&descriptor.path)?;
    if before.public.kind != PresentationFileKind::Regular || before != descriptor.metadata {
        return Err(StoreError::Invalid(
            "presentation content changed after catalogue emission".into(),
        ));
    }
    let bytes = reader.read(&descriptor.path)?;
    let after = reader.metadata(&descriptor.path)?;
    if after.public.kind != PresentationFileKind::Regular || after != before {
        return Err(StoreError::Invalid(
            "presentation content changed while it was read".into(),
        ));
    }
    Ok(bytes)
}

fn register_handles(
    inner: &SessionInner,
    target: &PresentationTarget,
    entries: &[PresentationEntry],
) {
    for entry in entries {
        let Some(handle) = &entry.content_handle else {
            continue;
        };
        let descriptor = Descriptor {
            path: entry.path.clone(),
            metadata: ReaderMetadata {
                public: entry.metadata.clone(),
                freshness: handle.freshness,
            },
            scope: entry.scope.clone(),
            kind: entry.kind,
            lazy: true,
            handle: Some(handle.clone()),
        };
        inner.handles.lock().expect("presentation handles").insert(
            handle.id,
            EmittedContent {
                descriptor,
                target: target.clone(),
            },
        );
        inner
            .handles_by_path
            .lock()
            .expect("presentation handle paths")
            .insert((target.clone(), entry.path.clone()), handle.id);
    }
}

fn select_emitted(
    inner: &SessionInner,
    target: &PresentationTarget,
    selector: &PresentationContentSelector,
) -> Result<EmittedContent, (Option<String>, String)> {
    let path = match selector {
        PresentationContentSelector::Handle { handle } => {
            if handle.session != inner.session_id {
                return Err((
                    Some(handle.path.clone()),
                    "content handle was not emitted by this session".into(),
                ));
            }
            let emitted = inner
                .handles
                .lock()
                .expect("presentation handles")
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| {
                    (
                        Some(handle.path.clone()),
                        "content handle was not emitted by this session".into(),
                    )
                })?;
            if emitted.descriptor.path != handle.path {
                return Err((
                    Some(handle.path.clone()),
                    "content handle does not match its emitted path".into(),
                ));
            }
            return Ok(emitted);
        }
        PresentationContentSelector::Path { path } => {
            let canonical = normalize_planning_relative(path)
                .map_err(|message| (Some(path.clone()), message.into()))?;
            if canonical != *path || is_store_path_excluded(Path::new(path)) {
                return Err((
                    Some(path.clone()),
                    "content path must be canonical, contained, and included".into(),
                ));
            }
            canonical
        }
    };
    let id = inner
        .handles_by_path
        .lock()
        .expect("presentation handle paths")
        .get(&(target.clone(), path.clone()))
        .copied()
        .ok_or_else(|| {
            (
                Some(path.clone()),
                "content path was not emitted by this session".into(),
            )
        })?;
    inner
        .handles
        .lock()
        .expect("presentation handles")
        .get(&id)
        .cloned()
        .ok_or_else(|| {
            (
                Some(path),
                "content path was not emitted by this session".into(),
            )
        })
}

fn presentation_scope(path: &str, active: &Activation) -> Option<PresentationScope> {
    let project = project_for(path, active)?;
    let investigation = scope_for(path, active)
        .and_then(|base| investigation_identity(project, base))
        .map(Into::into);
    Some(PresentationScope {
        project: project.into(),
        investigation,
    })
}

fn presentation_kind(path: &str, active: &Activation) -> Option<Kind> {
    match path {
        "casefile.toml" => Some(Kind::Activation),
        "projects.toml" => Some(Kind::ProjectMap),
        _ => kind_for_path(path, active),
    }
}

fn target_root(target: &PresentationTarget) -> String {
    match target {
        PresentationTarget::Store => String::new(),
        PresentationTarget::Project { project } => format!("projects/{project}"),
        PresentationTarget::Investigation { path, .. } => path.clone(),
    }
}

fn target_contains(target: &PresentationTarget, path: &str) -> bool {
    let root = target_root(target);
    root.is_empty() || path == root || path.starts_with(&(root + "/"))
}

fn available_if<T>(available: bool, value: T) -> PresentationFact<T> {
    if available {
        PresentationFact::Available(value)
    } else {
        PresentationFact::Unavailable
    }
}

fn digest(bytes: &[u8]) -> Revision {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Revision(format!("sha256:{}", hex(&hasher.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn metadata_freshness(metadata: &fs::Metadata, _modified: Option<u128>) -> u128 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mut hasher = Sha256::new();
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(metadata.mtime().to_le_bytes());
        hasher.update(metadata.mtime_nsec().to_le_bytes());
        hasher.update(metadata.ctime().to_le_bytes());
        hasher.update(metadata.ctime_nsec().to_le_bytes());
        u128::from_le_bytes(hasher.finalize()[..16].try_into().expect("digest width"))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let mut hasher = Sha256::new();
        hasher.update(
            metadata
                .volume_serial_number()
                .unwrap_or_default()
                .to_le_bytes(),
        );
        hasher.update(metadata.file_index().unwrap_or_default().to_le_bytes());
        hasher.update(metadata.file_size().to_le_bytes());
        hasher.update(metadata.creation_time().to_le_bytes());
        hasher.update(metadata.last_write_time().to_le_bytes());
        u128::from_le_bytes(hasher.finalize()[..16].try_into().expect("digest width"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        _modified.unwrap_or_default() ^ u128::from(metadata.len())
    }
}

fn send_entry_batches(
    sender: &SyncSender<PresentationEvent>,
    cancelled: &AtomicBool,
    generation: u64,
    target: &PresentationTarget,
    entries: &[PresentationEntry],
    completed: &mut usize,
    total: usize,
) -> bool {
    for scope_entries in entries.chunk_by(|left, right| left.scope == right.scope) {
        for chunk in scope_entries.chunks(PRESENTATION_BATCH_LIMIT) {
            *completed += chunk.len();
            if !send_bounded(
                sender,
                cancelled,
                PresentationEvent::Entries {
                    generation,
                    target: target.clone(),
                    coverage: PresentationCoverage {
                        catalogue: PresentationCoverageState::Complete,
                        payload: if *completed == total {
                            PresentationCoverageState::Complete
                        } else {
                            PresentationCoverageState::Partial
                        },
                        facts: if *completed == total {
                            PresentationCoverageState::Complete
                        } else {
                            PresentationCoverageState::Partial
                        },
                    },
                    progress: PresentationProgress {
                        completed: *completed,
                        total: Some(total),
                    },
                    entries: chunk.to_vec(),
                },
            ) {
                return false;
            }
        }
    }
    true
}

fn send_bounded(
    sender: &SyncSender<PresentationEvent>,
    cancelled: &AtomicBool,
    mut event: PresentationEvent,
) -> bool {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(value)) => {
                event = value;
                thread::sleep(SEND_RETRY);
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn send_content_bounded(
    sender: &SyncSender<PresentationContentEvent>,
    cancelled: &AtomicBool,
    mut event: PresentationContentEvent,
) -> bool {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(value)) => {
                event = value;
                thread::sleep(SEND_RETRY);
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn send_content_failure(
    sender: &SyncSender<PresentationContentEvent>,
    cancelled: &AtomicBool,
    request: &PresentationContentRequest,
    path: Option<String>,
    message: &str,
) {
    send_content_bounded(
        sender,
        cancelled,
        PresentationContentEvent::Failure {
            generation: request.generation,
            target: request.target.clone(),
            path,
            message: message.into(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::Project;
    use std::{collections::BTreeSet, sync::Condvar};
    use tempfile::TempDir;

    const INVESTIGATION: &str = "projects/demo/investigations/sample";
    const TICKET: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    const EVIDENCE: &str = "projects/demo/investigations/sample/evidence/observation.md";
    const RAW: &str = "projects/demo/investigations/sample/large.raw";

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Operation {
        Activation,
        ReadDir(String),
        Metadata(String),
        Body(String),
    }

    #[derive(Clone)]
    enum FakeNode {
        Directory,
        File { bytes: Vec<u8>, version: u128 },
        Symlink,
    }

    struct FakeState {
        activation: (ActivationState, Activation, Vec<Diagnostic>),
        nodes: BTreeMap<String, FakeNode>,
        operations: Vec<Operation>,
        fail_reads: BTreeSet<String>,
        blocked_path: Option<String>,
        block_entered: bool,
        block_released: bool,
        next_version: u128,
    }

    struct FakeReader {
        state: Mutex<FakeState>,
        changed: Condvar,
    }

    impl FakeReader {
        fn active() -> Arc<Self> {
            let activation = active_activation();
            let reader = Arc::new(Self {
                state: Mutex::new(FakeState {
                    activation: (ActivationState::Active, activation, Vec::new()),
                    nodes: BTreeMap::new(),
                    operations: Vec::new(),
                    fail_reads: BTreeSet::new(),
                    blocked_path: None,
                    block_entered: false,
                    block_released: false,
                    next_version: 1,
                }),
                changed: Condvar::new(),
            });
            reader.insert_file(
                "casefile.toml",
                b"schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/sample']\n".to_vec(),
            );
            reader.insert_file(
                "projects.toml",
                b"schema_version = 1\n[projects]\ndemo = 'projects/demo'\n".to_vec(),
            );
            reader.insert_file(
                TICKET,
                include_bytes!("../tests/fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md").to_vec(),
            );
            reader.insert_file(
                EVIDENCE,
                b"---\nattachments: []\n---\n\n# Observation\n\nEvidence body.\n".to_vec(),
            );
            reader.insert_file(RAW, vec![b'x'; 1024 * 1024]);
            reader.insert_file(".git/ignored.raw", b"implementation metadata".to_vec());
            reader.insert_file(
                "projects/demo/investigations/sample/.git/visible.raw",
                b"nested same-name content".to_vec(),
            );
            reader.insert_file(
                "projects/demo/investigations/sample/NUL",
                b"unsafe portable handle".to_vec(),
            );
            reader.insert_file("projects/demo/investigations/sample/empty.raw", Vec::new());
            reader
        }

        fn early(state: ActivationState) -> Arc<Self> {
            Arc::new(Self {
                state: Mutex::new(FakeState {
                    activation: (
                        state,
                        Activation::default(),
                        (state == ActivationState::Invalid)
                            .then(|| {
                                Diagnostic::new(
                                    "casefile.toml",
                                    "invalid_activation",
                                    "invalid fixture",
                                )
                            })
                            .into_iter()
                            .collect(),
                    ),
                    nodes: BTreeMap::from([(
                        "ignored.raw".into(),
                        FakeNode::File {
                            bytes: b"must not read".to_vec(),
                            version: 1,
                        },
                    )]),
                    operations: Vec::new(),
                    fail_reads: BTreeSet::new(),
                    blocked_path: None,
                    block_entered: false,
                    block_released: false,
                    next_version: 2,
                }),
                changed: Condvar::new(),
            })
        }

        fn many_raw(count: usize) -> Arc<Self> {
            let reader = Self::active();
            for index in 0..count {
                reader.insert_file(
                    &format!("{INVESTIGATION}/raw/{index:04}.txt"),
                    format!("raw {index}").into_bytes(),
                );
            }
            reader
        }

        fn insert_file(&self, path: &str, bytes: Vec<u8>) {
            let mut state = self.state.lock().expect("fake state");
            insert_parent_directories(&mut state.nodes, path);
            let version = state.next_version;
            state.next_version += 1;
            state
                .nodes
                .insert(path.into(), FakeNode::File { bytes, version });
        }

        fn remove(&self, path: &str) {
            self.state.lock().expect("fake state").nodes.remove(path);
        }

        fn replace(&self, path: &str, bytes: Vec<u8>) {
            self.insert_file(path, bytes);
        }

        fn symlink(&self, path: &str) {
            self.state
                .lock()
                .expect("fake state")
                .nodes
                .insert(path.into(), FakeNode::Symlink);
        }

        fn fail_read(&self, path: &str) {
            self.state
                .lock()
                .expect("fake state")
                .fail_reads
                .insert(path.into());
        }

        fn block(&self, path: &str) {
            let mut state = self.state.lock().expect("fake state");
            state.blocked_path = Some(path.into());
            state.block_entered = false;
            state.block_released = false;
        }

        fn wait_until_blocked(&self) {
            let mut state = self.state.lock().expect("fake state");
            while !state.block_entered {
                state = self.changed.wait(state).expect("fake block wait");
            }
        }

        fn release(&self) {
            let mut state = self.state.lock().expect("fake state");
            state.block_released = true;
            self.changed.notify_all();
        }

        fn operations(&self) -> Vec<Operation> {
            self.state.lock().expect("fake state").operations.clone()
        }
    }

    impl PresentationReader for FakeReader {
        fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
            let mut state = self.state.lock().expect("fake state");
            state.operations.push(Operation::Activation);
            Ok(state.activation.clone())
        }

        fn read_dir(&self, relative: &str) -> Result<Vec<String>, StoreError> {
            let mut state = self.state.lock().expect("fake state");
            state.operations.push(Operation::ReadDir(relative.into()));
            let prefix = if relative.is_empty() {
                String::new()
            } else {
                format!("{relative}/")
            };
            let mut values = state
                .nodes
                .keys()
                .filter_map(|path| {
                    let rest = path.strip_prefix(&prefix)?;
                    (!rest.is_empty() && !rest.contains('/')).then(|| path.clone())
                })
                .collect::<Vec<_>>();
            values.sort();
            Ok(values)
        }

        fn metadata(&self, relative: &str) -> Result<ReaderMetadata, StoreError> {
            let mut state = self.state.lock().expect("fake state");
            state.operations.push(Operation::Metadata(relative.into()));
            let node = state.nodes.get(relative).cloned().ok_or_else(not_found)?;
            let (kind, length, version) = match node {
                FakeNode::Directory => (PresentationFileKind::Directory, 0, 0),
                FakeNode::File { bytes, version } => {
                    (PresentationFileKind::Regular, bytes.len() as u64, version)
                }
                FakeNode::Symlink => (PresentationFileKind::Symlink, 0, 0),
            };
            Ok(ReaderMetadata {
                public: PresentationFileMetadata {
                    kind,
                    length,
                    modified_unix_nanos: Some(version),
                },
                freshness: version,
            })
        }

        fn read(&self, relative: &str) -> Result<Vec<u8>, StoreError> {
            let mut state = self.state.lock().expect("fake state");
            state.operations.push(Operation::Body(relative.into()));
            if state.fail_reads.contains(relative) {
                return Err(io::Error::other("injected read failure").into());
            }
            if state.blocked_path.as_deref() == Some(relative) {
                state.block_entered = true;
                self.changed.notify_all();
                while !state.block_released {
                    state = self.changed.wait(state).expect("fake release wait");
                }
            }
            match state.nodes.get(relative) {
                Some(FakeNode::File { bytes, .. }) => Ok(bytes.clone()),
                Some(_) => Err(StoreError::Invalid(
                    "fake path is not a regular file".into(),
                )),
                None => Err(not_found()),
            }
        }
    }

    #[test]
    fn catalogue_precedes_blocked_payload_and_lazy_body_reads() {
        let reader = FakeReader::active();
        reader.block(TICKET);
        let session = PresentationSession::with_reader(reader.clone());
        let request = load_request(41, PresentationTarget::Store);
        let stream = session.load(request.clone()).expect("load");

        let first = stream.recv().expect("catalogue");
        match first {
            PresentationEvent::Catalogue {
                generation,
                target,
                coverage,
                progress,
                catalogue,
            } => {
                assert_eq!(generation, 41);
                assert_eq!(target, request.target);
                assert_eq!(coverage.catalogue, PresentationCoverageState::Complete);
                assert_eq!(coverage.payload, PresentationCoverageState::Pending);
                assert_eq!(progress.total, None);
                assert_eq!(catalogue.activation, ActivationState::Active);
                assert_eq!(catalogue.projects[0].slug, "demo");
                assert_eq!(catalogue.projects[0].investigations[0].path, INVESTIGATION);
            }
            other => panic!("unexpected first event: {other:?}"),
        }

        let early = stream.recv().expect("lazy catalogue entries");
        assert!(matches!(&early, PresentationEvent::Entries { entries, .. }
            if entries.iter().any(|entry| entry.path == RAW)
                && entries.iter().any(|entry| entry.path == EVIDENCE)));
        reader.wait_until_blocked();
        let operations = reader.operations();
        assert!(!operations.contains(&Operation::Body(RAW.into())));
        assert!(!operations.contains(&Operation::Body(EVIDENCE.into())));
        reader.release();

        let mut events = vec![early];
        events.extend(drain(stream));
        let entries = event_entries(&events);
        assert!(events.iter().all(|event| event.generation() == 41));
        assert!(events.iter().all(|event| event.target() == &request.target));
        assert!(events.iter().all(|event| {
            !matches!(event, PresentationEvent::Entries { entries, .. } if entries.len() > PRESENTATION_BATCH_LIMIT)
        }));
        for event in &events {
            if let PresentationEvent::Entries { entries, .. } = event {
                assert!(entries.windows(2).all(|pair| pair[0].path <= pair[1].path));
                assert!(entries.iter().all(|entry| entry.scope == entries[0].scope));
            }
        }
        for path in [RAW, EVIDENCE] {
            let entry = entries
                .iter()
                .find(|entry| entry.path == path)
                .expect("lazy entry");
            assert!(entry.content_handle.is_some());
            assert_eq!(entry.body, PresentationFact::Unavailable);
        }
        assert!(entries.iter().all(|entry| entry.path != ".git/ignored.raw"));
        assert!(
            entries.iter().any(|entry| {
                entry.path == "projects/demo/investigations/sample/.git/visible.raw"
            })
        );
        assert!(
            entries
                .iter()
                .find(|entry| entry.path == "projects/demo/investigations/sample/NUL")
                .is_some_and(|entry| entry.content_handle.is_none())
        );
        let evidence = entries
            .iter()
            .find(|entry| entry.path == EVIDENCE)
            .expect("evidence");
        assert_eq!(evidence.classification, PresentationFact::Unavailable);
        assert_eq!(evidence.summary, PresentationFact::Unavailable);
        assert_eq!(evidence.diagnostics, PresentationFact::Unavailable);
        let operations = reader.operations();
        assert!(!operations.contains(&Operation::Body(RAW.into())));
        assert!(!operations.contains(&Operation::Body(EVIDENCE.into())));
    }

    #[test]
    fn content_replacement_or_removal_during_a_blocked_read_is_not_promoted() {
        for mutation in ["replace", "remove"] {
            let (reader, session, entries) = loaded_fake();
            let handle = entries
                .iter()
                .find(|entry| entry.path == RAW)
                .expect("raw")
                .content_handle
                .clone()
                .expect("handle");
            reader.block(RAW);
            let stream = session
                .fetch_content(content_request(10, handle))
                .expect("content");
            assert!(matches!(
                stream.recv().expect("pending"),
                PresentationContentEvent::Pending { .. }
            ));
            reader.wait_until_blocked();
            if mutation == "replace" {
                reader.replace(RAW, vec![b'z'; 1024 * 1024]);
            } else {
                reader.remove(RAW);
            }
            reader.release();
            assert!(matches!(
                stream.recv().expect("freshness failure"),
                PresentationContentEvent::Failure { .. }
            ));
        }
    }

    #[test]
    fn lazy_content_reports_pending_loaded_or_fresh_failure_without_old_bytes() {
        let (reader, session, entries) = loaded_fake();
        let evidence = entries
            .iter()
            .find(|entry| entry.path == EVIDENCE)
            .expect("evidence")
            .content_handle
            .clone()
            .expect("handle");
        let events = drain_content(
            session
                .fetch_content(content_request(7, evidence))
                .expect("content"),
        );
        assert!(matches!(
            events[0],
            PresentationContentEvent::Pending { .. }
        ));
        let PresentationContentEvent::Loaded { entry, .. } = &events[1] else {
            panic!("expected loaded evidence: {:?}", events[1]);
        };
        assert_eq!(entry.path, EVIDENCE);
        assert!(matches!(entry.body, PresentationFact::Available(ref bytes) if !bytes.is_empty()));
        assert!(matches!(
            entry.classification,
            PresentationFact::Available(Classification::Governed)
        ));
        assert!(matches!(
            entry.summary,
            PresentationFact::Available(Some(_))
        ));

        let by_path = drain_content(
            session
                .fetch_content(PresentationContentRequest {
                    generation: 7,
                    target: PresentationTarget::Store,
                    selector: PresentationContentSelector::Path { path: RAW.into() },
                })
                .expect("content by path"),
        );
        assert!(matches!(
            by_path[0],
            PresentationContentEvent::Pending { .. }
        ));
        assert!(matches!(
            by_path[1],
            PresentationContentEvent::Loaded { .. }
        ));

        for mutation in ["replace", "remove", "error", "symlink"] {
            let (reader, session, entries) = loaded_fake();
            let handle = entries
                .iter()
                .find(|entry| entry.path == RAW)
                .expect("raw")
                .content_handle
                .clone()
                .expect("handle");
            match mutation {
                "replace" => reader.replace(RAW, vec![b'y'; 1024 * 1024]),
                "remove" => reader.remove(RAW),
                "error" => reader.fail_read(RAW),
                "symlink" => reader.symlink(RAW),
                _ => unreachable!(),
            }
            let before_bodies = body_reads(&reader, RAW);
            let events = drain_content(
                session
                    .fetch_content(content_request(8, handle))
                    .expect("content"),
            );
            assert!(matches!(
                events[0],
                PresentationContentEvent::Pending { .. }
            ));
            assert!(matches!(
                events[1],
                PresentationContentEvent::Failure { .. }
            ));
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, PresentationContentEvent::Loaded { .. }))
            );
            if mutation == "replace" || mutation == "remove" || mutation == "symlink" {
                assert_eq!(body_reads(&reader, RAW), before_bodies);
            }
        }

        let empty = entries
            .iter()
            .find(|entry| entry.path.ends_with("empty.raw"))
            .expect("empty")
            .content_handle
            .clone()
            .expect("empty handle");
        let events = drain_content(
            session
                .fetch_content(content_request(9, empty))
                .expect("empty content"),
        );
        assert!(matches!(
            events[1],
            PresentationContentEvent::Failure { .. }
        ));
        assert!(
            reader
                .operations()
                .iter()
                .any(|operation| matches!(operation, Operation::Body(path) if path == EVIDENCE))
        );
    }

    #[test]
    fn lazy_content_rejects_non_emitted_escaping_and_excluded_paths() {
        let (_, session, entries) = loaded_fake();
        for path in ["../escape", ".git/config", "not-emitted.txt"] {
            let stream = session
                .fetch_content(PresentationContentRequest {
                    generation: 12,
                    target: PresentationTarget::Store,
                    selector: PresentationContentSelector::Path { path: path.into() },
                })
                .expect("content failure stream");
            let events = drain_content(stream);
            assert_eq!(events.len(), 1);
            assert!(matches!(
                events[0],
                PresentationContentEvent::Failure { .. }
            ));
        }

        let foreign = entries
            .iter()
            .find(|entry| entry.path == RAW)
            .expect("raw")
            .content_handle
            .clone()
            .expect("handle");
        let (_, other_session, _) = loaded_fake();
        let events = drain_content(
            other_session
                .fetch_content(content_request(12, foreign))
                .expect("foreign content failure"),
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PresentationContentEvent::Failure { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn production_fetch_rejects_a_catalogued_path_whose_parent_becomes_a_symlink() {
        let root = fixture();
        let raw_path = root.path().join(INVESTIGATION).join("lazy.raw");
        fs::write(&raw_path, "original").expect("raw");
        let store = crate::Store::open(root.path()).expect("store");
        let session = store.presentation_session();
        let entries = event_entries(&drain(
            session
                .load(load_request(13, PresentationTarget::Store))
                .expect("load"),
        ));
        let handle = entries
            .iter()
            .find(|entry| entry.path.ends_with("lazy.raw"))
            .expect("lazy entry")
            .content_handle
            .clone()
            .expect("handle");

        let investigation = root.path().join(INVESTIGATION);
        let moved = root.path().join("moved-investigation");
        fs::rename(&investigation, &moved).expect("move investigation");
        let outside = TempDir::new().expect("outside");
        fs::write(outside.path().join("lazy.raw"), "escaped").expect("outside raw");
        std::os::unix::fs::symlink(outside.path(), &investigation).expect("swap parent");

        let events = drain_content(
            session
                .fetch_content(content_request(13, handle))
                .expect("content"),
        );
        assert!(matches!(
            events[0],
            PresentationContentEvent::Pending { .. }
        ));
        assert!(matches!(
            events[1],
            PresentationContentEvent::Failure { .. }
        ));
    }

    #[test]
    fn unactivated_and_invalid_activation_complete_before_filesystem_catalogue_reads() {
        for state in [ActivationState::Unactivated, ActivationState::Invalid] {
            let reader = FakeReader::early(state);
            let session = PresentationSession::with_reader(reader.clone());
            let events = drain(
                session
                    .load(load_request(3, PresentationTarget::Store))
                    .expect("early load"),
            );
            assert_eq!(events.len(), 2);
            match &events[0] {
                PresentationEvent::Catalogue { catalogue, .. } => {
                    assert_eq!(catalogue.activation, state);
                    assert!(catalogue.projects.is_empty());
                    assert_eq!(
                        catalogue.diagnostics.is_empty(),
                        state == ActivationState::Unactivated
                    );
                }
                other => panic!("unexpected early event: {other:?}"),
            }
            assert!(matches!(events[1], PresentationEvent::Complete { .. }));
            assert_eq!(reader.operations(), vec![Operation::Activation]);
        }
    }

    #[test]
    fn governed_payload_failure_is_explicit_and_does_not_complete() {
        let reader = FakeReader::active();
        reader.fail_read(TICKET);
        let session = PresentationSession::with_reader(reader);
        let events = drain(
            session
                .load(load_request(4, PresentationTarget::Store))
                .expect("failed load stream"),
        );
        assert!(matches!(
            events.first(),
            Some(PresentationEvent::Catalogue { .. })
        ));
        assert!(
            matches!(events.last(), Some(PresentationEvent::Failure { message, .. }) if message.contains("injected read failure"))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, PresentationEvent::Complete { .. }))
        );
    }

    #[test]
    fn batches_and_channels_are_bounded_and_cancellation_stops_backpressure() {
        let reader = FakeReader::many_raw(PRESENTATION_BATCH_LIMIT * 4);
        let session = PresentationSession::with_reader(reader);
        let stream = session
            .load(load_request(15, PresentationTarget::Store))
            .expect("load");

        while stream.try_recv().is_err() {
            thread::yield_now();
        }
        while stream.try_recv().is_err() {
            thread::yield_now();
        }
        stream.cancel();
        while !stream.is_finished() {
            thread::yield_now();
        }
        let queued = std::iter::from_fn(|| stream.try_recv().ok()).collect::<Vec<_>>();
        assert!(queued.len() <= PRESENTATION_CHANNEL_CAPACITY);
        assert!(queued.iter().all(|event| {
            !matches!(event, PresentationEvent::Entries { entries, .. } if entries.len() > PRESENTATION_BATCH_LIMIT)
        }));
    }

    #[test]
    fn scoped_targets_emit_only_their_deterministic_activated_subtrees() {
        let reader = FakeReader::active();
        for target in [
            PresentationTarget::Project {
                project: "demo".into(),
            },
            PresentationTarget::Investigation {
                project: "demo".into(),
                path: INVESTIGATION.into(),
            },
        ] {
            let paths = |generation| {
                let session = PresentationSession::with_reader(reader.clone());
                let events = drain(
                    session
                        .load(load_request(generation, target.clone()))
                        .expect("scoped load"),
                );
                assert!(events.iter().all(|event| event.target() == &target));
                event_entries(&events)
                    .into_iter()
                    .map(|entry| entry.path)
                    .collect::<Vec<_>>()
            };
            let first = paths(30);
            let second = paths(31);
            assert_eq!(first, second);
            assert!(first.windows(2).all(|pair| pair[0] <= pair[1]));
            assert!(first.iter().all(|path| target_contains(&target, path)));
        }
    }

    #[test]
    fn scoped_load_keeps_untouched_target_content_handles_usable() {
        let reader = FakeReader::active();
        let session = PresentationSession::with_reader(reader);
        let store_entries = event_entries(&drain(
            session
                .load(load_request(32, PresentationTarget::Store))
                .expect("store load"),
        ));
        let handle = store_entries
            .iter()
            .find(|entry| entry.path == RAW)
            .expect("raw")
            .content_handle
            .clone()
            .expect("handle");
        drain(
            session
                .load(load_request(
                    33,
                    PresentationTarget::Investigation {
                        project: "demo".into(),
                        path: INVESTIGATION.into(),
                    },
                ))
                .expect("scoped load"),
        );

        let events = drain_content(
            session
                .fetch_content(content_request(32, handle))
                .expect("untouched content"),
        );
        assert!(matches!(events[1], PresentationContentEvent::Loaded { .. }));
    }

    #[test]
    fn scoped_cache_completion_replaces_deletions_and_preserves_untouched_scopes() {
        let mut cache = PresentationCache::default();
        let store = PresentationTarget::Store;
        apply_replacement(
            &mut cache,
            1,
            store,
            vec![
                dummy_entry("projects/demo/investigations/sample/one.raw"),
                dummy_entry("projects/demo/investigations/sample/deleted.raw"),
                dummy_entry("projects/demo/project.raw"),
                dummy_entry("projects/other/untouched.raw"),
            ],
        );
        let investigation = PresentationTarget::Investigation {
            project: "demo".into(),
            path: INVESTIGATION.into(),
        };
        apply_replacement(
            &mut cache,
            2,
            investigation,
            vec![
                dummy_entry("projects/demo/investigations/sample/one.raw"),
                dummy_entry("projects/demo/investigations/sample/new.raw"),
            ],
        );

        assert!(
            cache
                .get("projects/demo/investigations/sample/deleted.raw")
                .is_none()
        );
        assert!(
            cache
                .get("projects/demo/investigations/sample/new.raw")
                .is_some()
        );
        assert!(cache.get("projects/demo/project.raw").is_some());
        assert!(cache.get("projects/other/untouched.raw").is_some());

        let existing = cache
            .entries()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let failed_target = PresentationTarget::Project {
            project: "demo".into(),
        };
        let coverage = complete_coverage();
        let progress = PresentationProgress {
            completed: 0,
            total: Some(0),
        };
        cache.apply(&PresentationEvent::Catalogue {
            generation: 3,
            target: failed_target.clone(),
            coverage: coverage.clone(),
            progress: progress.clone(),
            catalogue: PresentationCatalogue {
                activation: ActivationState::Active,
                projects: Vec::new(),
                diagnostics: Vec::new(),
            },
        });
        cache.apply(&PresentationEvent::Failure {
            generation: 3,
            target: failed_target,
            coverage,
            progress,
            message: "injected".into(),
        });
        assert_eq!(
            cache
                .entries()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
            existing
        );

        let path = "projects/demo/investigations/sample/one.raw";
        let mut loaded = cache.get(path).expect("cached raw").clone();
        loaded.body = PresentationFact::Available(b"loaded".to_vec());
        cache.apply_content(&PresentationContentEvent::Loaded {
            generation: 2,
            target: PresentationTarget::Store,
            entry: Box::new(loaded),
        });
        assert!(matches!(
            cache.get(path).expect("loaded raw").body,
            PresentationFact::Available(_)
        ));
        cache.apply_content(&PresentationContentEvent::Pending {
            generation: 3,
            target: PresentationTarget::Store,
            path: path.into(),
        });
        assert_eq!(
            cache.get(path).expect("pending raw").body,
            PresentationFact::Unavailable
        );
    }

    #[test]
    fn loaded_presentation_facts_match_complete_canonical_inputs() {
        let root = fixture();
        let store = crate::Store::open(root.path()).expect("store");
        let canonical = store.scan().expect("canonical scan");
        let derived = store.derive_snapshot(&canonical);
        let session = store.presentation_session();
        let events = drain(
            session
                .load(load_request(22, PresentationTarget::Store))
                .expect("presentation load"),
        );
        let entries = event_entries(&events);
        let catalogue = match &events[0] {
            PresentationEvent::Catalogue { catalogue, .. } => catalogue,
            other => panic!("catalogue was not first: {other:?}"),
        };
        assert_eq!(catalogue.activation, canonical.activation);
        assert_eq!(catalogue.projects[0].slug, "demo");
        let encoded = serde_json::to_string(&events).expect("presentation JSON");
        for forbidden in ["source_revision", "original_bytes", "snapshot"] {
            assert!(!encoded.contains(forbidden), "leaked {forbidden}");
        }

        for expected in &canonical.snapshot.entries {
            let actual = entries
                .iter()
                .find(|entry| entry.path == expected.path)
                .unwrap_or_else(|| panic!("missing presentation entry {}", expected.path));
            assert_eq!(actual.path, expected.path);
            assert_eq!(actual.kind, expected.kind);
            let expected_scope =
                canonical
                    .scope_for_path(&expected.path)
                    .map(|(project, investigation)| PresentationScope {
                        project: project.into(),
                        investigation: investigation.map(Into::into),
                    });
            assert_eq!(actual.scope, expected_scope);
            if expected.kind == Some(Kind::Evidence) {
                assert_eq!(actual.classification, PresentationFact::Unavailable);
                assert_eq!(actual.summary, PresentationFact::Unavailable);
                assert_eq!(actual.body, PresentationFact::Unavailable);
                continue;
            }
            assert_eq!(
                actual.classification,
                PresentationFact::Available(expected.classification)
            );
            assert_eq!(
                actual.identity,
                PresentationFact::Available(expected.identity.clone())
            );
            if expected.kind.is_none() {
                assert_eq!(actual.body, PresentationFact::Unavailable);
                continue;
            }
            assert!(
                matches!(&actual.summary, PresentationFact::Available(summary) if summary.as_ref().map(|value| &value.record) == expected.summary.as_ref())
            );
            assert_eq!(
                actual.body,
                PresentationFact::Available(expected.original_bytes.clone())
            );
            let expected_diagnostics = canonical
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.path == expected.path)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                actual.diagnostics,
                PresentationFact::Available(expected_diagnostics)
            );
            let record = derived
                .records
                .iter()
                .find(|record| record.path == expected.path)
                .expect("derived record");
            assert_eq!(
                actual.progress,
                PresentationFact::Available(record.progress.clone())
            );
            let relationships = record
                .identity
                .as_ref()
                .map(|identity| {
                    derived
                        .relationships
                        .iter()
                        .filter(|relationship| &relationship.source == identity)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            assert_eq!(
                actual.relationships,
                PresentationFact::Available(relationships)
            );
            let boards = record
                .identity
                .as_ref()
                .map(|identity| {
                    derived
                        .boards
                        .iter()
                        .filter(|board| &board.identity == identity)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            assert_eq!(actual.boards, PresentationFact::Available(boards));
        }

        let evidence = entries
            .iter()
            .find(|entry| entry.kind == Some(Kind::Evidence))
            .expect("evidence");
        let content = drain_content(
            session
                .fetch_content(content_request(
                    22,
                    evidence.content_handle.clone().expect("evidence handle"),
                ))
                .expect("evidence fetch"),
        );
        let PresentationContentEvent::Loaded { entry, .. } = &content[1] else {
            panic!("evidence was not loaded");
        };
        let expected = canonical
            .snapshot
            .entries
            .iter()
            .find(|expected| expected.path == entry.path)
            .expect("canonical evidence");
        assert_eq!(
            entry.classification,
            PresentationFact::Available(expected.classification)
        );
        assert!(
            matches!(&entry.summary, PresentationFact::Available(summary) if summary.as_ref().map(|value| &value.record) == expected.summary.as_ref())
        );
        assert_eq!(
            entry.body,
            PresentationFact::Available(expected.original_bytes.clone())
        );
        let expected_diagnostics = canonical
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.path == expected.path)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            entry.diagnostics,
            PresentationFact::Available(expected_diagnostics)
        );
    }

    fn active_activation() -> Activation {
        Activation {
            schema_version: Some(1),
            projects: BTreeMap::from([(
                "demo".into(),
                Project {
                    prefix: "HMD".into(),
                    investigations: vec![INVESTIGATION.into()],
                },
            )]),
        }
    }

    fn insert_parent_directories(nodes: &mut BTreeMap<String, FakeNode>, path: &str) {
        let mut parent = path;
        while let Some((value, _)) = parent.rsplit_once('/') {
            nodes.entry(value.into()).or_insert(FakeNode::Directory);
            parent = value;
        }
    }

    fn not_found() -> StoreError {
        io::Error::new(io::ErrorKind::NotFound, "fake path missing").into()
    }

    fn load_request(generation: u64, target: PresentationTarget) -> PresentationLoadRequest {
        PresentationLoadRequest { generation, target }
    }

    fn content_request(
        generation: u64,
        handle: PresentationContentHandle,
    ) -> PresentationContentRequest {
        PresentationContentRequest {
            generation,
            target: PresentationTarget::Store,
            selector: PresentationContentSelector::Handle { handle },
        }
    }

    fn drain(stream: PresentationStream) -> Vec<PresentationEvent> {
        let mut events = Vec::new();
        while let Ok(event) = stream.recv() {
            let finished = matches!(
                event,
                PresentationEvent::Complete { .. } | PresentationEvent::Failure { .. }
            );
            events.push(event);
            if finished {
                break;
            }
        }
        events
    }

    fn drain_content(stream: PresentationContentStream) -> Vec<PresentationContentEvent> {
        let mut events = Vec::new();
        while let Ok(event) = stream.recv() {
            let finished = matches!(
                event,
                PresentationContentEvent::Loaded { .. } | PresentationContentEvent::Failure { .. }
            );
            events.push(event);
            if finished {
                break;
            }
        }
        events
    }

    fn event_entries(events: &[PresentationEvent]) -> Vec<PresentationEntry> {
        events
            .iter()
            .filter_map(|event| match event {
                PresentationEvent::Entries { entries, .. } => Some(entries.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn loaded_fake() -> (Arc<FakeReader>, PresentationSession, Vec<PresentationEntry>) {
        let reader = FakeReader::active();
        let session = PresentationSession::with_reader(reader.clone());
        let events = drain(
            session
                .load(load_request(7, PresentationTarget::Store))
                .expect("load"),
        );
        (reader, session, event_entries(&events))
    }

    fn body_reads(reader: &FakeReader, path: &str) -> usize {
        reader
            .operations()
            .iter()
            .filter(|operation| matches!(operation, Operation::Body(value) if value == path))
            .count()
    }

    fn dummy_entry(path: &str) -> PresentationEntry {
        PresentationEntry {
            path: path.into(),
            scope: None,
            kind: None,
            metadata: PresentationFileMetadata {
                kind: PresentationFileKind::Regular,
                length: 1,
                modified_unix_nanos: None,
            },
            content_handle: None,
            classification: PresentationFact::Available(Classification::Raw),
            identity: PresentationFact::Available(None),
            summary: PresentationFact::Available(None),
            diagnostics: PresentationFact::Available(Vec::new()),
            progress: PresentationFact::Available(None),
            relationships: PresentationFact::Available(Vec::new()),
            boards: PresentationFact::Available(Vec::new()),
            body: PresentationFact::Unavailable,
        }
    }

    fn apply_replacement(
        cache: &mut PresentationCache,
        generation: u64,
        target: PresentationTarget,
        entries: Vec<PresentationEntry>,
    ) {
        let coverage = complete_coverage();
        let progress = PresentationProgress {
            completed: entries.len(),
            total: Some(entries.len()),
        };
        cache.apply(&PresentationEvent::Catalogue {
            generation,
            target: target.clone(),
            coverage: coverage.clone(),
            progress: progress.clone(),
            catalogue: PresentationCatalogue {
                activation: ActivationState::Active,
                projects: Vec::new(),
                diagnostics: Vec::new(),
            },
        });
        cache.apply(&PresentationEvent::Entries {
            generation,
            target: target.clone(),
            coverage: coverage.clone(),
            progress: progress.clone(),
            entries,
        });
        cache.apply(&PresentationEvent::Complete {
            generation,
            target,
            coverage,
            progress,
        });
    }

    fn complete_coverage() -> PresentationCoverage {
        PresentationCoverage {
            catalogue: PresentationCoverageState::Complete,
            payload: PresentationCoverageState::Complete,
            facts: PresentationCoverageState::Complete,
        }
    }

    fn fixture() -> TempDir {
        let temporary = TempDir::new().expect("temporary root");
        copy_tree(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/minimum")
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
