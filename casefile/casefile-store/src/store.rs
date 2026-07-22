use crate::{
    activation::activation,
    derived::{DerivedSnapshot, derive_snapshot},
    index::RevisionSource,
    scanning::{ScanResult, scan},
    writing,
};
use casefile_core::{ApplyResult, ChangeRequest, Preview, Revision, parse_strategy_binding};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

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

    /// Replaces a governed writer binding and archives the previous exact source atomically.
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
        let target = self.root.join(&target_relative);
        if target.exists() && fs::symlink_metadata(&target)?.file_type().is_symlink() {
            return Err(StoreError::Invalid(
                "binding target must not be a symlink".into(),
            ));
        }
        let previous = if target.exists() {
            Some(fs::read(&target)?)
        } else {
            None
        };
        let history = previous.as_ref().map(|bytes| {
            let digest = Sha256::digest(bytes);
            self.root.join(format!(
                "{binding}/strategy/binding-history/{digest:x}.toml"
            ))
        });
        let history_before = history.as_ref().map(|path| fs::read(path).ok());
        let target_before = previous.clone();
        let write =
            |path: &std::path::Path, bytes: &[u8], create: bool| -> Result<(), StoreError> {
                let parent = path
                    .parent()
                    .ok_or_else(|| StoreError::Invalid("binding has no parent".into()))?;
                fs::create_dir_all(parent)?;
                let mut temporary = NamedTempFile::new_in(parent)?;
                temporary.write_all(bytes)?;
                temporary.flush()?;
                if create {
                    temporary
                        .persist_noclobber(path)
                        .map_err(|error| StoreError::Io(error.error))?;
                } else {
                    temporary
                        .persist(path)
                        .map_err(|error| StoreError::Io(error.error))?;
                }
                Ok(())
            };
        let result = (|| {
            if let (Some(history), Some(previous)) = (&history, &previous) {
                if !history.exists() {
                    write(history, previous, true)?;
                }
            }
            write(&target, source.as_bytes(), previous.is_none())?;
            if fs::read(&target)? != source.as_bytes() {
                return Err(StoreError::Invalid(
                    "binding post-write verification failed".into(),
                ));
            }
            Ok(())
        })();
        if result.is_err() {
            match target_before {
                Some(bytes) => {
                    let _ = write(&target, &bytes, false);
                }
                None => {
                    let _ = fs::remove_file(&target);
                }
            }
            if let Some(history) = history {
                match history_before.flatten() {
                    Some(bytes) => {
                        let _ = write(&history, &bytes, false);
                    }
                    None => {
                        let _ = fs::remove_file(history);
                    }
                }
            }
        }
        result
    }
}

impl RevisionSource for Store {
    fn current_revision(&self) -> Result<Revision, StoreError> {
        Ok(self.scan()?.snapshot.revision)
    }
}
