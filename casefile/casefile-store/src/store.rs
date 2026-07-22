use crate::{
    activation::{ActivationState, activation},
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
use tempfile::Builder;
use thiserror::Error;

const BINDING_JOURNAL: &str = ".binding-transaction.toml";
const BINDING_TEMP_PREFIX: &str = ".binding-transaction.tmp-";

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
    checksum: String,
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
        recover_binding_transactions(&self.root)?;
        writing::preview(&self.root, request)
    }

    pub fn apply(&self, preview: Preview) -> Result<ApplyResult, StoreError> {
        recover_binding_transactions(&self.root)?;
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
        let strategy = self.root.join(binding).join("strategy");
        replace_binding_transaction(&strategy, source)
    }
}

fn metadata_if_present(path: &Path) -> Result<Option<fs::Metadata>, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn read_regular_file(path: &Path, description: &str) -> Result<Option<Vec<u8>>, StoreError> {
    match metadata_if_present(path)? {
        None => Ok(None),
        Some(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(Some(fs::read(path)?))
        }
        Some(_) => Err(StoreError::Invalid(format!(
            "{description} must be a regular non-symlink file"
        ))),
    }
}

fn sync_directory(path: &Path) -> Result<(), StoreError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn create_directory_all_durable(path: &Path) -> Result<(), StoreError> {
    match metadata_if_present(path)? {
        Some(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            return Ok(());
        }
        Some(_) => {
            return Err(StoreError::Invalid(
                "binding transaction directory must be a non-symlink directory".into(),
            ));
        }
        None => {}
    }
    let parent = path
        .parent()
        .ok_or_else(|| StoreError::Invalid("binding directory has no parent".into()))?;
    create_directory_all_durable(parent)?;
    fs::create_dir(path)?;
    // The child sync persists its inode; the parent sync persists the new namespace entry.
    sync_directory(path)?;
    sync_directory(parent)
}

fn atomic_binding_write(
    path: &std::path::Path,
    bytes: &[u8],
    create: bool,
) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| StoreError::Invalid("binding has no parent".into()))?;
    create_directory_all_durable(parent)?;
    let mut temporary = Builder::new()
        .prefix(BINDING_TEMP_PREFIX)
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    if create {
        temporary
            .persist_noclobber(path)
            .map_err(|error| StoreError::Io(error.error))?;
    } else {
        temporary
            .persist(path)
            .map_err(|error| StoreError::Io(error.error))?;
    }
    sync_directory(parent)
}

fn remove_binding_file(path: &Path) -> Result<(), StoreError> {
    read_regular_file(path, "binding transaction file")?.ok_or_else(|| {
        StoreError::Invalid("binding transaction file disappeared before removal".into())
    })?;
    fs::remove_file(path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
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
    let journal = strategy.join(BINDING_JOURNAL);
    (target, history, journal)
}

fn journal_checksum(source: &str, previous: Option<&str>) -> String {
    let mut input = source.as_bytes().to_vec();
    input.push(0);
    if let Some(previous) = previous {
        input.extend_from_slice(previous.as_bytes());
    }
    format!("{:x}", Sha256::digest(input))
}

fn ensure_no_symlink(path: &Path) -> Result<(), StoreError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Invalid(
                    "binding transaction path must not be a symlink".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn validate_transaction_paths(strategy: &Path, history: Option<&Path>) -> Result<(), StoreError> {
    ensure_no_symlink(strategy)?;
    ensure_no_symlink(&strategy.join(BINDING_JOURNAL))?;
    ensure_no_symlink(&strategy.join("bindings.toml"))?;
    ensure_no_symlink(&strategy.join("binding-history"))?;
    if let Some(history) = history {
        ensure_no_symlink(history)?;
    }
    Ok(())
}

fn cleanup_owned_temps(directory: &Path) -> Result<(), StoreError> {
    let Some(metadata) = metadata_if_present(directory)? else {
        return Ok(());
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(StoreError::Invalid(
            "binding temporary-file directory must be a non-symlink directory".into(),
        ));
    }
    let mut removed = false;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name
            .to_str()
            .is_some_and(|name| name.starts_with(BINDING_TEMP_PREFIX))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
            fs::remove_file(entry.path())?;
            removed = true;
        }
    }
    if removed {
        sync_directory(directory)?;
    }
    Ok(())
}

fn cleanup_transaction_temps(strategy: &Path) -> Result<(), StoreError> {
    cleanup_owned_temps(strategy)?;
    cleanup_owned_temps(&strategy.join("binding-history"))
}

fn write_journal(path: &Path, journal: &BindingJournal) -> Result<(), StoreError> {
    ensure_no_symlink(path)?;
    let source =
        toml::to_string(journal).map_err(|error| StoreError::Invalid(error.to_string()))?;
    atomic_binding_write(path, source.as_bytes(), true)
}

fn recover_binding_transaction(strategy: &Path) -> Result<(), StoreError> {
    let journal_path = strategy.join(BINDING_JOURNAL);
    // Validate every statically-known component before even probing or reading the journal.
    validate_transaction_paths(strategy, None)?;
    let Some(journal_metadata) = metadata_if_present(&journal_path)? else {
        cleanup_transaction_temps(strategy)?;
        return Ok(());
    };
    if !journal_metadata.file_type().is_file() || journal_metadata.file_type().is_symlink() {
        return Err(StoreError::Invalid(
            "binding journal must be a regular non-symlink file".into(),
        ));
    }
    let journal: BindingJournal = toml::from_str(&fs::read_to_string(&journal_path)?)
        .map_err(|error| StoreError::Invalid(format!("invalid binding journal: {error}")))?;
    if journal.schema_version != 1
        || journal.checksum != journal_checksum(&journal.source, journal.previous.as_deref())
    {
        return Err(StoreError::Invalid(
            "binding journal integrity check failed".into(),
        ));
    }
    parse_strategy_binding("strategy/bindings.toml", &journal.source)
        .map_err(|_| StoreError::Invalid("binding journal source is invalid".into()))?;
    let (target, history, _) = transaction_paths(strategy, journal.previous.as_deref());
    // The exact history name is journal-derived, so validate it before its first read.
    validate_transaction_paths(strategy, history.as_deref())?;
    let current_target = read_regular_file(&target, "binding recovery target")?;
    if current_target.as_deref() != journal.previous.as_deref().map(str::as_bytes)
        && current_target.as_deref() != Some(journal.source.as_bytes())
    {
        return Err(StoreError::Invalid(
            "binding target conflicts with recovery journal".into(),
        ));
    }
    if let (Some(history), Some(previous)) = (&history, &journal.previous) {
        match read_regular_file(history, "binding recovery history")? {
            None => {}
            Some(current) if current == previous.as_bytes() => {}
            Some(_) => {
                return Err(StoreError::Invalid(
                    "binding history conflicts with recovery journal".into(),
                ));
            }
        }
    }
    // Invalid journals and conflicting pre-states retain every artifact. Once validated, owned
    // interruption temps are disposable and every following boundary is journal-recoverable.
    cleanup_transaction_temps(strategy)?;
    if let (Some(history), Some(previous)) = (&history, &journal.previous)
        && read_regular_file(history, "binding recovery history")?.is_none()
    {
        atomic_binding_write(history, previous.as_bytes(), true)?;
    }
    if current_target.as_deref() != Some(journal.source.as_bytes()) {
        atomic_binding_write(&target, journal.source.as_bytes(), current_target.is_none())?;
    }
    if read_regular_file(&target, "binding recovery target")?.as_deref()
        != Some(journal.source.as_bytes())
    {
        return Err(StoreError::Invalid(
            "binding recovery target verification failed".into(),
        ));
    }
    if let (Some(history), Some(previous)) = (&history, &journal.previous) {
        if read_regular_file(history, "binding recovery history")?.as_deref()
            != Some(previous.as_bytes())
        {
            return Err(StoreError::Invalid(
                "binding recovery history verification failed".into(),
            ));
        }
    }
    remove_binding_file(&journal_path)
}

fn recover_binding_transactions(root: &Path) -> Result<(), StoreError> {
    let (state, active, _) = crate::activation::activation(root)?;
    // Invalid activation paths are not governed roots and must never drive transaction I/O.
    if state != ActivationState::Active {
        return Ok(());
    }
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
    validate_transaction_paths(strategy, None)?;
    let previous = read_regular_file(&target, "binding target")?;
    let previous = previous
        .map(|value| {
            String::from_utf8(value)
                .map_err(|_| StoreError::Invalid("binding must be UTF-8".into()))
        })
        .transpose()?;
    let (_, history, journal_path) = transaction_paths(strategy, previous.as_deref());
    validate_transaction_paths(strategy, history.as_deref())?;
    let checksum = journal_checksum(source, previous.as_deref());
    let journal = BindingJournal {
        schema_version: 1,
        source: source.into(),
        previous,
        checksum,
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
    use super::{
        ActivationState, BINDING_JOURNAL, BINDING_TEMP_PREFIX, BindingJournal, Store,
        cleanup_owned_temps, recover_binding_transaction, transaction_paths, write_journal,
    };
    use casefile_core::ChangeRequest;
    use std::{fs, path::Path, process::Command};
    use tempfile::TempDir;

    const OLD: &str = "schema_version = 1\nadapter = \"codex\"\nrole = \"implementation-writer\"\nmodel = \"old\"\nreasoning_effort = \"high\"\n[resolution]\nmode = \"profile\"\nvalue = \"writer\"\n";

    fn replacement() -> String {
        OLD.replace("model = \"old\"", "model = \"new\"")
    }

    fn journal(source: &str, previous: Option<&str>) -> BindingJournal {
        BindingJournal {
            schema_version: 1,
            source: source.into(),
            previous: previous.map(str::to_owned),
            checksum: super::journal_checksum(source, previous),
        }
    }

    fn prepare(strategy: &Path, previous: Option<&str>, source: &str) -> BindingJournal {
        fs::create_dir_all(strategy).expect("strategy");
        if let Some(previous) = previous {
            fs::write(strategy.join("bindings.toml"), previous).expect("target");
        }
        let journal = journal(source, previous);
        write_journal(&strategy.join(BINDING_JOURNAL), &journal).expect("journal");
        journal
    }

    fn owned_temp(directory: &Path, suffix: &str) -> std::path::PathBuf {
        fs::create_dir_all(directory).expect("temporary directory");
        let path = directory.join(format!("{BINDING_TEMP_PREFIX}{suffix}"));
        fs::write(&path, "interrupted temporary bytes").expect("temporary file");
        path
    }

    fn assert_complete(strategy: &Path, journal: &BindingJournal) {
        let (target, history, journal_path) =
            transaction_paths(strategy, journal.previous.as_deref());
        assert_eq!(
            journal.source.as_bytes(),
            fs::read(target).expect("target").as_slice()
        );
        if let Some(previous) = &journal.previous {
            assert_eq!(
                previous.as_bytes(),
                fs::read(history.expect("history"))
                    .expect("history")
                    .as_slice()
            );
        } else {
            assert!(history.is_none());
            assert!(!strategy.join("binding-history").exists());
        }
        assert!(!journal_path.exists());
    }

    #[test]
    fn interrupted_replacement_recovers_every_filesystem_visible_write_boundary() {
        let new = replacement();
        // A crash around temp-data sync is represented by an owned orphan. A crash around rename
        // or destination-directory sync is represented by either the orphan or exact final name.
        // Those are all filesystem-visible states that the supported atomic write can produce.
        for boundary in 0..6 {
            let root = TempDir::new().expect("root");
            let strategy = root.path().join("strategy");
            let target = strategy.join("bindings.toml");
            let journal = prepare(&strategy, Some(OLD), &new);
            let (_, history, journal_path) =
                transaction_paths(&strategy, journal.previous.as_deref());
            if boundary >= 1 {
                fs::create_dir_all(strategy.join("binding-history"))
                    .expect("history directory boundary");
            }
            if boundary >= 2 {
                owned_temp(&strategy.join("binding-history"), "history");
            }
            if boundary >= 3 {
                super::atomic_binding_write(
                    history.as_ref().expect("history"),
                    OLD.as_bytes(),
                    true,
                )
                .expect("history");
            }
            if boundary >= 4 {
                owned_temp(&strategy, "target");
            }
            if boundary >= 5 {
                super::atomic_binding_write(&target, new.as_bytes(), false).expect("target");
            }
            recover_binding_transaction(&strategy).expect("recovery");
            assert_complete(&strategy, &journal);
            assert!(
                !fs::read_dir(&strategy)
                    .expect("strategy entries")
                    .any(|entry| entry
                        .expect("entry")
                        .file_name()
                        .to_string_lossy()
                        .starts_with(BINDING_TEMP_PREFIX))
            );
            assert!(!journal_path.exists());
        }
    }

    #[test]
    fn interrupted_first_creation_recovers_idempotently_without_history() {
        let new = replacement();
        for boundary in 0..3 {
            let root = TempDir::new().expect("root");
            let strategy = root.path().join("strategy");
            let journal = prepare(&strategy, None, &new);
            if boundary >= 1 {
                owned_temp(&strategy, "target");
            }
            if boundary >= 2 {
                super::atomic_binding_write(&strategy.join("bindings.toml"), new.as_bytes(), true)
                    .expect("target");
            }
            recover_binding_transaction(&strategy).expect("first recovery");
            assert_complete(&strategy, &journal);
            recover_binding_transaction(&strategy).expect("idempotent recovery");
            assert_complete(&strategy, &journal);
        }
    }

    #[test]
    fn recovery_removes_only_regular_owned_orphans_and_preserves_unrelated_entries() {
        let root = TempDir::new().expect("root");
        let strategy = root.path().join("strategy");
        let history = strategy.join("binding-history");
        fs::create_dir_all(&history).expect("history");
        let strategy_temp = owned_temp(&strategy, "strategy");
        let history_temp = owned_temp(&history, "history");
        let unrelated = strategy.join("notes.txt");
        fs::write(&unrelated, "keep").expect("unrelated");
        let reserved_directory = strategy.join(format!("{BINDING_TEMP_PREFIX}directory"));
        fs::create_dir(&reserved_directory).expect("reserved directory");

        recover_binding_transaction(&strategy).expect("orphan recovery");

        assert!(!strategy_temp.exists());
        assert!(!history_temp.exists());
        assert_eq!("keep", fs::read_to_string(unrelated).expect("unrelated"));
        assert!(reserved_directory.is_dir());
    }

    #[test]
    fn invalid_journals_and_conflicting_states_preserve_all_evidence() {
        let new = replacement();
        for (name, contents) in [
            ("malformed", "not = [toml".to_owned()),
            (
                "truncated",
                "schema_version = 1\nsource = \"unterminated".to_owned(),
            ),
            (
                "wrong schema",
                toml::to_string(&BindingJournal {
                    schema_version: 2,
                    ..journal(&new, Some(OLD))
                })
                .expect("journal TOML"),
            ),
            (
                "invalid source",
                toml::to_string(&journal("not = [toml", Some(OLD))).expect("journal TOML"),
            ),
            (
                "bad checksum",
                toml::to_string(&BindingJournal {
                    checksum: "altered".into(),
                    ..journal(&new, Some(OLD))
                })
                .expect("journal TOML"),
            ),
        ] {
            let root = TempDir::new().expect("root");
            let strategy = root.path().join("strategy");
            fs::create_dir_all(&strategy).expect("strategy");
            fs::write(strategy.join("bindings.toml"), OLD).expect("target");
            let journal_path = strategy.join(BINDING_JOURNAL);
            fs::write(&journal_path, contents).expect("journal");
            let orphan = owned_temp(&strategy, name);

            assert!(
                recover_binding_transaction(&strategy).is_err(),
                "{name} must fail closed"
            );
            assert!(journal_path.is_file(), "{name} journal retained");
            assert!(orphan.is_file(), "{name} orphan evidence retained");
            assert_eq!(
                OLD,
                fs::read_to_string(strategy.join("bindings.toml")).unwrap()
            );
        }

        for conflict in ["target", "history"] {
            let root = TempDir::new().expect("root");
            let strategy = root.path().join("strategy");
            let journal = prepare(&strategy, Some(OLD), &new);
            let (target, history, journal_path) =
                transaction_paths(&strategy, journal.previous.as_deref());
            if conflict == "target" {
                fs::write(&target, "conflicting target").expect("conflict");
            } else {
                fs::create_dir_all(strategy.join("binding-history")).expect("history directory");
                fs::write(history.expect("history"), "conflicting history").expect("conflict");
            }
            let orphan = owned_temp(&strategy, conflict);
            assert!(recover_binding_transaction(&strategy).is_err());
            assert!(journal_path.is_file());
            assert!(orphan.is_file());
        }
    }

    #[test]
    fn exact_existing_history_is_accepted_and_recovery_is_idempotent() {
        let root = TempDir::new().expect("root");
        let strategy = root.path().join("strategy");
        let new = replacement();
        let journal = prepare(&strategy, Some(OLD), &new);
        let (_, history, _) = transaction_paths(&strategy, journal.previous.as_deref());
        super::atomic_binding_write(&history.expect("history"), OLD.as_bytes(), true)
            .expect("existing history");
        recover_binding_transaction(&strategy).expect("recovery");
        assert_complete(&strategy, &journal);
        recover_binding_transaction(&strategy).expect("second recovery");
        assert_complete(&strategy, &journal);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_transaction_terminals_and_parents_fail_without_traversal() {
        use std::os::unix::fs::symlink;

        let new = replacement();
        for dangling in [false, true] {
            let root = TempDir::new().expect("root");
            let strategy = root.path().join("strategy");
            fs::create_dir_all(&strategy).expect("strategy");
            let external = root.path().join("external-journal");
            if !dangling {
                fs::write(&external, "external").expect("external");
            }
            symlink(&external, strategy.join(BINDING_JOURNAL)).expect("journal symlink");
            assert!(recover_binding_transaction(&strategy).is_err());
            if !dangling {
                assert_eq!("external", fs::read_to_string(external).expect("external"));
            }
        }

        for terminal in ["bindings.toml", "history"] {
            for dangling in [false, true] {
                let root = TempDir::new().expect("root");
                let strategy = root.path().join("strategy");
                let journal = prepare(&strategy, Some(OLD), &new);
                let (target, history, journal_path) =
                    transaction_paths(&strategy, journal.previous.as_deref());
                let path = if terminal == "bindings.toml" {
                    fs::remove_file(&target).expect("remove target");
                    target
                } else {
                    fs::create_dir_all(strategy.join("binding-history"))
                        .expect("history directory");
                    history.expect("history")
                };
                let external = root.path().join(format!("external-{terminal}-{dangling}"));
                if !dangling {
                    fs::write(&external, OLD).expect("external");
                }
                symlink(&external, &path).expect("terminal symlink");
                assert!(recover_binding_transaction(&strategy).is_err());
                assert!(journal_path.is_file());
                if !dangling {
                    assert_eq!(OLD, fs::read_to_string(external).expect("external"));
                }
            }
        }

        let root = TempDir::new().expect("root");
        let external = root.path().join("external-strategy");
        prepare(&external, Some(OLD), &new);
        let strategy = root.path().join("active").join("strategy");
        fs::create_dir_all(strategy.parent().expect("active")).expect("active");
        symlink(&external, &strategy).expect("strategy symlink");
        assert!(recover_binding_transaction(&strategy).is_err());
        assert!(external.join(BINDING_JOURNAL).is_file());

        let root = TempDir::new().expect("root");
        let strategy = root.path().join("strategy");
        let journal = prepare(&strategy, Some(OLD), &new);
        let external = root.path().join("external-history");
        fs::create_dir(&external).expect("external history");
        symlink(&external, strategy.join("binding-history")).expect("history symlink");
        assert!(recover_binding_transaction(&strategy).is_err());
        assert!(strategy.join(BINDING_JOURNAL).is_file());
        assert!(
            external
                .read_dir()
                .expect("external entries")
                .next()
                .is_none()
        );
        assert_eq!(Some(OLD), journal.previous.as_deref());
    }

    fn write_activation(root: &Path, investigation: &str) {
        fs::write(
            root.join("casefile.toml"),
            format!(
                "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"{investigation}\"]\n"
            ),
        )
        .expect("activation");
    }

    #[test]
    fn nested_active_root_recovery_runs_before_scan_exposure() {
        let root = TempDir::new().expect("root");
        let investigation = "projects/demo/investigations/outer/inner";
        write_activation(root.path(), investigation);
        let strategy = root.path().join(investigation).join("strategy");
        let new = replacement();
        let journal = prepare(&strategy, Some(OLD), &new);

        Store::open(root.path())
            .expect("store")
            .scan()
            .expect("scan after recovery");

        assert_complete(&strategy, &journal);
    }

    #[cfg(unix)]
    #[test]
    fn active_root_symlink_is_rejected_before_external_journal_read() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().expect("root");
        let investigation = "projects/demo/investigations/active";
        write_activation(root.path(), investigation);
        let external = TempDir::new().expect("external");
        let strategy = external.path().join("strategy");
        let new = replacement();
        prepare(&strategy, Some(OLD), &new);
        let active_parent = root.path().join("projects/demo/investigations");
        fs::create_dir_all(&active_parent).expect("active parent");
        symlink(external.path(), active_parent.join("active")).expect("active root symlink");

        assert!(Store::open(root.path()).expect("store").scan().is_err());
        assert!(strategy.join(BINDING_JOURNAL).is_file());
        assert_eq!(
            OLD,
            fs::read_to_string(strategy.join("bindings.toml")).unwrap()
        );
    }

    #[test]
    fn invalid_activation_path_cannot_drive_transaction_io_outside_the_planning_root() {
        let holder = TempDir::new().expect("holder");
        let root = holder.path().join("planning");
        let external = holder.path().join("external");
        fs::create_dir(&root).expect("planning root");
        write_activation(&root, "../external");
        let strategy = external.join("strategy");
        let new = replacement();
        prepare(&strategy, Some(OLD), &new);

        let scan = Store::open(&root)
            .expect("store")
            .scan()
            .expect("invalid activation remains inspectable");

        assert_eq!(scan.activation, ActivationState::Invalid);
        assert!(strategy.join(BINDING_JOURNAL).is_file());
        assert_eq!(
            OLD,
            fs::read_to_string(strategy.join("bindings.toml")).unwrap()
        );
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

    fn git(root: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .current_dir(root)
                .args(args)
                .status()
                .expect("git")
                .success()
        );
    }

    #[test]
    fn preview_and_apply_recover_before_exposure_or_unrelated_mutation() {
        let root = TempDir::new().expect("root");
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
            root.path(),
        );
        git(root.path(), &["init", "-q"]);
        git(
            root.path(),
            &["config", "user.email", "casefile@example.test"],
        );
        git(root.path(), &["config", "user.name", "Casefile Test"]);
        git(root.path(), &["add", "."]);
        git(root.path(), &["commit", "-qm", "fixture"]);
        let investigation = "projects/demo/investigations/sample";
        let strategy = root.path().join(investigation).join("strategy");
        let new = replacement();
        let first = prepare(&strategy, Some(OLD), &new);
        let ticket = format!("{investigation}/tickets/accepted/HMD-011.md");
        let store = Store::open(root.path()).expect("store");

        let preview = store
            .preview(ChangeRequest::Delete {
                path: ticket.clone(),
            })
            .expect("preview");
        assert_complete(&strategy, &first);

        let newest = new.replace("model = \"new\"", "model = \"newest\"");
        let second = prepare(&strategy, Some(&new), &newest);
        assert!(store.apply(preview).is_err());
        assert_complete(&strategy, &second);
        assert!(root.path().join(ticket).is_file());
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

    #[test]
    fn cleanup_helper_ignores_a_missing_history_directory() {
        let root = TempDir::new().expect("root");
        cleanup_owned_temps(&root.path().join("missing")).expect("missing directory");
    }
}
