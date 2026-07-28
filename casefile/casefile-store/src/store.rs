use crate::{
    derived::{DerivedSnapshot, derive_snapshot},
    governance::{
        self, GovernedApplyResult, StrategyTransitionPreview, StrategyTransitionRequest,
        WriterBindingPreview, WriterBindingRequest,
    },
    index::RevisionSource,
    progress::{self, ProgressApplyResult, ProgressChangeRequest, ProgressPreview},
    scanning::{ScanResult, scan},
    writing,
};
use casefile_core::{ApplyResult, ChangeRequest, Preview, Revision};
use std::{collections::BTreeMap, fs, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation is invalid: {0}")]
    Invalid(String),
    #[error("stale target revision")]
    StaleTargetRevision,
}

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err(StoreError::Invalid(
                "planning root must not be a symlink".into(),
            ));
        }
        Ok(Self { root })
    }

    pub fn scan(&self) -> Result<ScanResult, StoreError> {
        scan(&self.root, &BTreeMap::new())
    }

    pub fn derived_snapshot(&self) -> Result<DerivedSnapshot, StoreError> {
        let scan = self.scan()?;
        Ok(derive_snapshot(&scan))
    }

    pub fn derive_snapshot(&self, scan: &ScanResult) -> DerivedSnapshot {
        derive_snapshot(scan)
    }

    pub fn preview(&self, request: ChangeRequest) -> Result<Preview, StoreError> {
        writing::preview(&self.root, request)
    }

    pub fn apply(&self, preview: Preview) -> Result<ApplyResult, StoreError> {
        writing::apply(&self.root, preview)
    }

    pub fn preview_progress(
        &self,
        request: ProgressChangeRequest,
    ) -> Result<ProgressPreview, StoreError> {
        progress::preview(&self.root, request)
    }

    pub fn apply_progress(
        &self,
        preview: ProgressPreview,
    ) -> Result<ProgressApplyResult, StoreError> {
        progress::apply(&self.root, preview)
    }

    pub fn bootstrap_progress(
        &self,
        investigation: &str,
    ) -> Result<ProgressChangeRequest, StoreError> {
        progress::bootstrap(&self.root, investigation)
    }

    pub fn validate_investigation(&self, investigation: &str) -> Result<(), StoreError> {
        progress::validate_investigation(&self.root, investigation)
    }

    pub fn preview_strategy_transition(
        &self,
        request: StrategyTransitionRequest,
    ) -> Result<StrategyTransitionPreview, StoreError> {
        governance::preview_strategy_transition(&self.root, request)
    }

    pub fn apply_strategy_transition(
        &self,
        preview: StrategyTransitionPreview,
    ) -> Result<GovernedApplyResult, StoreError> {
        governance::apply_strategy_transition(&self.root, preview)
    }

    pub fn preview_writer_binding(
        &self,
        request: WriterBindingRequest,
    ) -> Result<WriterBindingPreview, StoreError> {
        governance::preview_writer_binding(&self.root, request)
    }

    pub fn apply_writer_binding(
        &self,
        preview: WriterBindingPreview,
    ) -> Result<GovernedApplyResult, StoreError> {
        governance::apply_writer_binding(&self.root, preview)
    }

    pub fn require_writer_progress(
        &self,
        investigation: &str,
        ticket_id: &str,
    ) -> Result<(), StoreError> {
        governance::require_writer_progress(&self.root, investigation, ticket_id)
    }
}

impl RevisionSource for Store {
    fn current_revision(&self) -> Result<Revision, StoreError> {
        Ok(self.scan()?.snapshot.revision)
    }
}
