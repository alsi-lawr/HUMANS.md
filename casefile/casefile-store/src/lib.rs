//! Filesystem and Git boundary for the compact Casefile v1 contract.
#![allow(clippy::collapsible_if)] // Nested validation keeps individual rules readable.

mod activation;
mod derived;
mod governance;
mod index;
mod layout;
mod mutation;
mod mutation_dependencies;
#[cfg(test)]
mod mutation_hooks;
mod mutation_locks;
mod mutation_metadata;
#[cfg(test)]
mod mutation_tests;
mod presentation;
mod progress;
mod provider;
mod revision;
mod scanning;
mod store;
mod validation;
mod writing;

pub use activation::ActivationState;
pub use derived::{
    DerivedBoard, DerivedBoardColumn, DerivedCard, DerivedProgressNote, DerivedProgressTransition,
    DerivedRecord, DerivedRelationship, DerivedSnapshot, DerivedStrategy, DerivedStrategyBinding,
    DerivedTicketProgress, EffectiveWriterBinding, RecordScope, RelationshipKind, ScopedIdentity,
    StrategyBindingState, WriterBindingSource,
};
pub use governance::{
    GovernedApplyResult, GovernedChange, GovernedOperationKind, StrategyTransitionPreview,
    StrategyTransitionRequest, WriterBindingPreview, WriterBindingRequest,
};
pub use index::{DerivedIndex, Indexed, RevisionSource};
pub use layout::normalize_planning_relative;
pub use presentation::{
    FactAvailability, PRESENTATION_BATCH_LIMIT, PRESENTATION_CHANNEL_CAPACITY, PresentationCache,
    PresentationCatalogue, PresentationContentEvent, PresentationContentHandle,
    PresentationContentRequest, PresentationContentSelector, PresentationContentStream,
    PresentationCoverage, PresentationCoverageState, PresentationEntry, PresentationEvent,
    PresentationFact, PresentationFileKind, PresentationFileMetadata, PresentationInvestigation,
    PresentationLoadRequest, PresentationProgress, PresentationProject, PresentationScope,
    PresentationSession, PresentationStream, PresentationSummary, PresentationTarget,
    presentation_revision,
};
pub use progress::{ProgressApplyResult, ProgressChangeRequest, ProgressPreview};
pub use provider::{
    CacheState, DefaultBoardApplyResult, DefaultBoardPreview, InvestigationScope,
    InvestigationScopedIdentity, NoCache, PROVIDER_PROTOCOL_VERSION, ProgressOperation, Provider,
    ProviderApplyOutcome, ProviderApprovalPolicy, ProviderBatchPreview, ProviderCache,
    ProviderCapabilities, ProviderCatalogue, ProviderDiagnosticCount, ProviderDiagnosticCoverage,
    ProviderError, ProviderIndexDiagnosticCoverage, ProviderIndexDiagnosticCoverageKind,
    ProviderInvestigation, ProviderMutationState, ProviderOperation, ProviderPreview,
    ProviderProgressPreview, ProviderProject, ProviderQuery, ProviderQueryResult,
    ProviderRecordApplyResult, ProviderRecordBatchApplyResult, ProviderRecordDetail,
    ProviderRecordDiagnosticCoverage, ProviderRecordIndexEntry, ProviderRecordProgressSummary,
    ProviderSnapshot, ProviderStrategyTransitionPreview, ProviderWriterBindingPreview,
    StrategyTransitionProjection,
};
pub use scanning::{ScanResult, is_store_path_excluded};
pub use store::{Store, StoreError};
