use crate::{
    activation::activation,
    derived::{DerivedSnapshot, derive_snapshot},
    index::RevisionSource,
    scanning::{ScanResult, scan},
    writing,
};
use casefile_core::{ApplyResult, ChangeRequest, Preview, Revision, parse_strategy_binding};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
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

#[derive(Deserialize, Serialize)]
struct BindingJournal {
    schema_version: u32,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<String>,
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
        recover_binding_transactions(&self.root)?;
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
        recover_binding_transactions(&self.root)?;
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
        let strategy = self.root.join(binding).join("strategy");
        replace_binding_transaction(&strategy, source)
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
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn remove_binding_file(path: &Path) -> Result<(), StoreError> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn transaction_paths(
    strategy: &Path,
    previous: Option<&str>,
) -> (PathBuf, Option<PathBuf>, PathBuf) {
    let target = strategy.join("bindings.toml");
    let history = previous.map(|value| {
        strategy
            .join("binding-history")
            .join(format!("{:x}.toml", Sha256::digest(value.as_bytes())))
    });
    let journal = strategy.join(".binding-transaction.toml");
    (target, history, journal)
}

fn write_journal(path: &Path, journal: &BindingJournal) -> Result<(), StoreError> {
    let source =
        toml::to_string(journal).map_err(|error| StoreError::Invalid(error.to_string()))?;
    atomic_binding_write(path, source.as_bytes(), true)
}

fn recover_binding_transaction(strategy: &Path) -> Result<(), StoreError> {
    let journal_path = strategy.join(".binding-transaction.toml");
    if !journal_path.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(&journal_path)?
        .file_type()
        .is_symlink()
    {
        return Err(StoreError::Invalid(
            "binding journal must not be a symlink".into(),
        ));
    }
    let journal: BindingJournal = toml::from_str(&fs::read_to_string(&journal_path)?)
        .map_err(|error| StoreError::Invalid(format!("invalid binding journal: {error}")))?;
    if journal.schema_version != 1 {
        return Err(StoreError::Invalid(
            "binding journal schema_version must be 1".into(),
        ));
    }
    let (target, history, _) = transaction_paths(strategy, journal.previous.as_deref());
    for path in std::iter::once(&target).chain(history.iter()) {
        if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(StoreError::Invalid(
                "binding recovery target must not be a symlink".into(),
            ));
        }
    }
    if let (Some(history), Some(previous)) = (&history, &journal.previous) {
        match read_file(history)? {
            None => atomic_binding_write(history, previous.as_bytes(), true)?,
            Some(current) if current == previous.as_bytes() => {}
            Some(_) => {
                return Err(StoreError::Invalid(
                    "binding history conflicts with recovery journal".into(),
                ));
            }
        }
    }
    atomic_binding_write(&target, journal.source.as_bytes(), !target.exists())?;
    if read_file(&target)?.as_deref() != Some(journal.source.as_bytes()) {
        return Err(StoreError::Invalid(
            "binding recovery target verification failed".into(),
        ));
    }
    if let (Some(history), Some(previous)) = (&history, &journal.previous) {
        if read_file(history)?.as_deref() != Some(previous.as_bytes()) {
            return Err(StoreError::Invalid(
                "binding recovery history verification failed".into(),
            ));
        }
    }
    remove_binding_file(&journal_path)
}

fn recover_binding_transactions(root: &Path) -> Result<(), StoreError> {
    let (_, active, _) = crate::activation::activation(root)?;
    for project in active.projects.values() {
        for investigation in &project.investigations {
            recover_binding_transaction(&root.join(investigation).join("strategy"))?;
        }
    }
    Ok(())
}

fn replace_binding_transaction(strategy: &Path, source: &str) -> Result<(), StoreError> {
    recover_binding_transaction(strategy)?;
    let target = strategy.join("bindings.toml");
    let previous = read_file(&target)?;
    let previous = previous
        .map(|value| {
            String::from_utf8(value)
                .map_err(|_| StoreError::Invalid("binding must be UTF-8".into()))
        })
        .transpose()?;
    let (_, _, journal_path) = transaction_paths(strategy, previous.as_deref());
    let journal = BindingJournal {
        schema_version: 1,
        source: source.into(),
        previous,
    };
    write_journal(&journal_path, &journal)?;
    // Every durable boundary after this point is recoverable to the complete committed state.
    recover_binding_transaction(strategy)
}

impl RevisionSource for Store {
    fn current_revision(&self) -> Result<Revision, StoreError> {
        Ok(self.scan()?.snapshot.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::{BindingJournal, recover_binding_transaction, transaction_paths, write_journal};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn interrupted_replacement_recovers_to_the_complete_state_at_each_boundary() {
        for boundary in 0..3 {
            let root = TempDir::new().expect("root");
            let strategy = root.path().join("strategy");
            fs::create_dir_all(&strategy).expect("strategy");
            let target = strategy.join("bindings.toml");
            fs::write(&target, "old").expect("target");
            let journal = BindingJournal {
                schema_version: 1,
                source: "new".into(),
                previous: Some("old".into()),
            };
            let (_, history, journal_path) =
                transaction_paths(&strategy, journal.previous.as_deref());
            write_journal(&journal_path, &journal).expect("journal");
            if boundary >= 1 {
                super::atomic_binding_write(history.as_ref().expect("history"), b"old", true)
                    .expect("history");
            }
            if boundary >= 2 {
                super::atomic_binding_write(&target, b"new", false).expect("target");
            }
            recover_binding_transaction(&strategy).expect("recovery");
            assert_eq!(b"new", fs::read(&target).expect("target").as_slice());
            assert_eq!(
                b"old",
                fs::read(history.expect("history"))
                    .expect("history")
                    .as_slice()
            );
            assert!(!journal_path.exists());
        }
    }

    #[test]
    fn failed_journal_creation_leaves_the_pre_state_unchanged() {
        let root = TempDir::new().expect("root");
        let strategy = root.path().join("strategy");
        fs::create_dir_all(strategy.join(".binding-transaction.toml")).expect("journal directory");
        fs::write(strategy.join("bindings.toml"), "old").expect("target");
        assert!(super::replace_binding_transaction(&strategy, "new").is_err());
        assert_eq!(
            b"old",
            fs::read(strategy.join("bindings.toml"))
                .expect("target")
                .as_slice()
        );
        assert!(!strategy.join("binding-history").exists());
    }
}
