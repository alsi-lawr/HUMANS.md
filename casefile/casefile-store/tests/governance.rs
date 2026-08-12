use std::{fs, path::Path, process::Command};

use casefile_core::{Diagnostic, Kind, ProgressEntry, ProgressLog, ProgressStatus, Revision};
use casefile_store::{
    GovernedOperationKind, InvestigationScope, ProgressChangeRequest, Provider, ProviderError,
    ProviderOperation, ProviderQuery, ProviderQueryResult, Store, StrategyTransitionRequest,
    WriterBindingRequest,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const INVESTIGATION: &str = "projects/demo/investigations/sample";
const BINDING: &str = "schema_version = 1\nadapter = \"codex\"\nrole = \"implementation-writer\"\nmodel = \"gpt-5.6-terra\"\nreasoning_effort = \"medium\"\n\n[resolution]\nmode = \"runtime_override\"\nvalue = \"ticket-batch=writer\"\n";

fn fixture() -> TempDir {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
    fs::copy(
        matrix_path("casefile-implement-ticket-batch.toml"),
        strategy(temporary.path()),
    )
    .expect("complete current matrix");
    fs::create_dir_all(
        temporary
            .path()
            .join(INVESTIGATION)
            .join("strategy/transitions"),
    )
    .expect("transition directory");
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "casefile@example.test"],
        &["config", "user.name", "Casefile Test"],
        &["add", "."],
        &["commit", "-qm", "fixture"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(temporary.path())
                .args(args)
                .status()
                .expect("git")
                .success()
        );
    }
    temporary
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

fn link_directory(target: &Path, link: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("directory symlink");
    }
    #[cfg(windows)]
    {
        assert!(
            Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .status()
                .expect("directory junction")
                .success(),
            "directory junction creation failed"
        );
    }
}

fn matrix_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../adapters/codex/matrices")
        .join(name)
}

fn strategy(root: &Path) -> std::path::PathBuf {
    root.join(INVESTIGATION)
        .join("strategy/implementation.toml")
}

fn transition_request() -> StrategyTransitionRequest {
    StrategyTransitionRequest {
        investigation: INVESTIGATION.replace('/', r"\\") + "///",
        operation_id: "select-pipeline".into(),
        recorded_at: "2026-07-27T12:00:00Z".into(),
        selected_matrix_origin: "adapters/codex/matrices/casefile-implement-pipeline.toml".into(),
        selected_matrix_source: fs::read_to_string(matrix_path("casefile-implement-pipeline.toml"))
            .expect("pipeline matrix"),
        available_capabilities: vec![
            "exclusive_writer".into(),
            "shared_writable_planning".into(),
            "subagents".into(),
        ],
        preserved_work_paths: vec![r"tickets\\accepted//HMD-011.md///".into()],
        active_ownership: vec![casefile_core::ActiveOwnership {
            owner: "writer".into(),
            paths: vec![r"casefile-store\\src///".into()],
        }],
        rationale: "Human selected the pipeline strategy.".into(),
    }
}

fn write_progress(root: &Path, status: Option<ProgressStatus>) {
    let directory = root.join(INVESTIGATION).join("progress");
    fs::create_dir_all(&directory).expect("progress directory");
    let entries = status
        .map(|to| {
            vec![ProgressEntry::Transition {
                id: format!("set-{}", to.as_str()),
                recorded_at: "2026-07-27T11:00:00Z".into(),
                recorded_by: "root".into(),
                ticket_id: "HMD-011".into(),
                from: ProgressStatus::Unknown,
                to,
            }]
        })
        .unwrap_or_default();
    fs::write(
        directory.join("log.toml"),
        casefile_core::render_progress_log(&ProgressLog { entries }),
    )
    .expect("progress log");
}

#[test]
fn strategy_transition_is_strict_store_visible_idempotent_and_creates_no_backup() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let preview = store
        .preview_strategy_transition(transition_request())
        .expect("transition preview");
    assert_eq!(preview.request.investigation, INVESTIGATION);
    assert_eq!(
        preview.request.preserved_work_paths,
        ["tickets/accepted/HMD-011.md"]
    );
    assert_eq!(
        preview.request.active_ownership[0].paths,
        ["casefile-store/src"]
    );
    assert_eq!(
        preview.transition_record.preserved_work_paths,
        preview.request.preserved_work_paths
    );
    assert_eq!(preview.operation, GovernedOperationKind::StrategyTransition);
    assert_eq!(preview.changes.len(), 2);
    assert!(preview.diagnostics.is_empty());
    assert!(!preview.no_op);
    assert!(preview.changes.iter().all(|change| !change.diff.is_empty()));
    assert_eq!(
        preview.transition_record.previous_strategy_id,
        "casefile-implement-ticket-batch"
    );
    let result = store
        .apply_strategy_transition(preview)
        .expect("transition apply");
    assert_eq!(
        result.resulting_store_revision,
        store.scan().expect("result scan").snapshot.revision
    );
    assert_eq!(result.paths.len(), 2);
    let scan = store.scan().expect("scan");
    let transition = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.kind == Some(Kind::StrategyTransition))
        .expect("typed transition entry");
    assert_eq!(
        transition.classification,
        casefile_core::Classification::Governed
    );
    assert_eq!(scan.snapshot.revision, result.resulting_store_revision);
    let provider = Provider::without_cache(store.clone());
    let snapshot = provider.snapshot().expect("provider snapshot");
    assert!(
        snapshot
            .capabilities
            .operations
            .contains(&ProviderOperation::StrategyTransitions)
    );
    assert!(matches!(
        provider
            .query(ProviderQuery::StrategyTransitions {
                scope: InvestigationScope {
                    project: "demo".into(),
                    investigation: "sample".into(),
                }
            })
            .expect("transition query"),
        ProviderQueryResult::StrategyTransitions { transitions, .. } if transitions.len() == 1
    ));
    let replay = store
        .preview_strategy_transition(transition_request())
        .expect("replay preview");
    assert!(replay.no_op);
    assert!(replay.changes.iter().all(|change| change.no_op));
    assert!(
        store
            .apply_strategy_transition(replay)
            .expect("replay apply")
            .no_op
    );
    fs::copy(
        matrix_path("casefile-implement-ticket-batch.toml"),
        strategy(root.path()),
    )
    .expect("diverge governed matrix after the completed operation");
    assert!(
        store
            .preview_strategy_transition(transition_request())
            .is_err()
    );
    let strategy_dir = root.path().join(INVESTIGATION).join("strategy");
    assert!(!strategy_dir.join("backups").exists());
    assert!(!strategy_dir.join("transition-history").exists());
    assert!(
        !fs::read_dir(strategy_dir)
            .expect("strategy entries")
            .any(|entry| entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains("tmp"))
    );
}

#[test]
fn strategy_replay_equates_only_line_endings_and_keeps_raw_preview_staleness() {
    fn crlf(bytes: &[u8]) -> Vec<u8> {
        String::from_utf8(bytes.to_vec())
            .expect("UTF-8 fixture")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .into_bytes()
    }

    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let first = store
        .preview_strategy_transition(transition_request())
        .expect("first preview");
    let matrix_path = first.changes[0].path.clone();
    let record_path = first.changes[1].path.clone();
    store.apply_strategy_transition(first).expect("first apply");
    fs::write(
        root.path().join(&matrix_path),
        crlf(&fs::read(root.path().join(&matrix_path)).expect("matrix")),
    )
    .expect("CRLF matrix");
    fs::write(
        root.path().join(&record_path),
        crlf(&fs::read(root.path().join(&record_path)).expect("record")),
    )
    .expect("CRLF record");

    let normalized_identity_replay = store
        .preview_strategy_transition(transition_request())
        .expect("normalized-identity CRLF replay");
    assert!(normalized_identity_replay.no_op);
    let normalized_matrix = fs::read(root.path().join(&matrix_path)).expect("CRLF matrix bytes");
    let normalized_record = fs::read(root.path().join(&record_path)).expect("CRLF record bytes");
    store
        .apply_strategy_transition(normalized_identity_replay)
        .expect("normalized-identity replay apply");
    assert_eq!(
        fs::read(root.path().join(&matrix_path)).expect("preserved matrix"),
        normalized_matrix
    );
    assert_eq!(
        fs::read(root.path().join(&record_path)).expect("preserved record"),
        normalized_record
    );

    let scan = store.scan().expect("CRLF scan");
    let matrix_entry = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == matrix_path)
        .expect("matrix entry");
    let raw_matrix_revision = Revision(format!(
        "sha256:{:x}",
        Sha256::digest(&matrix_entry.original_bytes)
    ));
    let record_entry = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == record_path)
        .expect("record entry");
    let mut legacy = casefile_core::parse_strategy_transition(
        &record_path,
        std::str::from_utf8(&record_entry.original_bytes).expect("UTF-8 transition"),
    )
    .expect("transition record");
    legacy.selected_matrix_sha256 = raw_matrix_revision
        .0
        .strip_prefix("sha256:")
        .expect("SHA revision")
        .into();
    legacy.proposed_matrix_revision = raw_matrix_revision;
    fs::write(
        root.path().join(&record_path),
        crlf(casefile_core::render_strategy_transition(&legacy).as_bytes()),
    )
    .expect("legacy raw identity record");
    let legacy_record = fs::read(root.path().join(&record_path)).expect("legacy record bytes");
    let legacy_replay = store
        .preview_strategy_transition(transition_request())
        .expect("legacy raw-identity replay");
    assert!(legacy_replay.no_op);
    store
        .apply_strategy_transition(legacy_replay)
        .expect("legacy replay apply");
    assert_eq!(
        fs::read(root.path().join(&record_path)).expect("preserved legacy record"),
        legacy_record
    );

    let mut substantive = transition_request();
    substantive
        .selected_matrix_source
        .push_str("# presentation changed\n");
    assert!(store.preview_strategy_transition(substantive).is_err());

    let stale_root = fixture();
    let stale_store = Store::open(stale_root.path()).expect("stale store");
    let stale_preview = stale_store
        .preview_strategy_transition(transition_request())
        .expect("stale preview");
    let stale_matrix = stale_preview.changes[0].path.clone();
    let stale_path = stale_root.path().join(&stale_matrix);
    let before = fs::read(&stale_path).expect("matrix before conversion");
    let after = if before.windows(2).any(|window| window == b"\r\n") {
        String::from_utf8(before.clone())
            .expect("UTF-8 fixture")
            .replace("\r\n", "\n")
            .into_bytes()
    } else {
        crlf(&before)
    };
    assert_ne!(after, before, "stale mutation must change the raw bytes");
    fs::write(&stale_path, &after).expect("post-preview EOL conversion");
    assert_ne!(
        stale_store
            .scan()
            .expect("scan changed matrix")
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == stale_matrix)
            .expect("changed matrix entry")
            .content_revision,
        stale_preview.changes[0]
            .expected_target_revision
            .clone()
            .expect("preview matrix revision")
    );
    assert!(matches!(
        stale_store.apply_strategy_transition(stale_preview),
        Err(casefile_store::StoreError::StaleTargetRevision)
    ));
}

#[test]
fn first_phase_strategy_transition_creates_missing_matrix_and_refuses_later_recovery() {
    let root = fixture();
    fs::write(
        root.path().join("casefile.toml"),
        "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"projects/demo/investigations/sample\", \"projects/demo/investigations/other\"]\n",
    )
    .expect("activate unrelated investigation");
    let store = Store::open(root.path()).expect("store");
    let mut unrelated = transition_request();
    unrelated.investigation = "projects/demo/investigations/other".into();
    store
        .apply_strategy_transition(
            store
                .preview_strategy_transition(unrelated)
                .expect("unrelated initial transition preview"),
        )
        .expect("unrelated initial transition apply");

    fs::remove_dir_all(root.path().join(INVESTIGATION).join("strategy"))
        .expect("remove seeded strategy directory");
    let preview = store
        .preview_strategy_transition(transition_request())
        .expect("initial transition preview");
    assert_eq!(preview.transition_record.previous_strategy_id, "unselected");
    assert_eq!(
        preview.transition_record.expected_matrix_revision,
        Revision("absent".into())
    );
    assert_eq!(preview.changes[0].expected_target_revision, None);
    assert!(!preview.changes[0].no_op);

    store
        .apply_strategy_transition(preview)
        .expect("initial transition apply");
    assert_eq!(
        fs::read(strategy(root.path())).expect("created matrix"),
        fs::read(matrix_path("casefile-implement-pipeline.toml")).expect("selected matrix")
    );
    let replay = store
        .preview_strategy_transition(transition_request())
        .expect("initial transition replay");
    assert!(replay.no_op);

    fs::remove_file(strategy(root.path())).expect("remove governed matrix");
    assert!(
        store
            .preview_strategy_transition(transition_request())
            .is_err()
    );
}

#[test]
fn historical_transition_and_backup_files_remain_raw_and_untouched() {
    let root = fixture();
    let strategy = root.path().join(INVESTIGATION).join("strategy");
    let historical_record = strategy.join("transitions/20260726T120000Z-old.toml");
    let historical_backup = strategy.join("backups/implementation-old.toml");
    fs::create_dir_all(historical_backup.parent().expect("backup parent")).expect("backups");
    let legacy = b"schema_version = 1\ntimestamp = '2026-07-26T12:00:00Z'\nphase = 'implementation'\nmode = 'governed'\nprevious_strategy_id = 'old'\nselected_strategy_id = 'old'\nselected_matrix = '/old.toml'\nselected_matrix_sha256 = 'old'\nroot_binding = 'root'\ngoverned_state_updated = true\nbackup_path = ''\nrationale = 'historical'\navailable_capabilities = []\npreserved_work_paths = []\n";
    let backup = b"historical matrix bytes\n";
    fs::write(&historical_record, legacy).expect("legacy transition");
    fs::write(&historical_backup, backup).expect("legacy backup");
    let store = Store::open(root.path()).expect("store");
    let scan = store.scan().expect("scan");
    let entry = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path.ends_with("20260726T120000Z-old.toml"))
        .expect("legacy entry");
    assert_eq!(entry.classification, casefile_core::Classification::Raw);
    assert_eq!(entry.kind, None);
    store
        .apply_strategy_transition(
            store
                .preview_strategy_transition(transition_request())
                .expect("preview"),
        )
        .expect("apply");
    assert_eq!(fs::read(historical_record).expect("record"), legacy);
    assert_eq!(fs::read(historical_backup).expect("backup"), backup);
}

#[test]
fn provider_refuses_every_authoritative_transition_preview_change_without_mutation() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let preview = provider
        .preview_strategy_transition(transition_request())
        .expect("preview");
    let before = fs::read(strategy(root.path())).expect("before matrix");
    let mut altered = Vec::new();
    let mut value = preview.clone();
    value.canonical.request.investigation = INVESTIGATION.replace('/', r"\\");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.operation = GovernedOperationKind::WriterBinding;
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.request.rationale.push_str(" altered");
    altered.push(value);
    let mut value = preview.clone();
    value
        .canonical
        .transition_record
        .rationale
        .push_str(" altered");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].path.push_str(".other");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].rendered_bytes.push(b'!');
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].diff.push_str("altered");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].expected_target_revision = Some(Revision("altered".into()));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].proposed_target_revision = Some(Revision("altered".into()));
    altered.push(value);
    let mut value = preview.clone();
    value
        .canonical
        .diagnostics
        .push(Diagnostic::new("x", "x", "x"));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.no_op = true;
    altered.push(value);
    for value in altered {
        assert!(matches!(
            provider.apply_strategy_transition(value),
            Err(ProviderError::PreviewIntegrity)
        ));
        assert_eq!(fs::read(strategy(root.path())).expect("matrix"), before);
        assert_eq!(
            fs::read_dir(root.path().join(INVESTIGATION).join("strategy/transitions"))
                .expect("transitions")
                .count(),
            0
        );
    }
}

#[test]
fn transition_accepts_unrelated_changes_and_preserves_collision_validation_and_rollback() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let preview = store
        .preview_strategy_transition(transition_request())
        .expect("preview");
    fs::write(root.path().join("unrelated"), "changed").expect("unrelated change");
    store
        .apply_strategy_transition(preview)
        .expect("unrelated change does not invalidate transition");

    let collision_root = fixture();
    let collision_store = Store::open(collision_root.path()).expect("store");
    let preview = collision_store
        .preview_strategy_transition(transition_request())
        .expect("preview");
    let record = collision_root.path().join(&preview.changes[1].path);
    fs::write(&record, "schema_version = 1\ndifferent = true\n").expect("collision");
    assert!(
        collision_store
            .preview_strategy_transition(transition_request())
            .is_err()
    );

    let invalid_root = fixture();
    let invalid_store = Store::open(invalid_root.path()).expect("store");
    let mut invalid = transition_request();
    invalid.available_capabilities.clear();
    assert!(invalid_store.preview_strategy_transition(invalid).is_err());
    let mut overlap = transition_request();
    overlap.active_ownership = vec![
        casefile_core::ActiveOwnership {
            owner: "one".into(),
            paths: vec!["source".into()],
        },
        casefile_core::ActiveOwnership {
            owner: "two".into(),
            paths: vec![r"source\\nested///".into()],
        },
    ];
    assert!(
        !invalid_store
            .preview_strategy_transition(overlap)
            .expect("diagnostic preview")
            .diagnostics
            .is_empty()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let rollback_root = fixture();
        let rollback_store = Store::open(rollback_root.path()).expect("store");
        let before = fs::read(strategy(rollback_root.path())).expect("before");
        let preview = rollback_store
            .preview_strategy_transition(transition_request())
            .expect("preview");
        let transitions = rollback_root
            .path()
            .join(INVESTIGATION)
            .join("strategy/transitions");
        fs::set_permissions(&transitions, fs::Permissions::from_mode(0o555))
            .expect("read-only transitions");
        let result = rollback_store.apply_strategy_transition(preview);
        fs::set_permissions(&transitions, fs::Permissions::from_mode(0o755))
            .expect("restore permissions");
        assert!(result.is_err());
        assert_eq!(
            fs::read(strategy(rollback_root.path())).expect("restored"),
            before
        );
        assert_eq!(fs::read_dir(transitions).expect("empty").count(), 0);
    }
}

#[test]
fn binding_activity_is_derived_exactly_from_canonical_progress_and_spawn_requires_in_progress() {
    for (status, permitted) in [
        (ProgressStatus::Unknown, true),
        (ProgressStatus::Complete, true),
        (ProgressStatus::InProgress, false),
        (ProgressStatus::InReview, false),
        (ProgressStatus::Verifying, false),
        (ProgressStatus::Blocked, false),
    ] {
        let root = fixture();
        write_progress(
            root.path(),
            (status != ProgressStatus::Unknown).then_some(status),
        );
        let store = Store::open(root.path()).expect("store");
        let result = store.preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.replace('/', r"\\") + "///",
            binding_source: BINDING.into(),
        });
        assert_eq!(result.is_ok(), permitted, "status {status:?}");
        assert_eq!(
            store
                .require_writer_progress(&(INVESTIGATION.replace('/', r"\\") + "///"), "HMD-011",)
                .is_ok(),
            status == ProgressStatus::InProgress,
            "spawn status {status:?}"
        );
    }
    let no_tickets = fixture();
    fs::remove_file(
        no_tickets
            .path()
            .join(INVESTIGATION)
            .join("tickets/accepted/HMD-011.md"),
    )
    .expect("remove accepted ticket");
    write_progress(no_tickets.path(), None);
    Store::open(no_tickets.path())
        .expect("store")
        .preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.into(),
            binding_source: BINDING.into(),
        })
        .expect("initial binding before tickets");

    let missing = fixture();
    let store = Store::open(missing.path()).expect("store");
    store
        .preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.into(),
            binding_source: BINDING.into(),
        })
        .expect("absent progress is inactive");
    assert!(
        store
            .require_writer_progress(INVESTIGATION, "HMD-011")
            .is_err()
    );

    for source in [
        "not = [toml",
        "schema_version = 1\n[[entries]]\nid = 'unsupported'\nrecorded_at = '2026-07-27T11:00:00Z'\nrecorded_by = 'root'\nticket_id = 'HMD-011'\nkind = 'transition'\nfrom = 'unknown'\nto = 'paused'\n",
        "schema_version = 1\n[[entries]]\nid = 'conflict'\nrecorded_at = '2026-07-27T11:00:00Z'\nrecorded_by = 'root'\nticket_id = 'HMD-999'\nkind = 'transition'\nfrom = 'unknown'\nto = 'complete'\n",
    ] {
        let root = fixture();
        let progress = root.path().join(INVESTIGATION).join("progress");
        fs::create_dir_all(&progress).expect("progress");
        fs::write(progress.join("log.toml"), source).expect("invalid progress");
        let store = Store::open(root.path()).expect("store");
        assert!(
            store
                .preview_writer_binding(WriterBindingRequest {
                    investigation: INVESTIGATION.into(),
                    binding_source: BINDING.into(),
                })
                .is_err()
        );
        assert!(
            store
                .require_writer_progress(INVESTIGATION, "HMD-011")
                .is_err()
        );
    }
}

#[test]
fn binding_provider_preview_is_complete_strict_atomic_and_has_no_archive_or_scratch_escape() {
    let root = fixture();
    write_progress(root.path(), None);
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let preview = provider
        .preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.replace('/', r"\\") + "///",
            binding_source: BINDING.into(),
        })
        .expect("preview");
    assert_eq!(preview.canonical.request.investigation, INVESTIGATION);
    assert_eq!(
        preview.canonical.operation,
        GovernedOperationKind::WriterBinding
    );
    assert_eq!(preview.canonical.changes.len(), 1);
    assert!(!preview.canonical.changes[0].diff.is_empty());
    let target = root.path().join(&preview.canonical.changes[0].path);
    let mut altered = Vec::new();
    let mut value = preview.clone();
    value.canonical.operation = GovernedOperationKind::StrategyTransition;
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.request.binding_source.push_str("# altered");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].path.push_str(".other");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].rendered_bytes.push(b'!');
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].diff.push_str("altered");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].expected_target_revision = Some(Revision("altered".into()));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.changes[0].proposed_target_revision = Some(Revision("altered".into()));
    altered.push(value);
    let mut value = preview.clone();
    value
        .canonical
        .diagnostics
        .push(Diagnostic::new("x", "x", "x"));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.no_op = true;
    altered.push(value);
    for value in altered {
        assert!(matches!(
            provider.apply_writer_binding(value),
            Err(ProviderError::PreviewIntegrity)
        ));
        assert!(!target.exists());
    }
    fs::write(&target, BINDING).expect("binding appeared after preview");
    assert!(matches!(
        provider.apply_writer_binding(preview.clone()),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleTargetRevision
        ))
    ));
    fs::remove_file(&target).expect("remove stale binding");
    let result = provider.apply_writer_binding(preview).expect("apply");
    assert!(!result.result.no_op);
    assert_eq!(fs::read_to_string(&target).expect("binding"), BINDING);
    let replay = provider
        .preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.into(),
            binding_source: BINDING.into(),
        })
        .expect("no-op preview");
    assert!(replay.canonical.no_op);
    assert!(
        provider
            .apply_writer_binding(replay)
            .expect("no-op")
            .result
            .no_op
    );
    let strategy = root.path().join(INVESTIGATION).join("strategy");
    assert!(!strategy.join("binding-history").exists());
    assert!(!strategy.join(".binding-transaction.toml").exists());
    let capabilities = provider
        .snapshot()
        .expect("snapshot")
        .capabilities
        .operations;
    assert!(capabilities.contains(&ProviderOperation::ApplyWriterBinding));
    assert!(
        capabilities
            .iter()
            .all(|operation| !format!("{operation:?}").contains("Scratch"))
    );
}

#[test]
fn binding_accepts_unrelated_changes_and_refuses_invalid_schema_and_symlink_targets() {
    let root = fixture();
    write_progress(root.path(), None);
    let store = Store::open(root.path()).expect("store");
    for source in [
        "not = [toml",
        &(BINDING.to_owned() + "unknown = true\n"),
        &BINDING.replace("adapter = \"codex\"", "adapter = \"claude\""),
    ] {
        assert!(
            store
                .preview_writer_binding(WriterBindingRequest {
                    investigation: INVESTIGATION.into(),
                    binding_source: source.into(),
                })
                .is_err()
        );
    }
    let preview = store
        .preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.into(),
            binding_source: BINDING.into(),
        })
        .expect("preview");
    fs::write(root.path().join("unrelated"), "changed").expect("unrelated");
    store
        .apply_writer_binding(preview)
        .expect("unrelated change does not invalidate binding");
    assert!(
        root.path()
            .join(INVESTIGATION)
            .join("strategy/bindings.toml")
            .exists()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let symlink_root = fixture();
        write_progress(symlink_root.path(), None);
        let external = symlink_root.path().join("external-binding");
        fs::write(&external, "external").expect("external");
        symlink(
            &external,
            symlink_root
                .path()
                .join(INVESTIGATION)
                .join("strategy/bindings.toml"),
        )
        .expect("symlink");
        let store = Store::open(symlink_root.path()).expect("store");
        assert!(
            store
                .preview_writer_binding(WriterBindingRequest {
                    investigation: INVESTIGATION.into(),
                    binding_source: BINDING.into(),
                })
                .is_err()
        );
        assert_eq!(fs::read_to_string(external).expect("preserved"), "external");
    }
}

#[test]
fn governed_and_progress_writes_allow_a_symlinked_ancestor_above_the_store_root() {
    let actual = fixture();
    write_progress(actual.path(), None);
    let aliases = TempDir::new().expect("alias root");
    let actual_parent = actual.path().parent().expect("actual parent");
    let ancestor_link = aliases.path().join("linked-external-ancestor");
    link_directory(actual_parent, &ancestor_link);
    let linked_root = ancestor_link.join(actual.path().file_name().expect("Store directory name"));

    let store = Store::open(&linked_root).expect("real Store below linked ancestor");
    let binding = store
        .preview_writer_binding(WriterBindingRequest {
            investigation: INVESTIGATION.into(),
            binding_source: BINDING.into(),
        })
        .expect("binding preview below linked ancestor");
    store
        .apply_writer_binding(binding)
        .expect("binding apply below linked ancestor");

    let progress = store
        .preview_progress(ProgressChangeRequest {
            investigation: INVESTIGATION.into(),
            entries: vec![ProgressEntry::Transition {
                id: "linked-ancestor-start".into(),
                recorded_at: "2026-07-27T12:10:00Z".into(),
                recorded_by: "root".into(),
                ticket_id: "HMD-011".into(),
                from: ProgressStatus::Unknown,
                to: ProgressStatus::InProgress,
            }],
            replacement: None,
            replacement_source: None,
            bootstrap: false,
        })
        .expect("progress preview below linked ancestor");
    store
        .apply_progress(progress)
        .expect("progress apply below linked ancestor");

    assert!(
        linked_root
            .join(INVESTIGATION)
            .join("strategy/bindings.toml")
            .is_file()
    );
    assert!(
        fs::read_to_string(linked_root.join(INVESTIGATION).join("progress/log.toml"))
            .expect("progress log")
            .contains("linked-ancestor-start")
    );

    #[cfg(unix)]
    {
        let root_link = aliases.path().join("Store-root-link");
        std::os::unix::fs::symlink(actual.path(), &root_link).expect("Store root symlink");
        assert!(
            Store::open(root_link).is_err(),
            "Store root symlink accepted"
        );
    }
}

#[test]
fn progress_transition_to_in_progress_is_required_again_after_interruption() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    write_progress(root.path(), None);
    assert!(
        store
            .require_writer_progress(INVESTIGATION, "HMD-011")
            .is_err()
    );
    let start = ProgressEntry::Transition {
        id: "writer-start".into(),
        recorded_at: "2026-07-27T12:10:00Z".into(),
        recorded_by: "root".into(),
        ticket_id: "HMD-011".into(),
        from: ProgressStatus::Unknown,
        to: ProgressStatus::InProgress,
    };
    let preview = store
        .preview_progress(ProgressChangeRequest {
            investigation: INVESTIGATION.into(),
            entries: vec![start],
            replacement: None,
            replacement_source: None,
            bootstrap: false,
        })
        .expect("start preview");
    store.apply_progress(preview).expect("start apply");
    assert!(
        store
            .require_writer_progress(INVESTIGATION, "HMD-011")
            .is_ok()
    );
    let interrupt = ProgressEntry::Transition {
        id: "writer-blocked".into(),
        recorded_at: "2026-07-27T12:11:00Z".into(),
        recorded_by: "root".into(),
        ticket_id: "HMD-011".into(),
        from: ProgressStatus::InProgress,
        to: ProgressStatus::Blocked,
    };
    let preview = store
        .preview_progress(ProgressChangeRequest {
            investigation: INVESTIGATION.into(),
            entries: vec![interrupt],
            replacement: None,
            replacement_source: None,
            bootstrap: false,
        })
        .expect("blocked preview");
    store.apply_progress(preview).expect("blocked apply");
    assert!(
        store
            .require_writer_progress(INVESTIGATION, "HMD-011")
            .is_err()
    );
}
