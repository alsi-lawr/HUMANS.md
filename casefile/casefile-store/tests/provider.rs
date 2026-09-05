use casefile_core::{
    ChangeRequest, Diagnostic, Kind, ProgressEntry, ProgressStatus, RecordDraft, Revision,
};
use casefile_store::{
    ActivationState, CacheState, InvestigationScope, InvestigationScopedIdentity, NoCache,
    ProgressOperation, Provider, ProviderApprovalPolicy, ProviderCache, ProviderError,
    ProviderMutationState, ProviderOperation, ProviderQuery, ProviderQueryResult, Store,
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
fn snapshot_and_exact_scoped_reads_are_bounded_protocol_v3() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let provider = Provider::without_cache(store.clone());
    provider
        .apply_progress(
            provider
                .bootstrap_progress(INVESTIGATION)
                .expect("bootstrap preview"),
        )
        .expect("bootstrap apply");
    provider
        .apply_progress(
            provider
                .preview_progress(ProgressOperation::Append {
                    investigation: INVESTIGATION.into(),
                    entries: vec![ProgressEntry::Transition {
                        id: "query-progress".into(),
                        recorded_at: "2026-07-27T00:30:00Z".into(),
                        recorded_by: "root".into(),
                        ticket_id: "HMD-011".into(),
                        from: ProgressStatus::Unknown,
                        to: ProgressStatus::InProgress,
                    }],
                })
                .expect("progress preview"),
        )
        .expect("progress apply");
    let snapshot = provider.snapshot_for_protocol(3).expect("snapshot");
    assert_eq!(snapshot.activation, ActivationState::Active);
    assert_eq!(snapshot.capabilities.protocol_version, 3);
    assert_eq!(snapshot.capabilities.planning_format_versions, [1]);
    assert_eq!(
        snapshot.capabilities.mutation,
        ProviderMutationState::ReadWrite
    );
    assert!(snapshot.capabilities.writes_require_external_approval);
    assert_eq!(
        snapshot.capabilities.approval_policy,
        ProviderApprovalPolicy::RecordDeletesOnly
    );
    assert!(
        snapshot
            .capabilities
            .operations
            .contains(&ProviderOperation::ApplyStrategyTransition)
    );
    assert!(
        snapshot
            .capabilities
            .operations
            .contains(&ProviderOperation::ApplyWriterBinding)
    );
    assert!(
        snapshot
            .capabilities
            .operations
            .iter()
            .all(|operation| !format!("{operation:?}").contains("Scratch"))
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

    fs::write(root.path().join(".git/hmd-046-provider-probe"), "changed")
        .expect("Git metadata probe");
    fs::create_dir_all(root.path().join(".agent-workspace/provider"))
        .expect("agent workspace probe");
    fs::write(
        root.path().join(".agent-workspace/provider/probe.txt"),
        "changed",
    )
    .expect("agent workspace probe file");
    assert_eq!(
        provider.snapshot().expect("metadata-stable snapshot"),
        snapshot
    );

    assert_eq!(snapshot.catalogue.projects[0].name, "demo");
    assert_eq!(
        snapshot.catalogue.projects[0].source_root.as_deref(),
        Some("/source/demo")
    );
    assert!(snapshot.catalogue.projects[0].governed);
    assert_eq!(
        snapshot.catalogue.projects[0].prefix.as_deref(),
        Some("HMD")
    );
    assert_eq!(
        snapshot.catalogue.projects[0].investigations[0].identity,
        "sample"
    );
    assert_eq!(
        snapshot.diagnostic_coverage.records,
        casefile_store::ProviderRecordDiagnosticCoverage::NotLoaded
    );
    let encoded = serde_json::to_string(&snapshot).expect("snapshot JSON");
    for forbidden in [
        "projections",
        "content",
        "rendered_markdown",
        "search_text",
        "work_item",
        "original_bytes",
    ] {
        assert!(!encoded.contains(forbidden), "root leaked {forbidden}");
    }
    let scope = InvestigationScope {
        project: "demo".into(),
        investigation: "sample".into(),
    };
    match provider
        .query(ProviderQuery::RecordIndex {
            scope: InvestigationScope {
                project: "demo///".into(),
                investigation: r"sample\\".into(),
            },
        })
        .expect("portable scoped query")
    {
        ProviderQueryResult::RecordIndex {
            records,
            revision,
            diagnostic_coverage,
            ..
        } => {
            assert_eq!(records.len(), 2);
            assert_eq!(revision, snapshot.revision);
            assert_eq!(diagnostic_coverage.scope.project, "demo");
            assert_eq!(
                diagnostic_coverage.kind,
                casefile_store::ProviderIndexDiagnosticCoverageKind::LocalAndInvestigation
            );
            let ticket = records
                .iter()
                .find(|record| record.identity.as_deref() == Some("HMD-011"))
                .expect("ticket index");
            assert_eq!(
                ticket.progress.as_ref().expect("progress").status,
                ProgressStatus::InProgress
            );
        }
        other => panic!("unexpected scoped query result: {other:?}"),
    }
    match provider
        .query(ProviderQuery::Boards {
            scope: scope.clone(),
        })
        .expect("purpose-built scoped boards")
    {
        ProviderQueryResult::Boards {
            revision, boards, ..
        } => {
            assert_eq!(revision, snapshot.revision);
            let canonical_scan = store.scan().expect("canonical board comparison scan");
            assert_eq!(boards, store.derive_snapshot(&canonical_scan).boards);
            assert_eq!(boards[0].columns[0].cards[0].identity.identity, "HMD-011");
        }
        other => panic!("unexpected scoped board result: {other:?}"),
    }
    let identity = InvestigationScopedIdentity {
        scope,
        identity: "HMD-011".into(),
    };
    assert!(matches!(
        provider.query(ProviderQuery::RecordDetail { identity }),
        Ok(ProviderQueryResult::RecordDetail {
            record: Some(_),
            ..
        })
    ));

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

#[cfg(unix)]
#[test]
fn hierarchical_reads_are_selective_bounded_and_exact() {
    use std::os::unix::fs::PermissionsExt;

    let root = fixture();
    let ticket = root
        .path()
        .join(format!("{INVESTIGATION}/tickets/accepted/HMD-011.md"));
    let baseline = fs::read_to_string(&ticket).expect("ticket");
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let snapshot_before = serde_json::to_vec(&provider.snapshot().expect("root")).expect("JSON");
    let scope = InvestigationScope {
        project: "demo".into(),
        investigation: "sample".into(),
    };
    let index_before = serde_json::to_vec(
        &provider
            .query(ProviderQuery::RecordIndex {
                scope: scope.clone(),
            })
            .expect("index before"),
    )
    .expect("index JSON");
    fs::write(
        &ticket,
        format!("{baseline}\nSENTINEL_BODY\n{}\n", "x".repeat(1_000_000)),
    )
    .expect("large body");
    let snapshot_after = serde_json::to_vec(&provider.snapshot().expect("root")).expect("JSON");
    assert_eq!(snapshot_before.len(), snapshot_after.len());
    assert!(
        !String::from_utf8(snapshot_after)
            .expect("UTF-8")
            .contains("SENTINEL_BODY")
    );

    let other = root.path().join("projects/demo/investigations/other");
    fs::create_dir_all(other.join("tickets/accepted")).expect("other");
    let unreadable = other.join("tickets/accepted/HMD-999.md");
    fs::write(&unreadable, b"unreadable").expect("unreadable");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).expect("permissions");
    let mut activation = fs::read_to_string(root.path().join("casefile.toml")).expect("activation");
    activation = activation.replace(
        "investigations = [\"projects/demo/investigations/sample\"]",
        "investigations = [\"projects/demo/investigations/sample\", \"projects/demo/investigations/other\"]",
    );
    fs::write(root.path().join("casefile.toml"), activation).expect("activation");
    provider
        .snapshot()
        .expect("root snapshot ignores unreadable record bodies");
    let index = provider
        .query(ProviderQuery::RecordIndex {
            scope: scope.clone(),
        })
        .expect("scoped index ignores other body");
    let ProviderQueryResult::RecordIndex { records, .. } = index else {
        panic!("record index")
    };
    let index_after = serde_json::to_vec(
        &provider
            .query(ProviderQuery::RecordIndex {
                scope: scope.clone(),
            })
            .expect("index after"),
    )
    .expect("index JSON");
    assert_eq!(index_before.len(), index_after.len());
    let encoded = serde_json::to_string(&records).expect("index JSON");
    for forbidden in [
        "SENTINEL_BODY",
        "rendered_markdown",
        "search_text",
        "work_item",
        "original_bytes",
    ] {
        assert!(!encoded.contains(forbidden), "index leaked {forbidden}");
    }

    let missing = provider
        .query(ProviderQuery::RecordDetail {
            identity: InvestigationScopedIdentity {
                scope: scope.clone(),
                identity: "HMD-404".into(),
            },
        })
        .expect("missing detail");
    assert!(matches!(
        missing,
        ProviderQueryResult::RecordDetail { record: None, .. }
    ));
    let duplicate = root
        .path()
        .join(format!("{INVESTIGATION}/tickets/provisional/HMD-011.md"));
    fs::create_dir_all(duplicate.parent().expect("parent")).expect("duplicate parent");
    let duplicate_bytes = fs::read_to_string(&ticket)
        .expect("duplicate identity")
        .replace("status: accepted", "status: provisional");
    fs::write(&duplicate, duplicate_bytes).expect("duplicate identity");
    let ambiguous = provider.query(ProviderQuery::RecordDetail {
        identity: InvestigationScopedIdentity {
            scope,
            identity: "HMD-011".into(),
        },
    });
    assert!(matches!(
        ambiguous,
        Err(ProviderError::AmbiguousRecordIdentity { ref paths }) if paths.len() == 2
    ));
}

#[test]
fn catalogue_union_keeps_mapping_only_and_governed_missing_mapping_projects() {
    let root = fixture();
    fs::write(
        root.path().join("projects.toml"),
        "schema_version = 1\n[projects]\ndemo = '/source/demo'\nmapped = '/source/mapped'\n",
    )
    .expect("project map");
    let mut activation = fs::read_to_string(root.path().join("casefile.toml")).expect("activation");
    activation.push_str(
        "\n[projects.governed]\nprefix = 'GOV'\ninvestigations = ['projects/governed/investigations/one']\n",
    );
    fs::write(root.path().join("casefile.toml"), activation).expect("activation");
    let snapshot = Provider::without_cache(Store::open(root.path()).expect("store"))
        .snapshot()
        .expect("catalogue");
    let mapped = snapshot
        .catalogue
        .projects
        .iter()
        .find(|project| project.name == "mapped")
        .expect("mapping-only");
    assert!(!mapped.governed);
    assert_eq!(mapped.source_root.as_deref(), Some("/source/mapped"));
    let governed = snapshot
        .catalogue
        .projects
        .iter()
        .find(|project| project.name == "governed")
        .expect("governed-only");
    assert!(governed.governed);
    assert!(governed.source_root.is_none());
    assert_eq!(governed.investigations[0].identity, "one");
    assert!(snapshot.diagnostic_coverage.catalogue.count > 0);
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
        assert_eq!(
            snapshot.diagnostic_coverage.catalogue.count > 0,
            activation.is_some()
        );
    }
}

#[test]
fn record_apply_requires_the_complete_provider_preview_and_preserves_store_on_alteration() {
    let root = fixture();
    let provider = Provider::new(Store::open(root.path()).expect("store"), NoCache);
    let (path, draft) = new_ticket(root.path());
    let portable_path = path.replace('/', r"\\") + "///";
    let preview = provider
        .preview_record(ChangeRequest::Create {
            path: portable_path.clone(),
            draft,
        })
        .expect("preview");
    assert_eq!(preview.canonical.request.path(), path);
    assert!(preview.canonical.diff.contains(&format!("b/{path}")));
    #[cfg(unix)]
    assert!(!root.path().join(&portable_path).exists());
    assert!(!preview.approval_required);
    let mut altered = Vec::new();
    let mut value = preview.clone();
    if let ChangeRequest::Create { path, .. } = &mut value.canonical.request {
        *path = portable_path;
    }
    altered.push(value);
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
fn record_preview_rejects_win32_special_components_without_writing() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let (_, draft) = new_ticket(root.path());
    let tickets = root.path().join(INVESTIGATION).join("tickets");

    for path in [
        format!("{INVESTIGATION}/tickets/accepted/.. /HMD-099.md"),
        format!("{INVESTIGATION}/tickets/accepted/item./HMD-099.md"),
        format!("{INVESTIGATION}/tickets/accepted/HMD-099.md:stream"),
        format!("{INVESTIGATION}/tickets/accepted/NUL.txt"),
    ] {
        assert!(matches!(
            provider.preview_record(ChangeRequest::Create {
                path,
                draft: draft.clone(),
            }),
            Err(ProviderError::Store(casefile_store::StoreError::Invalid(_)))
        ));
    }

    assert!(!tickets.join("HMD-099.md").exists());
    assert!(!tickets.join("accepted/item/HMD-099.md").exists());
    assert!(!tickets.join("accepted/HMD-099.md").exists());
    let status = Command::new("git")
        .current_dir(root.path())
        .args(["status", "--porcelain"])
        .output()
        .expect("git status");
    assert!(status.status.success());
    assert!(status.stdout.is_empty(), "unexpected write: {status:?}");
}

#[test]
fn record_batch_promotes_mutually_related_tickets_as_one_valid_change() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let provisional = format!("{INVESTIGATION}/tickets/provisional");
    fs::create_dir_all(root.path().join(&provisional)).expect("provisional directory");
    let draft = |id: &str, status: &str, related: &str| {
        let (_, mut draft) = new_ticket(root.path());
        if let RecordDraft::Ticket(item) = &mut draft {
            item.id = id.into();
            item.title = format!("Mutually related {id}");
            item.status = status.into();
            item.related_tickets = vec![related.into()];
        }
        draft
    };
    for (id, related) in [("HMD-012", "HMD-013"), ("HMD-013", "HMD-012")] {
        let path = format!("{provisional}/{id}.md");
        fs::write(
            root.path().join(&path),
            casefile_core::render_draft(&path, &draft(id, "provisional", related))
                .expect("render provisional ticket"),
        )
        .expect("write provisional ticket");
    }
    assert_eq!(
        provider
            .snapshot()
            .expect("valid provisional Store")
            .diagnostic_coverage
            .catalogue
            .count,
        0
    );

    let requests = ["HMD-012", "HMD-013"]
        .into_iter()
        .flat_map(|id| {
            let related = if id == "HMD-012" {
                "HMD-013"
            } else {
                "HMD-012"
            };
            [
                ChangeRequest::Delete {
                    path: format!("{provisional}/{id}.md"),
                },
                ChangeRequest::Create {
                    path: format!("{INVESTIGATION}/tickets/accepted/{id}.md"),
                    draft: draft(id, "accepted", related),
                },
            ]
        })
        .collect::<Vec<_>>();
    let portable_requests = requests
        .into_iter()
        .map(|request| match request {
            ChangeRequest::Create { path, draft } => ChangeRequest::Create {
                path: path.replace('/', r"\\") + "///",
                draft,
            },
            ChangeRequest::Delete { path } => ChangeRequest::Delete {
                path: path.replace('/', r"\\") + "///",
            },
            ChangeRequest::Replace { .. } => unreachable!(),
        })
        .collect();
    let mut preview = provider
        .preview_record_batch(portable_requests)
        .expect("batch preview");
    assert!(preview.canonical.diagnostics.is_empty());
    assert!(preview.approval_required);
    assert!(
        preview
            .canonical
            .requests
            .iter()
            .all(|request| !request.path().contains('\\') && !request.path().ends_with('/'))
    );
    let stale_path = root.path().join(format!("{provisional}/HMD-012.md"));
    let stale_original = fs::read(&stale_path).expect("batch stale baseline");
    fs::write(&stale_path, [stale_original.as_slice(), b"\n"].concat())
        .expect("batch external edit");
    assert!(matches!(
        provider.apply_record_batch(preview.clone()),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleTargetRevision
        ))
    ));
    fs::write(&stale_path, stale_original).expect("restore batch target");
    preview = provider
        .preview_record_batch(preview.canonical.requests.clone())
        .expect("fresh batch preview");
    provider
        .apply_record_batch(preview)
        .expect("atomic batch promotion");

    for id in ["HMD-012", "HMD-013"] {
        assert!(!root.path().join(format!("{provisional}/{id}.md")).exists());
        assert!(
            root.path()
                .join(format!("{INVESTIGATION}/tickets/accepted/{id}.md"))
                .is_file()
        );
    }
    assert_eq!(
        provider
            .snapshot()
            .expect("valid promoted Store")
            .diagnostic_coverage
            .catalogue
            .count,
        0
    );
}

#[test]
fn progress_preview_integrity_covers_bootstrap_transition_replay_no_op_and_conflict() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let portable_investigation = INVESTIGATION.replace('/', r"\\") + "///";
    let preview = provider
        .bootstrap_progress(&portable_investigation)
        .expect("bootstrap preview");
    assert!(matches!(
        &preview.operation,
        ProgressOperation::Bootstrap { investigation } if investigation == INVESTIGATION
    ));
    assert_eq!(preview.canonical.request.investigation, INVESTIGATION);
    let log = root.path().join(INVESTIGATION).join("progress/log.toml");
    let mut altered = Vec::new();
    let mut value = preview.clone();
    value.operation = ProgressOperation::Bootstrap {
        investigation: portable_investigation,
    };
    altered.push(value);
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
    let baseline_log = fs::read(&log).expect("progress baseline");
    fs::write(&log, [baseline_log.as_slice(), b"\n"].concat()).expect("progress external edit");
    assert!(matches!(
        provider.apply_progress(transition.clone()),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleTargetRevision
        ))
    ));
    fs::write(&log, baseline_log).expect("restore progress");
    let transition = provider
        .preview_progress(operation.clone())
        .expect("fresh transition preview");
    assert!(
        !provider
            .apply_progress(transition.clone())
            .expect("transition apply")
            .result
            .no_op
    );
    assert!(
        provider
            .apply_progress(transition)
            .expect("original exact preview replay")
            .result
            .no_op
    );
    let repreview = provider
        .preview_progress(operation)
        .expect("completed-operation preview");
    assert!(repreview.canonical.no_op);
    assert!(
        provider
            .apply_progress(repreview)
            .expect("completed-operation apply")
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
        .preview_default_delivery_board(INVESTIGATION.replace('/', r"\\") + "///")
        .expect("board preview");
    assert_eq!(preview.investigation, INVESTIGATION);
    assert!(!preview.no_op);
    match &preview.canonical.request {
        ChangeRequest::Create {
            path,
            draft: RecordDraft::Board(board),
        } => {
            assert_eq!(path, &format!("{INVESTIGATION}/boards/delivery.toml"));
            assert_eq!(board.id, "HMD-sample-delivery");
            assert_eq!(board.columns[0].name, "TODO");
            assert_eq!(board.columns[0].statuses, ["unknown"]);
        }
        other => panic!("unexpected default-board request: {other:?}"),
    }
    let board = root.path().join(INVESTIGATION).join("boards/delivery.toml");
    let mut altered = preview.clone();
    altered.investigation = INVESTIGATION.replace('/', r"\\");
    assert!(matches!(
        provider.apply_default_delivery_board(altered),
        Err(ProviderError::PreviewIntegrity)
    ));
    let mut altered = preview.clone();
    altered.investigation.push_str("-altered");
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
    fs::create_dir_all(board.parent().expect("board parent")).expect("board parent");
    fs::write(&board, &preview.rendered_bytes).expect("board appeared after preview");
    assert!(matches!(
        provider.apply_default_delivery_board(preview.clone()),
        Err(ProviderError::Store(
            casefile_store::StoreError::StaleTargetRevision
        ))
    ));
    fs::remove_file(&board).expect("remove stale board");
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

#[test]
fn default_board_refuses_missing_and_ambiguous_activation_mappings_before_preview() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    assert!(matches!(
        provider.preview_default_delivery_board("projects/demo/investigations/missing"),
        Err(ProviderError::DefaultBoardMapping(_))
    ));
    assert!(
        !root
            .path()
            .join("projects/demo/investigations/missing/boards/delivery.toml")
            .exists()
    );

    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"{INVESTIGATION}\", \"{INVESTIGATION}\"]\n"
        ),
    )
    .expect("ambiguous activation mapping");
    assert!(matches!(
        provider.preview_default_delivery_board(INVESTIGATION),
        Err(ProviderError::DefaultBoardMapping(_))
    ));
    assert!(
        !root
            .path()
            .join(INVESTIGATION)
            .join("boards/delivery.toml")
            .exists()
    );
}

#[test]
fn default_board_ignores_unrelated_diagnostics_within_and_across_investigations() {
    let unrelated = fixture();
    let other_investigation = "projects/demo/investigations/other";
    fs::write(
        unrelated.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"{INVESTIGATION}\", \"{other_investigation}\"]\n"
        ),
    )
    .expect("second investigation mapping");
    fs::create_dir_all(unrelated.path().join(other_investigation))
        .expect("other investigation directory");
    fs::write(
        unrelated
            .path()
            .join(other_investigation)
            .join("request.md"),
        "# Request\n",
    )
    .expect("unrelated invalid request");
    let unrelated_store = Store::open(unrelated.path()).expect("store");
    assert!(
        unrelated_store
            .scan()
            .expect("scan")
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == format!("{other_investigation}/request.md"))
    );
    let unrelated_provider = Provider::without_cache(unrelated_store);
    let preview = unrelated_provider
        .preview_default_delivery_board(INVESTIGATION)
        .expect("unrelated diagnostic does not block preview");
    assert!(preview.canonical.diagnostics.is_empty());

    let scoped = fixture();
    fs::write(
        scoped.path().join(INVESTIGATION).join("request.md"),
        "# Request\n",
    )
    .expect("scoped invalid request");
    let scoped_provider = Provider::without_cache(Store::open(scoped.path()).expect("store"));
    let preview = scoped_provider
        .preview_default_delivery_board(INVESTIGATION)
        .expect("scoped diagnostic preview");
    assert!(preview.canonical.diagnostics.is_empty());
    scoped_provider
        .apply_default_delivery_board(preview)
        .expect("unrelated request does not block board");
    assert!(
        scoped
            .path()
            .join(INVESTIGATION)
            .join("boards/delivery.toml")
            .exists()
    );
}

struct FailingCache;
impl ProviderCache for FailingCache {
    fn observe(&self, _: &Revision) -> CacheState {
        CacheState::Degraded {
            message: "injected cache observation failure".into(),
        }
    }
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
    assert!(matches!(
        provider.refresh_full_cache().expect("explicit refresh"),
        CacheState::Degraded { .. }
    ));
}

#[test]
fn every_provider_apply_family_accepts_unrelated_store_changes() {
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
    provider
        .apply_record(record)
        .expect("unrelated record does not invalidate preview");
    assert!(root.path().join(ticket_path).exists());

    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).expect("store"));
    let progress = provider
        .bootstrap_progress(INVESTIGATION)
        .expect("progress preview");
    fs::write(root.path().join("unrelated-progress.txt"), "changed").expect("external change");
    provider
        .apply_progress(progress)
        .expect("unrelated progress does not invalidate preview");
    assert!(
        root.path()
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
    provider
        .apply_default_delivery_board(board)
        .expect("unrelated board does not invalidate preview");
    assert!(
        root.path()
            .join(INVESTIGATION)
            .join("boards/delivery.toml")
            .exists()
    );
}
