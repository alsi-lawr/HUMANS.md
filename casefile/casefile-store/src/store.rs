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

type BindingWriter<'a> = dyn FnMut(&std::path::Path, &[u8], bool) -> Result<(), StoreError> + 'a;

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
        let previous = read_file(&target)?;
        let history = previous.as_ref().map(|bytes| {
            let digest = Sha256::digest(bytes);
            self.root.join(format!(
                "{binding}/strategy/binding-history/{digest:x}.toml"
            ))
        });
        replace_binding_files(
            &target,
            history.as_deref(),
            source.as_bytes(),
            &mut atomic_binding_write,
        )
    }
}

fn read_file(path: &std::path::Path) -> Result<Option<Vec<u8>>, StoreError> {
    if path.exists() {
        Ok(Some(fs::read(path)?))
    } else {
        Ok(None)
    }
}

fn atomic_binding_write(
    path: &std::path::Path,
    bytes: &[u8],
    create: bool,
) -> Result<(), StoreError> {
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
}

fn restore_binding_file(path: &std::path::Path, before: Option<&[u8]>) -> Result<(), StoreError> {
    match before {
        Some(bytes) => atomic_binding_write(path, bytes, false)?,
        None if path.exists() => fs::remove_file(path)?,
        None => {}
    }
    if read_file(path)?.as_deref() == before {
        Ok(())
    } else {
        Err(StoreError::Invalid(
            "binding rollback verification failed".into(),
        ))
    }
}

fn replace_binding_files(
    target: &std::path::Path,
    history: Option<&std::path::Path>,
    source: &[u8],
    writer: &mut BindingWriter<'_>,
) -> Result<(), StoreError> {
    let target_before = read_file(target)?;
    let history_before = history.map(read_file).transpose()?;
    let attempt = (|| {
        if let (Some(history), Some(previous)) = (history, target_before.as_deref()) {
            if history_before.as_ref().is_none_or(Option::is_none) {
                writer(history, previous, true)?;
            }
        }
        writer(target, source, target_before.is_none())?;
        if read_file(target)?.as_deref() != Some(source) {
            return Err(StoreError::Invalid(
                "binding post-write verification failed".into(),
            ));
        }
        Ok(())
    })();
    if let Err(error) = attempt {
        let target_recovery = restore_binding_file(target, target_before.as_deref());
        let history_recovery = history
            .map(|path| {
                restore_binding_file(
                    path,
                    history_before.as_ref().and_then(|value| value.as_deref()),
                )
            })
            .transpose();
        target_recovery?;
        history_recovery?;
        return Err(error);
    }
    Ok(())
}

impl RevisionSource for Store {
    fn current_revision(&self) -> Result<Revision, StoreError> {
        Ok(self.scan()?.snapshot.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::{StoreError, replace_binding_files};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn replacement_rolls_back_exactly_after_history_write_failure() {
        let root = TempDir::new().expect("root");
        let target = root.path().join("bindings.toml");
        let history = root.path().join("binding-history/previous.toml");
        fs::write(&target, b"old").expect("target");
        let mut writes = 0;
        let error = replace_binding_files(
            &target,
            Some(&history),
            b"new",
            &mut |path, bytes, create| {
                writes += 1;
                if writes == 2 {
                    return Err(StoreError::Invalid("injected target failure".into()));
                }
                super::atomic_binding_write(path, bytes, create)
            },
        )
        .expect_err("injected failure");
        assert!(error.to_string().contains("injected"));
        assert_eq!(
            b"old",
            fs::read(&target).expect("target restored").as_slice()
        );
        assert!(
            !history.exists(),
            "archive must be removed during verified recovery"
        );
    }
}
