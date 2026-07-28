use crate::{
    ActivationState, DerivedBoard, DerivedIndex, DerivedRecord, DerivedSnapshot,
    GovernedApplyResult, Indexed, ProgressApplyResult, ProgressChangeRequest, ProgressPreview,
    RecordScope, RevisionSource, ScanResult, Store, StoreError, StrategyTransitionPreview,
    StrategyTransitionRequest, WriterBindingPreview, WriterBindingRequest,
};
use casefile_core::{
    ApplyResult, BoardColumn, BoardDraft, BoardStatusSource, ChangeBatchApplyResult,
    ChangeBatchPreview, ChangeRequest, Diagnostic, Kind, Preview, ProgressEntry, RecordDraft,
    RecordSummary, Revision, StrategyTransitionRecord,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    sync::Mutex,
};
use thiserror::Error;

pub const PROVIDER_PROTOCOL_VERSION: u32 = 1;
const PREVIEW_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperation {
    Snapshot,
    QueryTickets,
    QueryEpics,
    QueryBoards,
    QueryProgress,
    QueryStrategyTransitions,
    PreviewRecordDraft,
    ApplyRecordDraft,
    BootstrapProgress,
    PreviewProgress,
    ApplyProgress,
    PreviewDefaultDeliveryBoard,
    ApplyDefaultDeliveryBoard,
    PreviewStrategyTransition,
    ApplyStrategyTransition,
    PreviewWriterBinding,
    ApplyWriterBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProviderMutationState {
    ReadWrite,
    ReadOnly { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub protocol_version: u32,
    pub planning_format_versions: Vec<u32>,
    pub mutation: ProviderMutationState,
    pub operations: Vec<ProviderOperation>,
    pub writes_require_external_approval: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressProjection {
    pub record: DerivedRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderProjections {
    pub tickets: Vec<DerivedRecord>,
    pub epics: Vec<DerivedRecord>,
    pub boards: Vec<DerivedBoard>,
    pub progress: Vec<ProgressProjection>,
    pub strategy_transitions: Vec<StrategyTransitionProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyTransitionProjection {
    pub path: String,
    pub scope: RecordScope,
    pub record: StrategyTransitionRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CacheState {
    NotConfigured,
    Current { source_revision: Revision },
    Degraded { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub capabilities: ProviderCapabilities,
    pub activation: ActivationState,
    pub revision: Revision,
    pub diagnostics: Vec<Diagnostic>,
    pub projections: ProviderProjections,
    pub cache: CacheState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum ProviderQuery {
    Tickets {
        scope: Option<RecordScope>,
        search: Option<String>,
    },
    Epics {
        scope: Option<RecordScope>,
        search: Option<String>,
    },
    Boards {
        scope: Option<RecordScope>,
    },
    Progress {
        scope: Option<RecordScope>,
    },
    StrategyTransitions {
        scope: Option<RecordScope>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ProviderQueryResult {
    Records {
        revision: Revision,
        records: Vec<DerivedRecord>,
    },
    Boards {
        revision: Revision,
        boards: Vec<DerivedBoard>,
    },
    Progress {
        revision: Revision,
        progress: Vec<ProgressProjection>,
    },
    StrategyTransitions {
        revision: Revision,
        transitions: Vec<StrategyTransitionProjection>,
    },
}

pub trait ProviderCache {
    fn refresh(
        &self,
        snapshot: &DerivedSnapshot,
        source: &dyn RevisionSource,
    ) -> Result<(), String>;
    fn configured(&self) -> bool {
        true
    }
}

impl<T> ProviderCache for T
where
    T: DerivedIndex,
    T::Error: Display,
{
    fn refresh(
        &self,
        snapshot: &DerivedSnapshot,
        source: &dyn RevisionSource,
    ) -> Result<(), String> {
        match self
            .state(&snapshot.source_revision)
            .map_err(|error| error.to_string())?
        {
            Indexed::Current { .. } => Ok(()),
            Indexed::Missing | Indexed::Stale { .. } => {
                let prepared = self.prepare(snapshot).map_err(|error| error.to_string())?;
                match self
                    .publish(prepared, source)
                    .map_err(|error| error.to_string())?
                {
                    Indexed::Current { .. } => Ok(()),
                    Indexed::Missing | Indexed::Stale { .. } => {
                        Err("canonical content changed during cache refresh".into())
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoCache;
impl ProviderCache for NoCache {
    fn refresh(&self, _: &DerivedSnapshot, _: &dyn RevisionSource) -> Result<(), String> {
        Ok(())
    }
    fn configured(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderPreview {
    pub preview_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_bytes: Option<Vec<u8>>,
    #[serde(flatten)]
    pub canonical: Preview,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderBatchPreview {
    pub preview_id: String,
    pub rendered_bytes: Vec<Option<Vec<u8>>>,
    #[serde(flatten)]
    pub canonical: ChangeBatchPreview,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ProgressOperation {
    Bootstrap {
        investigation: String,
    },
    Append {
        investigation: String,
        entries: Vec<ProgressEntry>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderProgressPreview {
    pub preview_id: String,
    pub operation: ProgressOperation,
    pub canonical: ProgressPreview,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderStrategyTransitionPreview {
    pub preview_id: String,
    pub canonical: StrategyTransitionPreview,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderWriterBindingPreview {
    pub preview_id: String,
    pub canonical: WriterBindingPreview,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DefaultBoardPreview {
    pub preview_id: String,
    pub investigation: String,
    pub canonical: Preview,
    pub rendered_bytes: Vec<u8>,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DefaultBoardApplyResult {
    #[serde(flatten)]
    pub result: ApplyResult,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecordApplyResult {
    #[serde(flatten)]
    pub result: ApplyResult,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecordBatchApplyResult {
    #[serde(flatten)]
    pub result: ChangeBatchApplyResult,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderApplyOutcome<T> {
    pub result: T,
    pub cache: CacheState,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("governed provider mutation is unavailable: {0}")]
    ReadOnly(String),
    #[error("provider preview is unknown, expired, or was altered")]
    PreviewIntegrity,
    #[error("default delivery-board mapping is invalid: {0}")]
    DefaultBoardMapping(String),
    #[error("unsupported provider protocol version {requested}; supported version is {supported}")]
    UnsupportedProtocol { requested: u32, supported: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoredPreview {
    Record(Preview, Option<Vec<u8>>, bool),
    RecordBatch(ChangeBatchPreview, Vec<Option<Vec<u8>>>, bool),
    Progress(ProgressOperation, ProgressPreview),
    Board(String, Preview, Vec<u8>, bool),
    StrategyTransition(StrategyTransitionPreview),
    WriterBinding(WriterBindingPreview),
}

#[derive(Default)]
struct PreviewVault {
    next: u64,
    order: VecDeque<String>,
    values: BTreeMap<String, StoredPreview>,
}

pub struct Provider<C = NoCache> {
    store: Store,
    cache: C,
    previews: Mutex<PreviewVault>,
}

impl Provider<NoCache> {
    pub fn without_cache(store: Store) -> Self {
        Self::new(store, NoCache)
    }
}

impl<C: ProviderCache> Provider<C> {
    pub fn new(store: Store, cache: C) -> Self {
        Self {
            store,
            cache,
            previews: Mutex::new(PreviewVault::default()),
        }
    }

    pub fn snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let scan = self.store.scan()?;
        Ok(self.snapshot_from_scan(scan))
    }

    pub fn snapshot_for_protocol(
        &self,
        protocol_version: u32,
    ) -> Result<ProviderSnapshot, ProviderError> {
        if protocol_version != PROVIDER_PROTOCOL_VERSION {
            return Err(ProviderError::UnsupportedProtocol {
                requested: protocol_version,
                supported: PROVIDER_PROTOCOL_VERSION,
            });
        }
        self.snapshot()
    }

    fn snapshot_from_scan(&self, scan: ScanResult) -> ProviderSnapshot {
        let derived = self.store.derive_snapshot(&scan);
        let cache = self.refresh_cache(&derived);
        let projections = projections(&derived, &scan);
        ProviderSnapshot {
            capabilities: capabilities(scan.activation),
            activation: scan.activation,
            revision: scan.snapshot.revision,
            diagnostics: scan.diagnostics,
            projections,
            cache,
        }
    }

    pub fn query(&self, query: ProviderQuery) -> Result<ProviderQueryResult, ProviderError> {
        let baseline = self.snapshot()?;
        let revision = baseline.revision.clone();
        Ok(match query {
            ProviderQuery::Tickets { scope, search } => ProviderQueryResult::Records {
                revision,
                records: filter_records(
                    baseline.projections.tickets,
                    scope.as_ref(),
                    search.as_deref(),
                ),
            },
            ProviderQuery::Epics { scope, search } => ProviderQueryResult::Records {
                revision,
                records: filter_records(
                    baseline.projections.epics,
                    scope.as_ref(),
                    search.as_deref(),
                ),
            },
            ProviderQuery::Boards { scope } => ProviderQueryResult::Boards {
                revision,
                boards: baseline
                    .projections
                    .boards
                    .into_iter()
                    .filter(|board| {
                        scope
                            .as_ref()
                            .is_none_or(|scope| &board.identity.scope == scope)
                    })
                    .collect(),
            },
            ProviderQuery::Progress { scope } => ProviderQueryResult::Progress {
                revision,
                progress: baseline
                    .projections
                    .progress
                    .into_iter()
                    .filter(|item| {
                        scope
                            .as_ref()
                            .is_none_or(|scope| item.record.scope.as_ref() == Some(scope))
                    })
                    .collect(),
            },
            ProviderQuery::StrategyTransitions { scope } => {
                ProviderQueryResult::StrategyTransitions {
                    revision,
                    transitions: baseline
                        .projections
                        .strategy_transitions
                        .into_iter()
                        .filter(|item| scope.as_ref().is_none_or(|scope| &item.scope == scope))
                        .collect(),
                }
            }
        })
    }

    pub fn preview_record(&self, request: ChangeRequest) -> Result<ProviderPreview, ProviderError> {
        self.require_mutation()?;
        let canonical = self.store.preview(request)?;
        let rendered_bytes = canonical
            .request
            .rendered()
            .transpose()
            .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?;
        let no_op = canonical.diff.is_empty() && canonical.diagnostics.is_empty();
        let preview_id = self.remember(StoredPreview::Record(
            canonical.clone(),
            rendered_bytes.clone(),
            no_op,
        ));
        Ok(ProviderPreview {
            preview_id,
            rendered_bytes,
            canonical,
            no_op,
        })
    }

    pub fn apply_record(
        &self,
        preview: ProviderPreview,
    ) -> Result<ProviderApplyOutcome<ProviderRecordApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::Record(
                preview.canonical.clone(),
                preview.rendered_bytes,
                preview.no_op,
            ),
        )?;
        let result = if preview.no_op {
            let current = self.store.preview(preview.canonical.request.clone())?;
            if current != preview.canonical {
                return Err(StoreError::StaleTargetRevision.into());
            }
            ApplyResult {
                path: current.request.path().into(),
                resulting_target_revision: current.expected_target_revision,
                resulting_store_revision: self.store.scan()?.snapshot.revision,
                diff: String::new(),
            }
        } else {
            self.store.apply(preview.canonical)?
        };
        self.outcome(ProviderRecordApplyResult {
            result,
            no_op: preview.no_op,
        })
    }

    pub fn preview_record_batch(
        &self,
        requests: Vec<ChangeRequest>,
    ) -> Result<ProviderBatchPreview, ProviderError> {
        self.require_mutation()?;
        let canonical = self.store.preview_batch(requests)?;
        let rendered_bytes = canonical
            .requests
            .iter()
            .map(ChangeRequest::rendered)
            .map(Option::transpose)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?;
        let no_op = canonical.diff.is_empty() && canonical.diagnostics.is_empty();
        let preview_id = self.remember(StoredPreview::RecordBatch(
            canonical.clone(),
            rendered_bytes.clone(),
            no_op,
        ));
        Ok(ProviderBatchPreview {
            preview_id,
            rendered_bytes,
            canonical,
            no_op,
        })
    }

    pub fn apply_record_batch(
        &self,
        preview: ProviderBatchPreview,
    ) -> Result<ProviderApplyOutcome<ProviderRecordBatchApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::RecordBatch(
                preview.canonical.clone(),
                preview.rendered_bytes,
                preview.no_op,
            ),
        )?;
        let result = if preview.no_op {
            let current = self
                .store
                .preview_batch(preview.canonical.requests.clone())?;
            if current != preview.canonical {
                return Err(StoreError::StaleTargetRevision.into());
            }
            ChangeBatchApplyResult {
                paths: current
                    .requests
                    .iter()
                    .map(|request| request.path().to_owned())
                    .collect(),
                resulting_target_revisions: current.expected_target_revisions,
                resulting_store_revision: self.store.scan()?.snapshot.revision,
                diff: String::new(),
            }
        } else {
            self.store.apply_batch(preview.canonical)?
        };
        self.outcome(ProviderRecordBatchApplyResult {
            result,
            no_op: preview.no_op,
        })
    }

    pub fn bootstrap_progress(
        &self,
        investigation: impl Into<String>,
    ) -> Result<ProviderProgressPreview, ProviderError> {
        self.preview_progress(ProgressOperation::Bootstrap {
            investigation: investigation.into(),
        })
    }

    pub fn preview_progress(
        &self,
        operation: ProgressOperation,
    ) -> Result<ProviderProgressPreview, ProviderError> {
        self.require_mutation()?;
        let request = match &operation {
            ProgressOperation::Bootstrap { investigation } => {
                self.store.bootstrap_progress(investigation)?
            }
            ProgressOperation::Append {
                investigation,
                entries,
            } => ProgressChangeRequest {
                investigation: investigation.clone(),
                entries: entries.clone(),
                replacement: None,
                replacement_source: None,
                bootstrap: false,
            },
        };
        let canonical = self.store.preview_progress(request)?;
        let preview_id = self.remember(StoredPreview::Progress(
            operation.clone(),
            canonical.clone(),
        ));
        Ok(ProviderProgressPreview {
            preview_id,
            operation,
            canonical,
        })
    }

    pub fn apply_progress(
        &self,
        preview: ProviderProgressPreview,
    ) -> Result<ProviderApplyOutcome<ProgressApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::Progress(preview.operation, preview.canonical.clone()),
        )?;
        let result = self.store.apply_progress(preview.canonical)?;
        self.outcome(result)
    }

    pub fn preview_default_delivery_board(
        &self,
        investigation: impl Into<String>,
    ) -> Result<DefaultBoardPreview, ProviderError> {
        let investigation = investigation.into().trim_end_matches('/').to_owned();
        let scan = self.require_mutation()?;
        let identity = default_board_identity(&scan, &investigation)?;
        let path = format!("{investigation}/boards/delivery.toml");
        let draft = RecordDraft::Board(default_board(identity));
        let existing = scan
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path);
        let request = if existing.is_some() {
            ChangeRequest::Replace {
                path: path.clone(),
                draft,
            }
        } else {
            ChangeRequest::Create {
                path: path.clone(),
                draft,
            }
        };
        let scoped = format!("{investigation}/");
        let scoped_diagnostics = scan
            .diagnostics
            .iter()
            .filter(|item| item.path.starts_with(&scoped))
            .cloned()
            .collect::<Vec<_>>();
        let mut canonical = if scoped_diagnostics.is_empty() {
            self.store.preview(request)?
        } else {
            Preview {
                request,
                expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
                diagnostics: scoped_diagnostics,
                diff: String::new(),
            }
        };
        if existing.is_some() && !canonical.diff.is_empty() && canonical.diagnostics.is_empty() {
            canonical.diagnostics.push(Diagnostic::new(
                &path,
                "default_board_collision",
                "delivery.toml already differs; the existing board was preserved",
            ));
        }
        let no_op =
            existing.is_some() && canonical.diff.is_empty() && canonical.diagnostics.is_empty();
        let rendered_bytes = canonical
            .request
            .rendered()
            .transpose()
            .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?
            .expect("board renders bytes");
        let preview_id = self.remember(StoredPreview::Board(
            investigation.clone(),
            canonical.clone(),
            rendered_bytes.clone(),
            no_op,
        ));
        Ok(DefaultBoardPreview {
            preview_id,
            investigation,
            canonical,
            rendered_bytes,
            no_op,
        })
    }

    pub fn apply_default_delivery_board(
        &self,
        preview: DefaultBoardPreview,
    ) -> Result<ProviderApplyOutcome<DefaultBoardApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::Board(
                preview.investigation,
                preview.canonical.clone(),
                preview.rendered_bytes,
                preview.no_op,
            ),
        )?;
        if !preview.canonical.diagnostics.is_empty() {
            return Err(StoreError::Invalid(
                "default delivery-board preview contains diagnostics".into(),
            )
            .into());
        }
        let result = if preview.no_op {
            let current = self.store.preview(preview.canonical.request.clone())?;
            if current != preview.canonical {
                return Err(StoreError::StaleTargetRevision.into());
            }
            ApplyResult {
                path: current.request.path().into(),
                resulting_target_revision: current.expected_target_revision,
                resulting_store_revision: self.store.scan()?.snapshot.revision,
                diff: String::new(),
            }
        } else {
            self.store.apply(preview.canonical)?
        };
        self.outcome(DefaultBoardApplyResult {
            result,
            no_op: preview.no_op,
        })
    }

    pub fn preview_strategy_transition(
        &self,
        request: StrategyTransitionRequest,
    ) -> Result<ProviderStrategyTransitionPreview, ProviderError> {
        self.require_mutation()?;
        let canonical = self.store.preview_strategy_transition(request)?;
        let preview_id = self.remember(StoredPreview::StrategyTransition(canonical.clone()));
        Ok(ProviderStrategyTransitionPreview {
            preview_id,
            canonical,
        })
    }

    pub fn apply_strategy_transition(
        &self,
        preview: ProviderStrategyTransitionPreview,
    ) -> Result<ProviderApplyOutcome<GovernedApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::StrategyTransition(preview.canonical.clone()),
        )?;
        let result = self.store.apply_strategy_transition(preview.canonical)?;
        self.outcome(result)
    }

    pub fn preview_writer_binding(
        &self,
        request: WriterBindingRequest,
    ) -> Result<ProviderWriterBindingPreview, ProviderError> {
        self.require_mutation()?;
        let canonical = self.store.preview_writer_binding(request)?;
        let preview_id = self.remember(StoredPreview::WriterBinding(canonical.clone()));
        Ok(ProviderWriterBindingPreview {
            preview_id,
            canonical,
        })
    }

    pub fn apply_writer_binding(
        &self,
        preview: ProviderWriterBindingPreview,
    ) -> Result<ProviderApplyOutcome<GovernedApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::WriterBinding(preview.canonical.clone()),
        )?;
        let result = self.store.apply_writer_binding(preview.canonical)?;
        self.outcome(result)
    }

    fn require_mutation(&self) -> Result<ScanResult, ProviderError> {
        let scan = self.store.scan()?;
        if scan.activation == ActivationState::Active {
            Ok(scan)
        } else {
            Err(ProviderError::ReadOnly(
                match scan.activation {
                    ActivationState::Unactivated => "planning Store is not activated",
                    ActivationState::Invalid => {
                        "planning Store activation is invalid or unsupported"
                    }
                    ActivationState::Active => unreachable!(),
                }
                .into(),
            ))
        }
    }

    fn remember(&self, value: StoredPreview) -> String {
        let mut vault = self.previews.lock().expect("preview vault");
        vault.next += 1;
        let id = format!("provider-preview-{}", vault.next);
        vault.order.push_back(id.clone());
        vault.values.insert(id.clone(), value);
        while vault.order.len() > PREVIEW_LIMIT {
            if let Some(expired) = vault.order.pop_front() {
                vault.values.remove(&expired);
            }
        }
        id
    }

    fn verify(&self, id: &str, expected: &StoredPreview) -> Result<(), ProviderError> {
        let vault = self.previews.lock().expect("preview vault");
        if vault.values.get(id) == Some(expected) {
            Ok(())
        } else {
            Err(ProviderError::PreviewIntegrity)
        }
    }

    fn refresh_cache(&self, derived: &DerivedSnapshot) -> CacheState {
        if !self.cache.configured() {
            return CacheState::NotConfigured;
        }
        match self.cache.refresh(derived, &self.store) {
            Ok(()) => CacheState::Current {
                source_revision: derived.source_revision.clone(),
            },
            Err(message) => CacheState::Degraded { message },
        }
    }

    fn outcome<T>(&self, result: T) -> Result<ProviderApplyOutcome<T>, ProviderError> {
        let scan = self.store.scan()?;
        let derived = self.store.derive_snapshot(&scan);
        Ok(ProviderApplyOutcome {
            result,
            cache: self.refresh_cache(&derived),
        })
    }
}

fn capabilities(activation: ActivationState) -> ProviderCapabilities {
    let reads = vec![
        ProviderOperation::Snapshot,
        ProviderOperation::QueryTickets,
        ProviderOperation::QueryEpics,
        ProviderOperation::QueryBoards,
        ProviderOperation::QueryProgress,
        ProviderOperation::QueryStrategyTransitions,
    ];
    let (mutation, operations) = if activation == ActivationState::Active {
        let mut operations = reads;
        operations.extend([
            ProviderOperation::PreviewRecordDraft,
            ProviderOperation::ApplyRecordDraft,
            ProviderOperation::BootstrapProgress,
            ProviderOperation::PreviewProgress,
            ProviderOperation::ApplyProgress,
            ProviderOperation::PreviewDefaultDeliveryBoard,
            ProviderOperation::ApplyDefaultDeliveryBoard,
            ProviderOperation::PreviewStrategyTransition,
            ProviderOperation::ApplyStrategyTransition,
            ProviderOperation::PreviewWriterBinding,
            ProviderOperation::ApplyWriterBinding,
        ]);
        (ProviderMutationState::ReadWrite, operations)
    } else {
        let reason = if activation == ActivationState::Unactivated {
            "planning Store is not activated"
        } else {
            "planning Store activation is invalid or unsupported"
        };
        (
            ProviderMutationState::ReadOnly {
                reason: reason.into(),
            },
            reads,
        )
    };
    ProviderCapabilities {
        protocol_version: PROVIDER_PROTOCOL_VERSION,
        planning_format_versions: vec![1],
        mutation,
        operations,
        writes_require_external_approval: true,
    }
}

fn projections(derived: &DerivedSnapshot, scan: &ScanResult) -> ProviderProjections {
    let tickets = derived
        .records
        .iter()
        .filter(|record| record.kind == Some(Kind::Ticket))
        .cloned()
        .collect::<Vec<_>>();
    let epics = derived
        .records
        .iter()
        .filter(|record| record.kind == Some(Kind::Epic))
        .cloned()
        .collect();
    let progress = tickets
        .iter()
        .filter(|record| record.progress.is_some())
        .cloned()
        .map(|record| ProgressProjection { record })
        .collect();
    ProviderProjections {
        tickets,
        epics,
        boards: derived.boards.clone(),
        progress,
        strategy_transitions: scan
            .snapshot
            .entries
            .iter()
            .filter_map(|entry| match &entry.summary {
                Some(RecordSummary::StrategyTransition { record }) => scan
                    .scope_for_path(&entry.path)
                    .and_then(|(project, investigation)| {
                        investigation.map(|investigation| StrategyTransitionProjection {
                            path: entry.path.clone(),
                            scope: RecordScope {
                                project: project.into(),
                                investigation: Some(investigation.into()),
                            },
                            record: record.as_ref().clone(),
                        })
                    }),
                _ => None,
            })
            .collect(),
    }
}

fn filter_records(
    mut records: Vec<DerivedRecord>,
    scope: Option<&RecordScope>,
    search: Option<&str>,
) -> Vec<DerivedRecord> {
    records.retain(|record| {
        scope.is_none_or(|scope| record.scope.as_ref() == Some(scope))
            && search.is_none_or(|text| {
                record
                    .search_text
                    .to_lowercase()
                    .contains(&text.to_lowercase())
            })
    });
    records
}

fn default_board_identity(scan: &ScanResult, investigation: &str) -> Result<String, ProviderError> {
    let activation = crate::activation::activation_from_scan(scan)?;
    let mut matches = Vec::new();
    let mut counts = BTreeMap::new();
    for project in activation.projects.values() {
        for activated in &project.investigations {
            let directory = activated
                .rsplit('/')
                .next()
                .filter(|value| !value.is_empty() && !matches!(*value, "." | ".."))
                .ok_or_else(|| {
                    ProviderError::DefaultBoardMapping(
                        "investigation has no safe directory identity".into(),
                    )
                })?;
            let identity = format!("{}-{directory}-delivery", project.prefix);
            *counts.entry(identity.clone()).or_insert(0usize) += 1;
            if activated == investigation {
                matches.push(identity);
            }
        }
    }
    if matches.len() != 1 {
        return Err(ProviderError::DefaultBoardMapping(
            "investigation must have exactly one activated project-prefix mapping".into(),
        ));
    }
    let identity = matches.pop().expect("one mapping");
    if counts.get(&identity) != Some(&1) {
        return Err(ProviderError::DefaultBoardMapping(
            "default board identity must map to exactly one activated investigation".into(),
        ));
    }
    Ok(identity)
}

fn default_board(id: String) -> BoardDraft {
    BoardDraft {
        id,
        title: "Delivery".into(),
        status_source: BoardStatusSource::Progress,
        filter_statuses: None,
        filter_kinds: Some(vec!["ticket".into()]),
        columns: [
            ("Unknown", "unknown"),
            ("In progress", "in_progress"),
            ("In review", "in_review"),
            ("Verifying", "verifying"),
            ("Blocked", "blocked"),
            ("Complete", "complete"),
        ]
        .into_iter()
        .map(|(name, status)| BoardColumn {
            name: name.into(),
            statuses: vec![status.into()],
        })
        .collect(),
    }
}
