use super::*;
use std::{cell::Cell, fs, path::Path, process::Command};
use tempfile::TempDir;

const INVESTIGATION: &str = "projects/demo/investigations/sample";

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("directory");
    for entry in fs::read_dir(from).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("fixture file");
        }
    }
}

fn committed_progress<C: ProviderCache>(
    cache: C,
) -> (TempDir, Provider<C>, ProviderPreview, ProgressApplyResult) {
    let temporary = TempDir::new().expect("temporary root");
    let root = temporary.path().join("store");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
        &root,
    );
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("git init")
            .success()
    );
    let provider = Provider::new(Store::open(root).expect("store"), cache);
    let preview = provider.bootstrap_progress(INVESTIGATION).expect("preview");
    let original = provider
        .retained(&preview.preview_id)
        .expect("retained original");
    let StoredPreview::Progress(canonical) = original.as_ref() else {
        panic!("progress original")
    };
    let result = provider
        .store
        .apply_progress_ref(canonical)
        .expect("canonical commit");
    assert!(!result.no_op);
    (temporary, provider, preview, result)
}

#[test]
fn unconfigured_outcomes_do_not_read_the_store_after_commit_or_replay() {
    let (temporary, provider, preview, committed) = committed_progress(NoCache);
    let root = temporary.path().join("store");
    let hidden = temporary.path().join("committed");
    let bytes = fs::read(root.join(&committed.path)).expect("committed bytes");

    // Move the fixture only to inject a deterministic read failure at the outcome boundary.
    fs::rename(&root, &hidden).expect("inject unavailable root");
    assert!(provider.store.scan().is_err());
    let outcome = provider
        .outcome(committed.clone())
        .expect("committed outcome");
    assert_eq!(outcome.result, committed);
    assert_eq!(outcome.cache, CacheState::NotConfigured);
    assert_eq!(
        fs::read(hidden.join(&committed.path)).expect("bytes"),
        bytes
    );

    fs::rename(&hidden, &root).expect("restore root");
    let replay = provider
        .apply_progress(&preview.preview_id)
        .expect("supported replay");
    assert!(replay.result.no_op);
    assert_eq!(
        replay.result.resulting_target_revision,
        committed.resulting_target_revision
    );
    assert_eq!(fs::read(root.join(&committed.path)).expect("bytes"), bytes);

    fs::rename(&root, &hidden).expect("inject unavailable root");
    let outcome = provider
        .outcome(replay.result.clone())
        .expect("replay outcome");
    assert_eq!(outcome.result, replay.result);
    assert_eq!(outcome.cache, CacheState::NotConfigured);
    assert_eq!(
        fs::read(hidden.join(&committed.path)).expect("bytes"),
        bytes
    );
}

#[derive(Default)]
struct CountingCache {
    refreshes: Cell<usize>,
}

impl ProviderCache for CountingCache {
    fn observe(&self, _: &Revision) -> CacheState {
        CacheState::Missing
    }

    fn refresh(&self, _: &DerivedSnapshot, _: &dyn RevisionSource) -> Result<(), String> {
        self.refreshes.set(self.refreshes.get() + 1);
        Ok(())
    }
}

#[test]
fn cache_preparation_failure_preserves_committed_receipt_bytes_and_supported_replay() {
    let (temporary, provider, preview, committed) = committed_progress(CountingCache::default());
    let root = temporary.path().join("store");
    let hidden = temporary.path().join("committed");
    let bytes = fs::read(root.join(&committed.path)).expect("committed bytes");

    // The failure is before cache.refresh(), not a failure of cache publication.
    fs::rename(&root, &hidden).expect("inject unavailable root");
    assert!(provider.store.scan().is_err());
    let outcome = provider
        .outcome(committed.clone())
        .expect("committed outcome");
    assert_eq!(outcome.result, committed);
    assert!(matches!(outcome.cache, CacheState::Degraded { .. }));
    assert_eq!(provider.cache.refreshes.get(), 0);
    assert_eq!(
        fs::read(hidden.join(&committed.path)).expect("bytes"),
        bytes
    );
    assert!(provider.refresh_full_cache().is_err());

    fs::rename(&hidden, &root).expect("restore root");
    reconcile(&provider);
    let replay = provider
        .apply_progress(&preview.preview_id)
        .expect("supported replay");
    assert!(replay.result.no_op);
    assert_eq!(
        replay.result.resulting_target_revision,
        committed.resulting_target_revision
    );
    assert_eq!(fs::read(root.join(&committed.path)).expect("bytes"), bytes);
}

#[derive(Default)]
struct RetainedCache {
    snapshot: std::cell::RefCell<Option<DerivedSnapshot>>,
    refuse_publication: Cell<bool>,
    observation_error: Cell<bool>,
    edit_during_observe: std::cell::RefCell<Option<std::path::PathBuf>>,
}

impl ProviderCache for RetainedCache {
    fn observe(&self, revision: &Revision) -> CacheState {
        if self.observation_error.get() {
            return CacheState::Degraded {
                message: "cache observation unavailable".into(),
            };
        }
        if let Some(path) = self.edit_during_observe.borrow_mut().take() {
            fs::write(path, "external edit during cache observation").unwrap();
        }
        match self.snapshot.borrow().as_ref() {
            None => CacheState::Missing,
            Some(snapshot) if &snapshot.source_revision == revision => CacheState::Current {
                source_revision: revision.clone(),
            },
            Some(snapshot) => CacheState::Stale {
                indexed_revision: snapshot.source_revision.clone(),
                current_revision: revision.clone(),
            },
        }
    }
    fn refresh(&self, snapshot: &DerivedSnapshot, _: &dyn RevisionSource) -> Result<(), String> {
        if self.refuse_publication.get() {
            return Err("cache publication unavailable".into());
        }
        *self.snapshot.borrow_mut() = Some(snapshot.clone());
        Ok(())
    }
}

fn configured_fixture() -> (TempDir, Provider<RetainedCache>) {
    let temporary = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
        temporary.path(),
    );
    let provider = Provider::new(
        Store::open(temporary.path()).unwrap(),
        RetainedCache::default(),
    );
    (temporary, provider)
}

fn reconcile<C: ProviderCache>(provider: &Provider<C>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if matches!(
            provider.refresh_full_cache().unwrap(),
            CacheState::Current { .. }
        ) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "native invalidation did not settle"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn current_cache_survives_publication_failure_but_required_external_changes_do_not() {
    let (temporary, provider) = configured_fixture();
    reconcile(&provider);
    provider.cache.refuse_publication.set(true);
    assert!(matches!(
        provider.refresh_full_cache().unwrap(),
        CacheState::Current { .. }
    ));
    fs::write(temporary.path().join("external.md"), "external body").unwrap();
    assert!(matches!(
        provider.refresh_full_cache().unwrap(),
        CacheState::Degraded { .. }
    ));
    provider.cache.refuse_publication.set(false);
    reconcile(&provider);
    assert!(
        provider
            .cache
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .records
            .iter()
            .any(|record| record.path == "external.md")
    );
}

fn wait_for_native_invalidation(provider: &Provider<RetainedCache>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !provider
        .watch
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .observe()
        .dirty
    {
        assert!(
            std::time::Instant::now() < deadline,
            "native change was not delivered"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn native_watch_create_edit_rename_delete_reconciles_canonical_index_content() {
    let (temporary, provider) = configured_fixture();
    reconcile(&provider);
    let first = temporary.path().join("native.md");
    fs::write(&first, "first body").unwrap();
    wait_for_native_invalidation(&provider);
    reconcile(&provider);
    fs::write(&first, "second body").unwrap();
    wait_for_native_invalidation(&provider);
    reconcile(&provider);
    assert_eq!(
        provider
            .cache
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .records
            .iter()
            .find(|record| record.path == "native.md")
            .unwrap()
            .content
            .as_deref(),
        Some("second body")
    );
    fs::rename(&first, temporary.path().join("renamed.md")).unwrap();
    wait_for_native_invalidation(&provider);
    reconcile(&provider);
    {
        let snapshot = provider.cache.snapshot.borrow();
        let snapshot = snapshot.as_ref().unwrap();
        assert!(
            !snapshot
                .records
                .iter()
                .any(|record| record.path == "native.md")
        );
        assert!(
            snapshot
                .records
                .iter()
                .any(|record| record.path == "renamed.md")
        );
    }
    fs::remove_file(temporary.path().join("renamed.md")).unwrap();
    wait_for_native_invalidation(&provider);
    reconcile(&provider);
    assert!(
        !provider
            .cache
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .records
            .iter()
            .any(|record| record.path == "renamed.md")
    );
}

#[test]
fn events_and_rescan_require_rebuild_even_when_metadata_revision_is_unchanged() {
    use notify::event::{Flag, ModifyKind};
    let (temporary, provider) = configured_fixture();
    reconcile(&provider);
    provider.cache.refuse_publication.set(true);
    #[cfg(windows)]
    {
        let inject = |path| {
            provider
                .watch
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .inject(Ok(notify::Event::new(notify::EventKind::Modify(
                    ModifyKind::Any,
                ))
                .add_path(path)));
        };
        inject(temporary.path().join(INVESTIGATION));
        assert!(matches!(
            provider.refresh_full_cache().unwrap(),
            CacheState::Current { .. }
        ));
        inject(
            temporary
                .path()
                .join(format!("{INVESTIGATION}/tickets/accepted/HMD-011.md")),
        );
        assert!(matches!(
            provider.refresh_full_cache().unwrap(),
            CacheState::Degraded { .. }
        ));
        provider.cache.refuse_publication.set(false);
        reconcile(&provider);
        provider.cache.refuse_publication.set(true);
    }
    let mut event = notify::Event::new(notify::EventKind::Modify(ModifyKind::Any))
        .add_path(temporary.path().join(INVESTIGATION));
    event.attrs.set_flag(Flag::Rescan);
    provider
        .watch
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .inject(Ok(event));
    assert!(matches!(
        provider.refresh_full_cache().unwrap(),
        CacheState::Degraded { .. }
    ));
    provider.cache.refuse_publication.set(false);
    reconcile(&provider);
    provider
        .watch
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .inject(Err(notify::Error::generic("native watch error")));
    reconcile(&provider);
}

#[test]
fn observation_window_external_edit_cannot_claim_the_old_index_is_current() {
    let (temporary, provider) = configured_fixture();
    reconcile(&provider);
    *provider.cache.edit_during_observe.borrow_mut() = Some(temporary.path().join("window.md"));
    assert!(!matches!(
        provider.refresh_full_cache().unwrap(),
        CacheState::Current { .. }
    ));
    reconcile(&provider);
    assert!(
        provider
            .cache
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .records
            .iter()
            .any(|record| record.path == "window.md")
    );
}

#[cfg(unix)]
#[test]
fn native_watch_keeps_opaque_symlink_descendant_changes_outside_canonical_cache() {
    let (temporary, provider) = configured_fixture();
    let outside = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), temporary.path().join("opaque-link")).unwrap();
    reconcile(&provider);
    provider.cache.refuse_publication.set(true);
    fs::write(
        outside.path().join("external.md"),
        "outside canonical Store",
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    while std::time::Instant::now() < deadline {
        assert!(matches!(
            provider.refresh_full_cache().unwrap(),
            CacheState::Current { .. }
        ));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        !provider
            .cache
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .records
            .iter()
            .any(|record| record.path.ends_with("external.md"))
    );
}

#[test]
fn ordinary_cache_observation_error_preserves_the_previous_publication_without_repair() {
    let (_temporary, provider) = configured_fixture();
    reconcile(&provider);
    let retained_revision = provider
        .cache
        .snapshot
        .borrow()
        .as_ref()
        .unwrap()
        .source_revision
        .clone();
    provider.cache.observation_error.set(true);
    provider.cache.refuse_publication.set(true);
    let CacheState::Degraded { message } = provider.refresh_full_cache().unwrap() else {
        panic!("ordinary cache error")
    };
    assert_eq!(message, "cache observation unavailable");
    assert_eq!(
        provider
            .cache
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .source_revision,
        retained_revision
    );
    provider.cache.observation_error.set(false);
    assert!(matches!(
        provider.refresh_full_cache().unwrap(),
        CacheState::Current { .. }
    ));
}
