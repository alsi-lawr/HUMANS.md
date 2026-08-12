use crate::layout::checked_path;
use crate::{
    ActivationState, DerivedBoard, DerivedIndex, DerivedSnapshot, DerivedTicketProgress,
    GovernedApplyResult, Indexed, ProgressApplyResult, ProgressChangeRequest, ProgressPreview,
    RevisionSource, ScanResult, Store, StoreError, StrategyTransitionPreview,
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

pub const PROVIDER_PROTOCOL_VERSION: u32 = 2;
const PREVIEW_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperation {
    Snapshot,
    RecordIndex,
    RecordDetail,
    Boards,
    StrategyTransitions,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApprovalPolicy {
    RecordDeletesOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub protocol_version: u32,
    pub planning_format_versions: Vec<u32>,
    pub mutation: ProviderMutationState,
    pub operations: Vec<ProviderOperation>,
    pub approval_policy: ProviderApprovalPolicy,
    pub writes_require_external_approval: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyTransitionProjection {
    pub path: String,
    pub scope: InvestigationScope,
    pub record: StrategyTransitionRecord,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationScope {
    pub project: String,
    pub investigation: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationScopedIdentity {
    pub scope: InvestigationScope,
    pub identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CacheState {
    NotConfigured,
    Missing,
    Stale {
        indexed_revision: Revision,
        current_revision: Revision,
    },
    Current {
        source_revision: Revision,
    },
    Degraded {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderDiagnosticCoverage {
    pub catalogue: ProviderDiagnosticCount,
    pub records: ProviderRecordDiagnosticCoverage,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderDiagnosticCount {
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRecordDiagnosticCoverage {
    NotLoaded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderCatalogue {
    pub projects: Vec<ProviderProject>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderProject {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_root: Option<String>,
    pub governed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub investigations: Vec<ProviderInvestigation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderInvestigation {
    pub identity: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub capabilities: ProviderCapabilities,
    pub activation: ActivationState,
    pub revision: Revision,
    pub diagnostic_coverage: ProviderDiagnosticCoverage,
    pub catalogue: ProviderCatalogue,
    pub cache: CacheState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecordProgressSummary {
    pub status: casefile_core::ProgressStatus,
    pub note_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecordIndexEntry {
    pub path: String,
    pub classification: casefile_core::Classification,
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProviderRecordProgressSummary>,
    pub diagnostic_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecordDetail {
    pub path: String,
    pub classification: casefile_core::Classification,
    pub kind: Kind,
    pub identity: InvestigationScopedIdentity,
    pub draft: RecordDraft,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<DerivedTicketProgress>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum ProviderQuery {
    RecordIndex {
        scope: InvestigationScope,
    },
    RecordDetail {
        identity: InvestigationScopedIdentity,
    },
    Boards {
        scope: InvestigationScope,
    },
    StrategyTransitions {
        scope: InvestigationScope,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ProviderQueryResult {
    RecordIndex {
        revision: Revision,
        scope: InvestigationScope,
        diagnostic_coverage: ProviderIndexDiagnosticCoverage,
        records: Vec<ProviderRecordIndexEntry>,
    },
    RecordDetail {
        revision: Revision,
        identity: InvestigationScopedIdentity,
        record: Option<Box<ProviderRecordDetail>>,
    },
    Boards {
        revision: Revision,
        scope: InvestigationScope,
        boards: Vec<DerivedBoard>,
    },
    StrategyTransitions {
        revision: Revision,
        scope: InvestigationScope,
        transitions: Vec<StrategyTransitionProjection>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderIndexDiagnosticCoverage {
    pub scope: InvestigationScope,
    pub kind: ProviderIndexDiagnosticCoverageKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderIndexDiagnosticCoverageKind {
    LocalAndInvestigation,
}

pub trait ProviderCache {
    fn observe(&self, revision: &Revision) -> CacheState;
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
    fn observe(&self, revision: &Revision) -> CacheState {
        match self.state(revision) {
            Ok(Indexed::Current {
                source_revision, ..
            }) => CacheState::Current { source_revision },
            Ok(Indexed::Missing) => CacheState::Missing,
            Ok(Indexed::Stale {
                indexed_revision,
                current_revision,
            }) => CacheState::Stale {
                indexed_revision,
                current_revision,
            },
            Err(error) => CacheState::Degraded {
                message: error.to_string(),
            },
        }
    }
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
    fn observe(&self, _: &Revision) -> CacheState {
        CacheState::NotConfigured
    }
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
    pub approval_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderBatchPreview {
    pub preview_id: String,
    pub rendered_bytes: Vec<Option<Vec<u8>>>,
    #[serde(flatten)]
    pub canonical: ChangeBatchPreview,
    pub no_op: bool,
    pub approval_required: bool,
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
    pub approval_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderStrategyTransitionPreview {
    pub preview_id: String,
    pub canonical: StrategyTransitionPreview,
    pub approval_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderWriterBindingPreview {
    pub preview_id: String,
    pub canonical: WriterBindingPreview,
    pub approval_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DefaultBoardPreview {
    pub preview_id: String,
    pub investigation: String,
    pub canonical: Preview,
    pub rendered_bytes: Vec<u8>,
    pub no_op: bool,
    pub approval_required: bool,
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
    #[error("record identity is ambiguous across paths: {paths:?}")]
    AmbiguousRecordIdentity { paths: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoredPreview {
    Record(Preview, Option<Vec<u8>>, bool, bool),
    RecordBatch(ChangeBatchPreview, Vec<Option<Vec<u8>>>, bool, bool),
    Progress(ProgressOperation, ProgressPreview, bool),
    Board(String, Preview, Vec<u8>, bool, bool),
    StrategyTransition(StrategyTransitionPreview, bool),
    WriterBinding(WriterBindingPreview, bool),
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
        let baseline = crate::scanning::catalogue_baseline(self.store.observation_root())?;
        let mut names = baseline
            .projects
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        names.extend(baseline.active.projects.keys().cloned());
        let projects = names
            .into_iter()
            .map(|name| {
                let governed = baseline.active.projects.get(&name);
                ProviderProject {
                    source_root: baseline.projects.get(&name).cloned(),
                    prefix: governed.map(|project| project.prefix.clone()),
                    investigations: governed
                        .into_iter()
                        .flat_map(|project| &project.investigations)
                        .filter_map(|path| {
                            crate::activation::investigation_identity(&name, path).map(|identity| {
                                ProviderInvestigation {
                                    identity: identity.into(),
                                    path: path.clone(),
                                }
                            })
                        })
                        .collect(),
                    governed: governed.is_some(),
                    name,
                }
            })
            .collect();
        let diagnostic_count = baseline.diagnostics.len();
        let cache = self.cache.observe(&baseline.revision);
        Ok(ProviderSnapshot {
            capabilities: capabilities(baseline.activation),
            activation: baseline.activation,
            revision: baseline.revision,
            diagnostic_coverage: ProviderDiagnosticCoverage {
                catalogue: ProviderDiagnosticCount {
                    count: diagnostic_count,
                },
                records: ProviderRecordDiagnosticCoverage::NotLoaded,
            },
            catalogue: ProviderCatalogue { projects },
            cache,
        })
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

    pub fn query(&self, query: ProviderQuery) -> Result<ProviderQueryResult, ProviderError> {
        let query = canonical_query(query)?;
        Ok(match query {
            ProviderQuery::RecordIndex { scope } => self.record_index(scope)?,
            ProviderQuery::RecordDetail { identity } => self.record_detail(identity)?,
            ProviderQuery::Boards { scope } => self.boards(scope)?,
            ProviderQuery::StrategyTransitions { scope } => self.strategy_transitions(scope)?,
        })
    }

    fn record_index(
        &self,
        scope: InvestigationScope,
    ) -> Result<ProviderQueryResult, ProviderError> {
        let selected = crate::scanning::scoped_scan(
            self.store.observation_root(),
            &scope.project,
            &scope.investigation,
            crate::scanning::ScopedRead::RecordIndex,
        )?;
        let progress = crate::derived::scoped_progress(
            &selected.entries,
            &selected.diagnostics,
            &selected.path,
        );
        let diagnostics = diagnostic_counts(&selected.diagnostics);
        let records = selected
            .entries
            .iter()
            .filter(|entry| matches!(entry.kind, Some(Kind::Ticket | Kind::Epic)))
            .map(|entry| {
                let summary = match &entry.summary {
                    Some(RecordSummary::WorkItem {
                        id,
                        title,
                        status,
                        rank,
                    }) => Some((id.clone(), title.clone(), status.clone(), *rank)),
                    _ => None,
                };
                ProviderRecordIndexEntry {
                    path: entry.path.clone(),
                    classification: entry.classification,
                    kind: entry.kind,
                    identity: summary.as_ref().map(|summary| summary.0.clone()),
                    title: summary.as_ref().map(|summary| summary.1.clone()),
                    status: summary.as_ref().map(|summary| summary.2.clone()),
                    rank: summary.as_ref().and_then(|summary| summary.3),
                    progress: summary
                        .as_ref()
                        .filter(|summary| {
                            summary.2 == "accepted" && entry.kind == Some(Kind::Ticket)
                        })
                        .map(|summary| {
                            progress.get(&summary.0).map_or(
                                ProviderRecordProgressSummary {
                                    status: casefile_core::ProgressStatus::Unknown,
                                    note_count: 0,
                                },
                                |progress| ProviderRecordProgressSummary {
                                    status: progress.status,
                                    note_count: progress.notes.len(),
                                },
                            )
                        }),
                    diagnostic_count: diagnostics.get(&entry.path).copied().unwrap_or_default(),
                }
            })
            .collect();
        Ok(ProviderQueryResult::RecordIndex {
            revision: selected.revision,
            diagnostic_coverage: ProviderIndexDiagnosticCoverage {
                scope: scope.clone(),
                kind: ProviderIndexDiagnosticCoverageKind::LocalAndInvestigation,
            },
            scope,
            records,
        })
    }

    fn record_detail(
        &self,
        identity: InvestigationScopedIdentity,
    ) -> Result<ProviderQueryResult, ProviderError> {
        let selected = crate::scanning::scoped_detail_scan(
            self.store.observation_root(),
            &identity.scope.project,
            &identity.scope.investigation,
            &identity.identity,
        )?;
        let matches = selected
            .entries
            .iter()
            .filter(|entry| {
                matches!(entry.kind, Some(Kind::Ticket | Kind::Epic))
                    && entry.identity.as_deref() == Some(identity.identity.as_str())
            })
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err(ProviderError::AmbiguousRecordIdentity {
                paths: matches.iter().map(|entry| entry.path.clone()).collect(),
            });
        }
        let record = matches
            .first()
            .map(|entry| {
                let text = std::str::from_utf8(&entry.original_bytes)
                    .map_err(|_| StoreError::Invalid("record detail must be UTF-8".into()))?;
                let kind = entry.kind.expect("filtered work item");
                let draft = casefile_core::parse_draft(&entry.path, kind, text)
                    .map_err(|diagnostics| StoreError::Invalid(diagnostics[0].message.clone()))?;
                let progress = crate::derived::scoped_progress(
                    &selected.entries,
                    &selected.diagnostics,
                    &selected.path,
                )
                .remove(&identity.identity);
                let diagnostics = selected
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.path == entry.path)
                    .cloned()
                    .collect();
                Ok::<_, StoreError>(Box::new(ProviderRecordDetail {
                    path: entry.path.clone(),
                    classification: entry.classification,
                    kind,
                    identity: identity.clone(),
                    draft,
                    progress,
                    diagnostics,
                }))
            })
            .transpose()?;
        Ok(ProviderQueryResult::RecordDetail {
            revision: selected.revision,
            identity,
            record,
        })
    }

    fn boards(&self, scope: InvestigationScope) -> Result<ProviderQueryResult, ProviderError> {
        let selected = crate::scanning::scoped_scan(
            self.store.observation_root(),
            &scope.project,
            &scope.investigation,
            crate::scanning::ScopedRead::Boards,
        )?;
        let boards = crate::derived::scoped_boards(
            &selected.entries,
            &selected.diagnostics,
            &selected.project,
            &selected.investigation,
            &selected.path,
        );
        Ok(ProviderQueryResult::Boards {
            revision: selected.revision,
            scope,
            boards,
        })
    }

    fn strategy_transitions(
        &self,
        scope: InvestigationScope,
    ) -> Result<ProviderQueryResult, ProviderError> {
        let selected = crate::scanning::scoped_scan(
            self.store.observation_root(),
            &scope.project,
            &scope.investigation,
            crate::scanning::ScopedRead::StrategyTransitions,
        )?;
        let transitions = selected
            .entries
            .iter()
            .filter_map(|entry| match &entry.summary {
                Some(RecordSummary::StrategyTransition { record }) => {
                    Some(StrategyTransitionProjection {
                        path: entry.path.clone(),
                        scope: scope.clone(),
                        record: record.as_ref().clone(),
                    })
                }
                _ => None,
            })
            .collect();
        Ok(ProviderQueryResult::StrategyTransitions {
            revision: selected.revision,
            scope,
            transitions,
        })
    }

    pub fn refresh_full_cache(&self) -> Result<CacheState, ProviderError> {
        let scan = self.store.scan()?;
        let derived = self.store.derive_snapshot(&scan);
        Ok(self.refresh_cache(&derived))
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
        let approval_required = record_approval_required(&canonical.request);
        let preview_id = self.remember(StoredPreview::Record(
            canonical.clone(),
            rendered_bytes.clone(),
            no_op,
            approval_required,
        ));
        Ok(ProviderPreview {
            preview_id,
            rendered_bytes,
            canonical,
            no_op,
            approval_required,
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
                preview.approval_required,
            ),
        )?;
        let result = if preview.no_op {
            let current = self.store.scan()?;
            let path = preview.canonical.request.path();
            let revision = entry_revision(&current, path);
            if revision.as_ref() != preview.canonical.expected_target_revision.as_ref() {
                return Err(StoreError::StaleTargetRevision.into());
            }
            ApplyResult {
                path: path.into(),
                resulting_target_revision: revision,
                resulting_store_revision: current.snapshot.revision,
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
        let approval_required = canonical.requests.iter().any(record_approval_required);
        let preview_id = self.remember(StoredPreview::RecordBatch(
            canonical.clone(),
            rendered_bytes.clone(),
            no_op,
            approval_required,
        ));
        Ok(ProviderBatchPreview {
            preview_id,
            rendered_bytes,
            canonical,
            no_op,
            approval_required,
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
                preview.approval_required,
            ),
        )?;
        let result = if preview.no_op {
            let current = self.store.scan()?;
            for (path, expected) in &preview.canonical.expected_target_revisions {
                if entry_revision(&current, path).as_ref() != expected.as_ref() {
                    return Err(StoreError::StaleTargetRevision.into());
                }
            }
            ChangeBatchApplyResult {
                paths: preview
                    .canonical
                    .requests
                    .iter()
                    .map(|request| request.path().to_owned())
                    .collect(),
                resulting_target_revisions: preview.canonical.expected_target_revisions,
                resulting_store_revision: current.snapshot.revision,
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
        let operation = canonical_progress_operation(operation)?;
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
            false,
        ));
        Ok(ProviderProgressPreview {
            preview_id,
            operation,
            canonical,
            approval_required: false,
        })
    }

    pub fn apply_progress(
        &self,
        preview: ProviderProgressPreview,
    ) -> Result<ProviderApplyOutcome<ProgressApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::Progress(
                preview.operation,
                preview.canonical.clone(),
                preview.approval_required,
            ),
        )?;
        let result = self.store.apply_progress(preview.canonical)?;
        self.outcome(result)
    }

    pub fn preview_default_delivery_board(
        &self,
        investigation: impl Into<String>,
    ) -> Result<DefaultBoardPreview, ProviderError> {
        let investigation = checked_path(&investigation.into())?;
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
            false,
        ));
        Ok(DefaultBoardPreview {
            preview_id,
            investigation,
            canonical,
            rendered_bytes,
            no_op,
            approval_required: false,
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
                preview.approval_required,
            ),
        )?;
        if !preview.canonical.diagnostics.is_empty() {
            return Err(StoreError::Invalid(
                "default delivery-board preview contains diagnostics".into(),
            )
            .into());
        }
        let result = if preview.no_op {
            let current = self.store.scan()?;
            let path = preview.canonical.request.path();
            let revision = entry_revision(&current, path);
            if revision.as_ref() != preview.canonical.expected_target_revision.as_ref() {
                return Err(StoreError::StaleTargetRevision.into());
            }
            ApplyResult {
                path: path.into(),
                resulting_target_revision: revision,
                resulting_store_revision: current.snapshot.revision,
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
        let preview_id = self.remember(StoredPreview::StrategyTransition(canonical.clone(), false));
        Ok(ProviderStrategyTransitionPreview {
            preview_id,
            canonical,
            approval_required: false,
        })
    }

    pub fn apply_strategy_transition(
        &self,
        preview: ProviderStrategyTransitionPreview,
    ) -> Result<ProviderApplyOutcome<GovernedApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::StrategyTransition(
                preview.canonical.clone(),
                preview.approval_required,
            ),
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
        let preview_id = self.remember(StoredPreview::WriterBinding(canonical.clone(), false));
        Ok(ProviderWriterBindingPreview {
            preview_id,
            canonical,
            approval_required: false,
        })
    }

    pub fn apply_writer_binding(
        &self,
        preview: ProviderWriterBindingPreview,
    ) -> Result<ProviderApplyOutcome<GovernedApplyResult>, ProviderError> {
        self.require_mutation()?;
        self.verify(
            &preview.preview_id,
            &StoredPreview::WriterBinding(preview.canonical.clone(), preview.approval_required),
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

fn canonical_query(query: ProviderQuery) -> Result<ProviderQuery, ProviderError> {
    Ok(match query {
        ProviderQuery::RecordIndex { scope } => ProviderQuery::RecordIndex {
            scope: canonical_scope(scope)?,
        },
        ProviderQuery::RecordDetail { identity } => ProviderQuery::RecordDetail {
            identity: InvestigationScopedIdentity {
                scope: canonical_scope(identity.scope)?,
                identity: checked_identity(&identity.identity)?,
            },
        },
        ProviderQuery::Boards { scope } => ProviderQuery::Boards {
            scope: canonical_scope(scope)?,
        },
        ProviderQuery::StrategyTransitions { scope } => ProviderQuery::StrategyTransitions {
            scope: canonical_scope(scope)?,
        },
    })
}

fn canonical_scope(mut scope: InvestigationScope) -> Result<InvestigationScope, ProviderError> {
    scope.project = checked_path(&scope.project)?;
    scope.investigation = checked_path(&scope.investigation)?;
    Ok(scope)
}

fn checked_identity(identity: &str) -> Result<String, ProviderError> {
    if identity.is_empty() || identity.contains(['/', '\\']) {
        return Err(
            StoreError::Invalid("record identity must be a non-empty identifier".into()).into(),
        );
    }
    Ok(identity.into())
}

fn canonical_progress_operation(
    operation: ProgressOperation,
) -> Result<ProgressOperation, ProviderError> {
    Ok(match operation {
        ProgressOperation::Bootstrap { investigation } => ProgressOperation::Bootstrap {
            investigation: checked_path(&investigation)?,
        },
        ProgressOperation::Append {
            investigation,
            entries,
        } => ProgressOperation::Append {
            investigation: checked_path(&investigation)?,
            entries,
        },
    })
}

fn capabilities(activation: ActivationState) -> ProviderCapabilities {
    let reads = vec![
        ProviderOperation::Snapshot,
        ProviderOperation::RecordIndex,
        ProviderOperation::RecordDetail,
        ProviderOperation::Boards,
        ProviderOperation::StrategyTransitions,
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
        approval_policy: ProviderApprovalPolicy::RecordDeletesOnly,
        writes_require_external_approval: true,
    }
}

fn record_approval_required(request: &ChangeRequest) -> bool {
    matches!(request, ChangeRequest::Delete { .. })
}

fn diagnostic_counts(diagnostics: &[Diagnostic]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for diagnostic in diagnostics {
        *counts.entry(diagnostic.path.clone()).or_default() += 1;
    }
    counts
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
            ("TODO", "unknown"),
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

fn entry_revision(scan: &ScanResult, path: &str) -> Option<Revision> {
    scan.snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.content_revision.clone())
}

#[cfg(test)]
mod hierarchy_tests {
    use super::*;
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tempfile::TempDir;

    struct CountingCache {
        observes: Arc<AtomicUsize>,
        refreshes: Arc<AtomicUsize>,
    }
    impl ProviderCache for CountingCache {
        fn observe(&self, _: &Revision) -> CacheState {
            self.observes.fetch_add(1, Ordering::SeqCst);
            CacheState::Missing
        }
        fn refresh(&self, _: &DerivedSnapshot, _: &dyn RevisionSource) -> Result<(), String> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn root() -> TempDir {
        let root = TempDir::new().expect("root");
        fs::create_dir_all(
            root.path()
                .join("projects/demo/investigations/sample/tickets/accepted"),
        )
        .expect("directories");
        fs::write(
            root.path().join("casefile.toml"),
            "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/sample']\n",
        )
        .expect("activation");
        fs::write(
            root.path().join("projects.toml"),
            "schema_version = 1\n[projects]\ndemo = '/source/demo'\n",
        )
        .expect("map");
        fs::write(
            root.path().join("projects/demo/investigations/sample/tickets/accepted/HMD-011.md"),
            include_bytes!("../tests/fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md"),
        )
        .expect("ticket");
        root
    }

    #[test]
    fn snapshot_and_index_observe_cache_without_refreshing_it() {
        let root = root();
        let observes = Arc::new(AtomicUsize::new(0));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let provider = Provider::new(
            Store::open(root.path()).expect("store"),
            CountingCache {
                observes: observes.clone(),
                refreshes: refreshes.clone(),
            },
        );
        provider.snapshot().expect("snapshot");
        provider
            .query(ProviderQuery::RecordIndex {
                scope: InvestigationScope {
                    project: "demo".into(),
                    investigation: "sample".into(),
                },
            })
            .expect("index");
        assert_eq!(observes.load(Ordering::SeqCst), 1);
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
        provider.refresh_full_cache().expect("explicit refresh");
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    }
}
