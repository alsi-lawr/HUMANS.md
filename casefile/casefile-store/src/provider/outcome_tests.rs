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
) -> (
    TempDir,
    Provider<C>,
    ProviderProgressPreview,
    ProgressApplyResult,
) {
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
    let result = provider
        .store
        .apply_progress(preview.canonical.clone())
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
    let replay = provider.apply_progress(preview).expect("supported replay");
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
    assert!(matches!(
        provider.refresh_full_cache().expect("explicit refresh"),
        CacheState::Current { .. }
    ));
    assert_eq!(provider.cache.refreshes.get(), 1);
    let replay = provider.apply_progress(preview).expect("supported replay");
    assert!(replay.result.no_op);
    assert_eq!(
        replay.result.resulting_target_revision,
        committed.resulting_target_revision
    );
    assert_eq!(fs::read(root.join(&committed.path)).expect("bytes"), bytes);
}
