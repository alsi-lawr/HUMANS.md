use crate::{
    derived::{DerivedSnapshot, derive_snapshot},
    governance::{
        self, GovernedApplyResult, StrategyTransitionPreview, StrategyTransitionRequest,
        WriterBindingPreview, WriterBindingRequest,
    },
    index::RevisionSource,
    presentation::PresentationSession,
    progress::{self, ProgressApplyResult, ProgressChangeRequest, ProgressPreview},
    scanning::{ScanResult, metadata_inventory, scan},
    writing,
};
use casefile_core::{
    ApplyResult, ChangeBatchApplyResult, ChangeBatchPreview, ChangeRequest, Preview, Revision,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};
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

pub(super) fn require_safe_target_parent(
    root: &Path,
    relative_parent: &Path,
    target_name: &str,
) -> Result<(), StoreError> {
    let mut current = root.to_path_buf();
    match fs::symlink_metadata(&current) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(StoreError::Invalid(format!(
                "{target_name} path must not contain a symlink"
            )));
        }
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err(StoreError::Invalid(
                "configured Store root must be a non-symlink directory".into(),
            ));
        }
        Ok(_) => {}
        Err(error) => return Err(error.into()),
    }
    for component in relative_parent.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(component) => current.push(component),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(StoreError::Invalid(format!(
                    "{target_name} path must remain inside the configured Store root"
                )));
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Invalid(format!(
                    "{target_name} path must not contain a symlink"
                )));
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(StoreError::Invalid(format!(
                    "{target_name} parent must be a directory"
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
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

    /// Creates an isolated catalogue-first presentation session for this Store root.
    pub fn presentation_session(&self) -> PresentationSession {
        PresentationSession::new(self.root.clone())
    }

    /// Returns the fixed, read-only root used by advisory observation adapters.
    pub fn observation_root(&self) -> &Path {
        &self.root
    }

    pub fn preview(&self, request: ChangeRequest) -> Result<Preview, StoreError> {
        writing::preview(&self.root, request)
    }

    pub fn apply(&self, preview: Preview) -> Result<ApplyResult, StoreError> {
        writing::apply(&self.root, preview)
    }

    pub fn preview_batch(
        &self,
        requests: Vec<ChangeRequest>,
    ) -> Result<ChangeBatchPreview, StoreError> {
        writing::preview_batch(&self.root, requests)
    }

    pub fn apply_batch(
        &self,
        preview: ChangeBatchPreview,
    ) -> Result<ChangeBatchApplyResult, StoreError> {
        writing::apply_batch(&self.root, preview)
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
        Ok(metadata_inventory(&self.root)?.revision)
    }
}

#[cfg(all(test, unix))]
mod safety_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn current_revision_uses_metadata_inventory_without_body_reads() {
        let root = TempDir::new().expect("root");
        let path = root.path().join("unreadable.raw");
        fs::write(&path, b"not readable").expect("file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("permissions");
        let store = Store::open(root.path()).expect("store");

        let revision = RevisionSource::current_revision(&store).expect("metadata revision");
        assert!(revision.0.starts_with("fsmeta-tree-v1:"));
        assert!(store.scan().is_err());
    }

    #[test]
    fn safe_target_parent_rejects_the_root_and_in_store_descendant_symlinks() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary root");
        let root = temporary.path().join("Store");
        let external = temporary.path().join("external");
        fs::create_dir_all(&root).expect("Store root");
        fs::create_dir_all(&external).expect("external directory");

        let root_link = temporary.path().join("Store-link");
        symlink(&root, &root_link).expect("root symlink");
        assert!(require_safe_target_parent(&root_link, Path::new(""), "test target").is_err());

        symlink(&external, root.join("linked-parent")).expect("descendant symlink");
        assert!(
            require_safe_target_parent(&root, Path::new("linked-parent/nested"), "test target")
                .is_err()
        );
    }
}
