use crate::{
    activation::activation,
    derived::{DerivedSnapshot, derive_snapshot},
    index::RevisionSource,
    progress::{self, ProgressApplyResult, ProgressChangeRequest, ProgressPreview},
    scanning::{ScanResult, scan},
    writing,
};
use casefile_core::{ApplyResult, ChangeRequest, Preview, Revision, parse_strategy_binding};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::{Builder, NamedTempFile};
use thiserror::Error;

const BINDING_TEMP_PREFIX: &str = ".bindings.toml.tmp-";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation is invalid: {0}")]
    Invalid(String),
    #[error("stale store revision")]
    StaleStoreRevision,
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
        let (_, active, _) = activation(&self.root)?;
        Ok(derive_snapshot(&scan, &active))
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

    /// Replaces the sole governed writer-binding state file atomically.
    /// The runtime owner must report active implementation or correction work truthfully.
    pub fn replace_strategy_binding(
        &self,
        investigation: &str,
        source: &str,
        implementation_active: bool,
    ) -> Result<(), StoreError> {
        if implementation_active {
            return Err(StoreError::Invalid(
                "cannot replace a writer binding while implementation work is active".into(),
            ));
        }
        if !crate::layout::safe_relative(investigation) {
            return Err(StoreError::Invalid(
                "investigation path must be contained".into(),
            ));
        }
        let binding = investigation.trim_end_matches('/');
        let target_relative = format!("{binding}/strategy/bindings.toml");
        let active = crate::activation::activation(&self.root)?.1;
        if crate::layout::kind_for_path(&target_relative, &active)
            != Some(casefile_core::Kind::StrategyBinding)
        {
            return Err(StoreError::Invalid(
                "binding path is not an activated investigation binding".into(),
            ));
        }
        parse_strategy_binding(&target_relative, source).map_err(|diagnostics| {
            StoreError::Invalid(
                diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        replace_binding_file(&self.root.join(binding).join("strategy"), source)
    }
}

fn metadata_if_present(path: &Path) -> Result<Option<fs::Metadata>, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn ensure_no_symlink(path: &Path) -> Result<(), StoreError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Invalid(
                    "binding path must not contain a symlink".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn replace_binding_file(strategy: &Path, source: &str) -> Result<(), StoreError> {
    replace_binding_file_with(strategy, source, persist_binding_file)
}

fn replace_binding_file_with(
    strategy: &Path,
    source: &str,
    persist: impl FnOnce(NamedTempFile, &Path) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    ensure_no_symlink(strategy)?;
    let strategy_metadata = fs::symlink_metadata(strategy)?;
    if !strategy_metadata.file_type().is_dir() {
        return Err(StoreError::Invalid(
            "strategy path must be a non-symlink directory".into(),
        ));
    }
    let target = strategy.join("bindings.toml");
    if let Some(metadata) = metadata_if_present(&target)?
        && (!metadata.file_type().is_file() || metadata.file_type().is_symlink())
    {
        return Err(StoreError::Invalid(
            "binding target must be a regular non-symlink file".into(),
        ));
    }
    let mut temporary = Builder::new()
        .prefix(BINDING_TEMP_PREFIX)
        .tempfile_in(strategy)?;
    temporary.write_all(source.as_bytes())?;
    temporary.flush()?;
    persist(temporary, &target)
}

fn persist_binding_file(temporary: NamedTempFile, target: &Path) -> Result<(), StoreError> {
    temporary
        .persist(target)
        .map_err(|error| StoreError::Io(error.error))?;
    Ok(())
}

impl RevisionSource for Store {
    fn current_revision(&self) -> Result<Revision, StoreError> {
        Ok(self.scan()?.snapshot.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::{StoreError, replace_binding_file, replace_binding_file_with};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn pre_rename_persist_failure_leaves_the_selected_file_unchanged() {
        let root = TempDir::new().expect("root");
        let strategy = root.path().join("strategy");
        fs::create_dir(&strategy).expect("strategy");
        let target = strategy.join("bindings.toml");
        fs::write(&target, "selected").expect("selected binding");

        let result = replace_binding_file_with(&strategy, "replacement", |_, _| {
            Err(StoreError::Io(std::io::Error::other(
                "injected pre-rename persist failure",
            )))
        });

        assert!(result.is_err());
        assert_eq!("selected", fs::read_to_string(target).expect("unchanged"));
        assert!(
            !fs::read_dir(strategy)
                .expect("strategy entries")
                .any(|entry| {
                    entry
                        .expect("strategy entry")
                        .file_name()
                        .to_string_lossy()
                        .starts_with(super::BINDING_TEMP_PREFIX)
                })
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_target_or_parent_cannot_redirect_the_binding_write() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().expect("root");
        let external = root.path().join("external");
        fs::create_dir(&external).expect("external");
        let external_target = external.join("bindings.toml");
        fs::write(&external_target, "external").expect("external target");

        let strategy = root.path().join("strategy");
        fs::create_dir(&strategy).expect("strategy");
        symlink(&external_target, strategy.join("bindings.toml")).expect("target symlink");
        assert!(replace_binding_file(&strategy, "replacement").is_err());
        assert_eq!(
            "external",
            fs::read_to_string(&external_target).expect("external unchanged")
        );

        let linked_strategy = root.path().join("linked-strategy");
        symlink(&external, &linked_strategy).expect("parent symlink");
        assert!(replace_binding_file(&linked_strategy, "replacement").is_err());
        assert_eq!(
            "external",
            fs::read_to_string(external_target).expect("external unchanged")
        );
    }
}
