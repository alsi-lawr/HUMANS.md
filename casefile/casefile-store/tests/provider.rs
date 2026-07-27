use casefile_core::{
    ChangeRequest, Diagnostic, Kind, ProgressEntry, ProgressStatus, RecordDraft, Revision,
};
use casefile_store::{
    ActivationState, CacheState, NoCache, ProgressOperation, Provider, ProviderCache,
    ProviderError, ProviderMutationState, ProviderOperation, ProviderQuery, ProviderQueryResult,
    Store,
};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

const INVESTIGATION: &str = "projects/demo/investigations/sample";

fn fixture() -> TempDir {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
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

fn new_ticket(root: &Path) -> (String, RecordDraft) {
    let existing = format!("{INVESTIGATION}/tickets/accepted/HMD-011.md");
    let text = fs::read_to_string(root.join(&existing)).expect("ticket");
    let mut draft = casefile_core::parse_draft(&existing, Kind::Ticket, &text).expect("draft");
    if let RecordDraft::Ticket(item) = &mut draft {
        item.id = "HMD-099".into();
        item.title = "Provider exact preview".into();
    }
    (
        format!("{INVESTIGATION}/tickets/accepted/HMD-099.md"),
        draft,
    )
}

#[test]
fn snapshot_negotiates_one_single_scan_v1_baseline_and_queries_store_projections() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let provider = Provider::without_cache(store.clone());
    let snapshot = provider.snapshot_for_protocol(1).expect("snapshot");
    assert_eq!(snapshot.activation, ActivationState::Active);
    assert_eq!(snapshot.capabilities.protocol_version, 1);
    assert_eq!(snapshot.capabilities.planning_format_versions, [1]);
    assert_eq!(
        snapshot.capabilities.mutation,
        ProviderMutationState::ReadWrite
    );
    assert!(snapshot.capabilities.writes_require_external_approval);
    assert!(
        !snapshot
            .capabilities
            .operations
            .iter()
            .any(|operation| format!("{operation:?}").contains("Strategy"))
    );
    assert!(
        snapshot
            .capabilities
            .operations
            .contains(&ProviderOperation::ApplyProgress)
    );
    assert!(matches!(
        provider.snapshot_for_protocol(2),
        Err(ProviderError::UnsupportedProtocol { .. })
    ));

    let derived = store.derived_snapshot().expect("derived");
    assert_eq!(snapshot.revision, derived.source_revision);
    assert_eq!(snapshot.diagnostics, derived.diagnostics);
    assert_eq!(
        snapshot.projections.tickets.len(),
        derived
            .records
            .iter()
            .filter(|record| record.kind == Some(Kind::Ticket))
            .count()
    );
    match provider
        .query(ProviderQuery::Tickets {
            scope: None,
            search: Some("HMD-011".into()),
        })
        .expect("query")
    {
        ProviderQueryResult::Records { records, .. } => assert_eq!(records.len(), 1),
        other => panic!("unexpected query result: {other:?}"),
    }

    let scan = store.scan().expect("controlled baseline");
    fs::remove_file(root.path().join("casefile.toml")).expect("remove activation after scan");
    let derived_from_scan = store.derive_snapshot(&scan);
    assert_eq!(derived_from_scan.source_revision, scan.snapshot.revision);
    assert!(!derived_from_scan.records.is_empty());
    let changed = provider.snapshot().expect("new baseline");
    assert_eq!(changed.activation, ActivationState::Unactivated);
    assert!(matches!(
        changed.capabilities.mutation,
        ProviderMutationState::ReadOnly { .. }
    ));
    assert!(
        !changed
            .capabilities
            .operations
            .contains(&ProviderOperation::ApplyRecordDraft)
    );
}

#[test]
fn invalid_unactivated_and_legacy_activation_fail_closed_without_conversion() {
    for activation in [
        None,
        Some("schema_version = 2\n"),
        Some(
            "schema_version = 1\n\n[[investigations]]\npath = 'projects/demo/investigations/sample'\n",
        ),
    ] {
        let root = fixture();
        match activation {
            Some(text) => fs::write(root.path().join("casefile.toml"), text).expect("activation"),
            None => fs::remove_file(root.path().join("casefile.toml")).expect("activation removed"),
        }
        let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
        let before = fs::read(root.path().join("projects.toml")).expect("before");
        let snapshot = provider.snapshot().expect("snapshot");
        assert_ne!(snapshot.activation, ActivationState::Active);
        assert!(matches!(
            snapshot.capabilities.mutation,
            ProviderMutationState::ReadOnly { .. }
        ));
        assert!(matches!(
            provider.bootstrap_progress(INVESTIGATION),
            Err(ProviderError::ReadOnly(_))
        ));
        assert_eq!(
            before,
            fs::read(root.path().join("projects.toml")).expect("unchanged")
        );
        if activation.is_some_and(|text| text.contains("investigations")) {
            assert!(
                snapshot
                    .diagnostics
                    .iter()
                    .any(|item| item.code == "invalid_activation")
            );
        }
    }
}

#[test]
fn record_apply_requires_the_complete_provider_preview_and_preserves_store_on_alteration() {
    let root = fixture();
    let provider = Provider::new(Store::open(root.path()).expect("store"), NoCache);
    let (path, draft) = new_ticket(root.path());
    let preview = provider
        .preview_record(ChangeRequest::Create {
            path: path.clone(),
            draft,
        })
        .expect("preview");
    let mut altered = Vec::new();
    let mut value = preview.clone();
    value.canonical.request = ChangeRequest::Delete { path: path.clone() };
    altered.push(value);
    let mut value = preview.clone();
    if let ChangeRequest::Create { draft, .. } = &preview.canonical.request {
        value.canonical.request = ChangeRequest::Create {
            path: format!("{INVESTIGATION}/tickets/accepted/HMD-098.md"),
            draft: draft.clone(),
        };
    }
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.expected_target_revision = Some(Revision("altered".into()));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.expected_store_revision = Revision("altered".into());
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.proposed_store_revision = Revision("altered".into());
    altered.push(value);
    let mut value = preview.clone();
    value
        .canonical
        .diagnostics
        .push(Diagnostic::new(&path, "altered", "altered"));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.diff.push_str("altered");
    altered.push(value);
    let mut value = preview.clone();
    value.rendered_bytes.as_mut().expect("bytes").push(b'!');
    altered.push(value);
    let mut value = preview.clone();
    value.no_op = true;
    altered.push(value);
    for value in altered {
        assert!(matches!(
            provider.apply_record(value),
            Err(ProviderError::PreviewIntegrity)
        ));
        assert!(!root.path().join(&path).exists());
    }
    let result = provider.apply_record(preview).expect("exact apply");
    assert!(root.path().join(&path).is_file());
    assert_eq!(result.cache, CacheState::NotConfigured);
    let bytes = fs::read(root.path().join(&path)).expect("created bytes");
    let draft = casefile_core::parse_draft(
        &path,
        Kind::Ticket,
        std::str::from_utf8(&bytes).expect("UTF-8 ticket"),
    )
    .expect("created draft");
    let no_op = provider
        .preview_record(ChangeRequest::Replace {
            path: path.clone(),
            draft,
        })
        .expect("no-op preview");
    assert!(no_op.no_op);
    assert!(
        provider
            .apply_record(no_op)
            .expect("no-op apply")
            .result
            .no_op
    );
    assert_eq!(fs::read(root.path().join(path)).expect("preserved"), bytes);
}

#[test]
fn progress_preview_integrity_covers_bootstrap_transition_replay_no_op_and_conflict() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let preview = provider
        .bootstrap_progress(INVESTIGATION)
        .expect("bootstrap preview");
    let log = root.path().join(INVESTIGATION).join("progress/log.toml");
    let mut altered = Vec::new();
    let mut value = preview.clone();
    value.operation = ProgressOperation::Append {
        investigation: INVESTIGATION.into(),
        entries: Vec::new(),
    };
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.path.push_str(".other");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.request.investigation.push_str("-altered");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.expected_target_revision = Some(Revision("altered".into()));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.expected_store_revision = Revision("altered".into());
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.proposed_store_revision = Revision("altered".into());
    altered.push(value);
    let mut value = preview.clone();
    value
        .canonical
        .diagnostics
        .push(Diagnostic::new("progress/log.toml", "altered", "altered"));
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.diff.push_str("altered");
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.no_op = true;
    altered.push(value);
    let mut value = preview.clone();
    value.canonical.bootstrap_ticket_ids.push("HMD-999".into());
    altered.push(value);
    let mut value = preview.clone();
    value
        .canonical
        .proposed_bytes
        .as_mut()
        .expect("bytes")
        .push(b'!');
    altered.push(value);
    for value in altered {
        assert!(matches!(
            provider.apply_progress(value),
            Err(ProviderError::PreviewIntegrity)
        ));
        assert!(!log.exists());
    }
    provider.apply_progress(preview).expect("bootstrap apply");
    let operation = ProgressOperation::Append {
        investigation: INVESTIGATION.into(),
        entries: vec![ProgressEntry::Transition {
            id: "provider-start".into(),
            recorded_at: "2026-07-27T01:00:00Z".into(),
            recorded_by: "root".into(),
            ticket_id: "HMD-011".into(),
            from: ProgressStatus::Unknown,
            to: ProgressStatus::InProgress,
        }],
    };
    let transition = provider
        .preview_progress(operation.clone())
        .expect("transition preview");
    provider
        .apply_progress(transition)
        .expect("transition apply");
    let replay = provider
        .preview_progress(operation)
        .expect("replay preview");
    assert!(replay.canonical.no_op);
    assert!(
        provider
            .apply_progress(replay)
            .expect("replay apply")
            .result
            .no_op
    );
    let conflict = provider
        .preview_progress(ProgressOperation::Append {
            investigation: INVESTIGATION.into(),
            entries: vec![ProgressEntry::Transition {
                id: "provider-start".into(),
                recorded_at: "2026-07-27T01:00:00Z".into(),
                recorded_by: "root".into(),
                ticket_id: "HMD-011".into(),
                from: ProgressStatus::InProgress,
                to: ProgressStatus::Complete,
            }],
        })
        .expect("conflict preview");
    assert!(
        conflict
            .canonical
            .diagnostics
            .iter()
            .any(|item| item.code == "conflicting_progress_operation_id")
    );
    assert!(provider.apply_progress(conflict).is_err());
}

#[test]
fn default_board_is_named_exact_preview_with_preflight_collision_and_byte_preserving_no_op() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let preview = provider
        .preview_default_delivery_board(INVESTIGATION)
        .expect("board preview");
    assert!(!preview.no_op);
    assert!(
        preview
            .canonical
            .request
            .path()
            .ends_with("boards/delivery.toml")
    );
    let board = root.path().join(INVESTIGATION).join("boards/delivery.toml");
    let mut altered = preview.clone();
    altered.investigation.push_str("-altered");
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.canonical.expected_store_revision = Revision("altered".into());
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.canonical.expected_target_revision = Some(Revision("altered".into()));
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.canonical.proposed_store_revision = Revision("altered".into());
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.canonical.diagnostics.push(Diagnostic::new(
        "boards/delivery.toml",
        "altered",
        "altered",
    ));
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.canonical.diff.push_str("altered");
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    if let ChangeRequest::Create { draft, .. } = &preview.canonical.request {
        altered.canonical.request = ChangeRequest::Create {
            path: format!("{INVESTIGATION}/boards/other.toml"),
            draft: draft.clone(),
        };
    }
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.rendered_bytes.push(b'!');
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.no_op = true;
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    assert!(!board.exists());
    provider
        .apply_default_delivery_board(preview)
        .expect("board apply");
    let exact_bytes = fs::read(&board).expect("board bytes");
    let no_op = provider
        .preview_default_delivery_board(INVESTIGATION)
        .expect("no-op preview");
    assert!(no_op.no_op);
    assert!(
        provider
            .apply_default_delivery_board(no_op)
            .expect("no-op apply")
            .result
            .no_op
    );
    assert_eq!(fs::read(&board).expect("preserved"), exact_bytes);

    let different = String::from_utf8(exact_bytes.clone())
        .expect("UTF-8 board")
        .replace("title = \"Delivery\"", "title = \"Different\"")
        .into_bytes();
    assert_ne!(different, exact_bytes);
    fs::write(&board, &different).expect("collision");
    let collision = provider
        .preview_default_delivery_board(INVESTIGATION)
        .expect("collision preview");
    assert!(
        collision
            .canonical
            .diagnostics
            .iter()
            .any(|item| item.code == "default_board_collision")
    );
    assert!(provider.apply_default_delivery_board(collision).is_err());
    assert_eq!(fs::read(&board).expect("collision preserved"), different);
}

struct FailingCache;
impl ProviderCache for FailingCache {
    fn refresh(
        &self,
        _: &casefile_store::DerivedSnapshot,
        _: &dyn casefile_store::RevisionSource,
    ) -> Result<(), String> {
        Err("injected cache refresh failure".into())
    }
}

#[test]
fn cache_refresh_failure_after_a_confirmed_write_is_degraded_not_authoritative() {
    let root = fixture();
    let provider = Provider::new(Store::open(root.path()).expect("store"), FailingCache);
    let (path, draft) = new_ticket(root.path());
    let preview = provider
        .preview_record(ChangeRequest::Create {
            path: path.clone(),
            draft,
        })
        .expect("preview");
    let outcome = provider
        .apply_record(preview)
        .expect("canonical write succeeds");
    assert!(root.path().join(path).is_file());
    assert!(
        matches!(outcome.cache, CacheState::Degraded { ref message } if message.contains("injected"))
    );
    assert!(matches!(
        provider.snapshot().expect("canonical snapshot").cache,
        CacheState::Degraded { .. }
    ));
}

#[test]
fn every_provider_apply_family_refuses_a_stale_store_without_target_mutation() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let (ticket_path, draft) = new_ticket(root.path());
    let record = provider
        .preview_record(ChangeRequest::Create {
            path: ticket_path.clone(),
            draft,
        })
        .expect("record preview");
    fs::write(root.path().join("unrelated-record.txt"), "changed").expect("external change");
    assert!(matches!(
        provider.apply_record(record),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleStoreRevision
        ))
    ));
    assert!(!root.path().join(ticket_path).exists());

    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let progress = provider
        .bootstrap_progress(INVESTIGATION)
        .expect("progress preview");
    fs::write(root.path().join("unrelated-progress.txt"), "changed").expect("external change");
    assert!(matches!(
        provider.apply_progress(progress),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleStoreRevision
        ))
    ));
    assert!(
        !root
            .path()
            .join(INVESTIGATION)
            .join("progress/log.toml")
            .exists()
    );

    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let board = provider
        .preview_default_delivery_board(INVESTIGATION)
        .expect("board preview");
    fs::write(root.path().join("unrelated-board.txt"), "changed").expect("external change");
    assert!(matches!(
        provider.apply_default_delivery_board(board),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleStoreRevision
        ))
    ));
    assert!(
        !root
            .path()
            .join(INVESTIGATION)
            .join("boards/delivery.toml")
            .exists()
    );
}
