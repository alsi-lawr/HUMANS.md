use casefile_core::{BoardStatusSource, Classification, Kind};
use casefile_store::{DerivedBoard, DerivedIndex, Indexed, RecordScope, ScopedIdentity, Store};
use casefile_store_sqlite::{SqliteIndex, SqliteIndexError};
use std::{fs, path::Path};
use tempfile::TempDir;

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

fn fixture() -> TempDir {
    let root = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../casefile-store/tests/fixtures/minimum")
            .as_path(),
        root.path(),
    );
    root
}

fn current(index: &SqliteIndex, store: &Store) -> casefile_store::DerivedSnapshot {
    let snapshot = store.derived_snapshot().expect("snapshot");
    assert!(matches!(
        index
            .publish(index.prepare(&snapshot).expect("prepare"), store)
            .expect("publish"),
        Indexed::Current { .. }
    ));
    snapshot
}

#[test]
fn old_derived_board_json_defaults_to_disposition() {
    let board: DerivedBoard = serde_json::from_str(
        r#"{
      "identity":{"scope":{"project":"demo","investigation":"sample"},"identity":"HMD-board"},
      "title":"Board","filter_statuses":null,"filter_kinds":null,"columns":[]
    }"#,
    )
    .expect("old board JSON");
    assert_eq!(BoardStatusSource::Disposition, board.status_source);
}

#[test]
fn replacement_index_is_revision_bound_repairable_and_queryable() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/legacy.txt"),
        "legacy",
    )
    .expect("raw");
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/decision-log/HMD-D-200-bad.md"),
        "# broken\n",
    )
    .expect("invalid");
    fs::create_dir_all(root.path().join("projects/demo/decision-log")).expect("project decisions");
    fs::write(
        root.path()
            .join("projects/demo/decision-log/HMD-D-100-project.md"),
        "# HMD-D-100 - Project\n\n## Status\n\naccepted\n\n## Decision\n\nProject scope.\n",
    )
    .expect("project decision");
    let ticket_path = root
        .path()
        .join("projects/demo/investigations/sample/tickets/accepted/HMD-011.md");
    let ticket = fs::read_to_string(&ticket_path)
        .expect("ticket")
        .replace("HMD-D-001", "HMD-D-100");
    fs::write(&ticket_path, &ticket).expect("project decision reference");
    fs::write(
        ticket_path.with_file_name("HMD-012.md"),
        ticket
            .replace("HMD-011", "HMD-012")
            .replace("rank: 1", "rank: 2"),
    )
    .expect("ranked ticket");
    let indexes = TempDir::new().expect("index parent");
    let path = indexes.path().join("casefile.sqlite");
    assert!(SqliteIndex::open(root.path().join("inside.sqlite"), root.path()).is_err());
    let index = SqliteIndex::open(&path, root.path()).expect("external index");
    let first = store.derived_snapshot().expect("snapshot");
    assert!(matches!(
        index.state(&first.source_revision).expect("state"),
        Indexed::Missing
    ));
    let before = store.scan().expect("scan").snapshot.entries;
    let snapshot = current(&index, &store);
    let first_bytes = fs::read(&path).expect("database");
    current(&index, &store);
    assert_eq!(
        first_bytes,
        fs::read(&path).expect("deterministic database")
    );
    assert_eq!(
        before,
        store.scan().expect("canonical unchanged").snapshot.entries
    );
    assert!(!root.path().join("casefile.sqlite").exists());
    assert!(
        snapshot
            .records
            .iter()
            .any(|record| record.kind == Some(Kind::Decision)
                && matches!(record.classification, Classification::Invalid))
    );
    let expected = snapshot
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.path.ends_with("HMD-D-200-bad.md"))
        .expect("snapshot diagnostic");
    let Indexed::Current { value, .. } = index
        .diagnostics(&snapshot.source_revision)
        .expect("diagnostics")
    else {
        panic!("current diagnostics");
    };
    let actual = value
        .iter()
        .find(|diagnostic| diagnostic.path == expected.path)
        .expect("indexed diagnostic");
    assert_eq!(
        (&actual.code, &actual.message, &actual.path),
        (&expected.code, &expected.message, &expected.path)
    );
    assert!(
        snapshot
            .records
            .iter()
            .any(|record| record.path.ends_with("legacy.txt"))
    );
    assert!(snapshot.records.iter().any(|record| {
        record.kind == Some(Kind::Board)
            && record
                .board
                .as_ref()
                .is_some_and(|board| board.id == "HMD-board")
    }));

    let ticket = snapshot
        .records
        .iter()
        .find(|record| record.kind == Some(Kind::Ticket))
        .and_then(|record| record.identity.clone())
        .expect("ticket identity");
    let scope = RecordScope {
        project: "demo".into(),
        investigation: Some("sample".into()),
    };
    assert!(matches!(
        index
            .record(&snapshot.source_revision, &ticket)
            .expect("record"),
        Indexed::Current { value: Some(_), .. }
    ));
    assert!(matches!(
        index
            .record(
                &snapshot.source_revision,
                &ScopedIdentity {
                    scope: RecordScope {
                        project: "other".into(),
                        investigation: Some("sample".into())
                    },
                    identity: ticket.identity.clone()
                }
            )
            .expect("scoped miss"),
        Indexed::Current { value: None, .. }
    ));
    assert!(
        matches!(index.records(&snapshot.source_revision, Some(&scope), Some("minimum")).expect("search"), Indexed::Current { value, .. } if !value.is_empty())
    );
    assert!(
        matches!(index.relationships(&snapshot.source_revision, &ticket).expect("relationships"), Indexed::Current { value, .. } if value.iter().any(|relationship| relationship.target.scope.investigation.is_none()))
    );
    assert!(
        matches!(index.boards(&snapshot.source_revision, &scope).expect("boards"), Indexed::Current { value, .. } if value[0].columns[0].cards.iter().map(|card| card.rank).collect::<Vec<_>>() == vec![Some(1), Some(2)])
    );

    let prepared = index.prepare(&snapshot).expect("prepare old");
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/legacy.txt"),
        "changed",
    )
    .expect("canonical change");
    let published = index.publish(prepared, &store).expect("stale publish");
    let changed = store.derived_snapshot().expect("changed snapshot");
    assert!(
        matches!(published, Indexed::Stale { indexed_revision, current_revision } if indexed_revision == snapshot.source_revision && current_revision == changed.source_revision)
    );
    assert_eq!(first_bytes, fs::read(&path).expect("atomic replacement"));
    assert!(matches!(
        index
            .records(&changed.source_revision, None, None)
            .expect("stale read"),
        Indexed::Stale { .. }
    ));

    fs::remove_file(&path).expect("delete index");
    assert!(matches!(
        index
            .state(&changed.source_revision)
            .expect("missing state"),
        Indexed::Missing
    ));
    current(&index, &store);
    assert_eq!(
        before.len(),
        store
            .scan()
            .expect("repair preserves canonical")
            .snapshot
            .entries
            .len()
    );
}

#[test]
fn strategy_binding_projection_survives_json_index_round_trip() {
    let root = fixture();
    let base = root
        .path()
        .join("projects/demo/investigations/sample/strategy");
    fs::write(
        base.join("implementation.toml"),
        r#"schema_version = 1
strategy_id = "casefile-implement-ticket-batch"
phase = "implementation"
adapter = "codex"
[orchestrator]
binding = "root"
[limits]
max_concurrent_subagents = 1
max_depth = 1
[requirements]
capabilities = ["subagents"]
[[workers]]
role = "implementation-writer"
platform_profile = "writer"
model = "gpt-5.6-sol"
reasoning = "high"
minimum_count = 1
maximum_count = 1
can_spawn_subagents = false
[coordination]
batch_when_capacity_exceeded = true
candidate_review_before_ticket = false
shared_ticket_storage_required = true
"#,
    )
    .expect("matrix");
    fs::write(
        base.join("bindings.toml"),
        r#"schema_version = 1
adapter = "codex"
role = "implementation-writer"
model = "gpt-5.6-terra"
reasoning_effort = "high"
[resolution]
mode = "profile"
value = "writer"
"#,
    )
    .expect("binding");
    let store = Store::open(root.path()).expect("store");
    let indexes = TempDir::new().expect("indexes");
    let index =
        SqliteIndex::open(indexes.path().join("casefile.sqlite"), root.path()).expect("index");
    let snapshot = current(&index, &store);
    let Indexed::Current { value, .. } = index
        .records(&snapshot.source_revision, None, None)
        .expect("records")
    else {
        panic!("current");
    };
    let implementation = value
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("strategy projection");
    assert!(matches!(
        implementation.binding,
        Some(casefile_store::StrategyBindingState::Resolved { .. })
    ));
    assert!(
        value
            .iter()
            .any(|record| record.path.ends_with("strategy/bindings.toml")
                && record.strategy_binding.is_some())
    );
}

#[test]
fn compact_cache_round_trip_preserves_source_search_and_requested_rendering() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let external = TempDir::new().unwrap();
    let index = SqliteIndex::open(external.path().join("index.sqlite"), root.path()).unwrap();
    let snapshot = current(&index, &store);
    let record = snapshot
        .records
        .iter()
        .find(|record| record.path.ends_with("tickets/accepted/HMD-011.md"))
        .unwrap();
    let identity = record.identity.as_ref().unwrap();
    let Indexed::Current {
        value: Some(cached),
        ..
    } = index.record(&snapshot.source_revision, identity).unwrap()
    else {
        panic!("cached ticket")
    };
    assert_eq!(&cached, record);
    assert_eq!(cached.rendered_markdown(), record.rendered_markdown());
    let text = record.content.as_ref().unwrap();
    let RecordScope {
        project,
        investigation,
    } = identity.scope.clone();
    let Indexed::Current { value, .. } = index
        .records(
            &snapshot.source_revision,
            Some(&RecordScope {
                project,
                investigation,
            }),
            Some("rEqUiReD"),
        )
        .unwrap()
    else {
        panic!("searched records")
    };
    assert!(text.to_lowercase().contains("required"));
    assert!(value.iter().any(|entry| entry.path == cached.path));
    let Indexed::Current { value, .. } = index
        .records(
            &snapshot.source_revision,
            None,
            Some("no-such-body-fragment"),
        )
        .unwrap()
    else {
        panic!("searched records")
    };
    assert!(value.is_empty());
}

#[test]
fn pushed_scope_and_search_match_rust_unicode_substrings_in_path_order() {
    let root = fixture();
    fs::write(
        root.path().join("unscoped.md"),
        "İSTANBUL ΟΣ Σ Straße café cafe\u{301} 100% foo_bar left\0right",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("projects/demo/decision-log")).unwrap();
    fs::write(root.path().join("projects/demo/decision-log/HMD-D-700-project.md"),
        "# HMD-D-700 - Project Unicode\n\n## Status\n\naccepted\n\n## Decision\n\nİSTANBUL Straße project-only.\n").unwrap();
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/Unicode.md"),
        "İSTANBUL ΟΣ investigation-only 100% foo_bar left\0right",
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let external = TempDir::new().unwrap();
    let index = SqliteIndex::open(external.path().join("index.sqlite"), root.path()).unwrap();
    let mut snapshot = store.derived_snapshot().unwrap();
    snapshot.records.reverse();
    assert!(matches!(
        index
            .publish(index.prepare(&snapshot).unwrap(), &store)
            .unwrap(),
        Indexed::Current { .. }
    ));
    let scopes = [
        None,
        Some(RecordScope {
            project: "demo".into(),
            investigation: None,
        }),
        Some(RecordScope {
            project: "demo".into(),
            investigation: Some("sample".into()),
        }),
        Some(RecordScope {
            project: "other".into(),
            investigation: Some("sample".into()),
        }),
    ];
    for scope in &scopes {
        for needle in [
            None,
            Some(""),
            Some("İ"),
            Some("i\u{307}"),
            Some("ΟΣ"),
            Some("οσ"),
            Some("Σ"),
            Some("straße"),
            Some("STRASSE"),
            Some("café"),
            Some("cafe\u{301}"),
            Some("%"),
            Some("_"),
            Some("left\0right"),
            Some("missing"),
        ] {
            let mut expected = snapshot
                .records
                .iter()
                .filter(|record| {
                    scope
                        .as_ref()
                        .is_none_or(|scope| record.scope.as_ref() == Some(scope))
                        && needle.is_none_or(|needle| {
                            record
                                .search_text()
                                .to_lowercase()
                                .contains(&needle.to_lowercase())
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            expected.sort_by(|a, b| a.path.cmp(&b.path));
            let Indexed::Current { value, .. } = index
                .records(&snapshot.source_revision, scope.as_ref(), needle)
                .unwrap()
            else {
                panic!("current search");
            };
            assert_eq!(value, expected, "scope {scope:?}, needle {needle:?}");
        }
    }
    let null_scope = RecordScope {
        project: "demo".into(),
        investigation: None,
    };
    let Indexed::Current { value, .. } = index
        .records(
            &snapshot.source_revision,
            Some(&null_scope),
            Some("project-only"),
        )
        .unwrap()
    else {
        panic!("project records");
    };
    assert_eq!(value.len(), 1);
    assert!(value[0].path.ends_with("HMD-D-700-project.md"));
    let Indexed::Current {
        value: Some(project_decision),
        ..
    } = index
        .record(
            &snapshot.source_revision,
            value[0].identity.as_ref().unwrap(),
        )
        .unwrap()
    else {
        panic!("project identity");
    };
    assert_eq!(project_decision, value[0]);
}

#[test]
fn exact_duplicate_identity_is_ambiguous_in_canonical_and_indexed_reads() {
    use casefile_store::{
        InvestigationScope, InvestigationScopedIdentity, Provider, ProviderError, ProviderQuery,
    };
    let root = fixture();
    let original = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    let duplicate = "projects/demo/investigations/sample/tickets/provisional/HMD-011.md";
    fs::create_dir_all(
        root.path()
            .join("projects/demo/investigations/sample/tickets/provisional"),
    )
    .unwrap();
    fs::write(
        root.path().join(duplicate),
        fs::read_to_string(root.path().join(original))
            .unwrap()
            .replace("status: accepted", "status: provisional"),
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let provider = Provider::without_cache(store.clone());
    assert!(
        matches!(provider.query(ProviderQuery::RecordDetail { identity: InvestigationScopedIdentity {
        scope: InvestigationScope { project: "demo".into(), investigation: "sample".into() }, identity: "HMD-011".into()
    } }), Err(ProviderError::AmbiguousRecordIdentity { paths }) if paths == [original, duplicate])
    );
    let external = TempDir::new().unwrap();
    let index = SqliteIndex::open(external.path().join("index.sqlite"), root.path()).unwrap();
    let snapshot = current(&index, &store);
    let identity = ScopedIdentity {
        scope: RecordScope {
            project: "demo".into(),
            investigation: Some("sample".into()),
        },
        identity: "HMD-011".into(),
    };
    assert!(
        matches!(index.record(&snapshot.source_revision, &identity), Err(SqliteIndexError::AmbiguousRecordIdentity { paths }) if paths == [original, duplicate])
    );
    let other_scope = ScopedIdentity {
        scope: RecordScope {
            project: "demo".into(),
            investigation: None,
        },
        identity: "HMD-011".into(),
    };
    assert!(matches!(
        index
            .record(&snapshot.source_revision, &other_scope)
            .unwrap(),
        Indexed::Current { value: None, .. }
    ));
}

#[test]
fn relationship_endpoints_and_board_scopes_preserve_nullable_identity_and_order() {
    let root = fixture();
    fs::create_dir_all(root.path().join("projects/demo/decision-log")).unwrap();
    fs::write(
        root.path()
            .join("projects/demo/decision-log/HMD-D-700-project.md"),
        "# HMD-D-700 - Project\n\n## Status\n\naccepted\n\n## Decision\n\nProject.\n",
    )
    .unwrap();
    let path = root
        .path()
        .join("projects/demo/investigations/sample/tickets/accepted/HMD-011.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("HMD-D-001", "HMD-D-700"),
    )
    .unwrap();
    let boards = root
        .path()
        .join("projects/demo/investigations/sample/boards");
    let board = fs::read_to_string(boards.join("main.toml")).unwrap();
    fs::write(boards.join("a.toml"), board.replace("HMD-board", "HMD-z")).unwrap();
    fs::write(boards.join("z.toml"), board.replace("HMD-board", "HMD-a")).unwrap();
    let store = Store::open(root.path()).unwrap();
    let external = TempDir::new().unwrap();
    let index = SqliteIndex::open(external.path().join("index.sqlite"), root.path()).unwrap();
    let snapshot = current(&index, &store);
    let project_decision = ScopedIdentity {
        scope: RecordScope {
            project: "demo".into(),
            investigation: None,
        },
        identity: "HMD-D-700".into(),
    };
    let ticket = ScopedIdentity {
        scope: RecordScope {
            project: "demo".into(),
            investigation: Some("sample".into()),
        },
        identity: "HMD-011".into(),
    };
    for identity in [&project_decision, &ticket] {
        let mut expected = snapshot
            .relationships
            .iter()
            .filter(|r| &r.source == identity || &r.target == identity)
            .cloned()
            .collect::<Vec<_>>();
        expected.sort_by_key(|r| {
            (
                format!("{:?}", r.kind),
                r.source.identity.clone(),
                r.target.identity.clone(),
            )
        });
        let Indexed::Current { value, .. } = index
            .relationships(&snapshot.source_revision, identity)
            .unwrap()
        else {
            panic!("relationships");
        };
        assert_eq!(value, expected);
        assert!(
            value
                .iter()
                .any(|r| r.source == ticket && r.target == project_decision)
        );
    }
    let Indexed::Current { value, .. } = index
        .relationships(
            &snapshot.source_revision,
            &ScopedIdentity {
                scope: ticket.scope.clone(),
                identity: project_decision.identity.clone(),
            },
        )
        .unwrap()
    else {
        panic!("nullable miss");
    };
    assert!(value.is_empty());
    for scope in [&ticket.scope, &project_decision.scope] {
        let mut expected = snapshot
            .boards
            .iter()
            .filter(|b| &b.identity.scope == scope)
            .cloned()
            .collect::<Vec<_>>();
        expected.sort_by(|a, b| a.identity.identity.cmp(&b.identity.identity));
        let Indexed::Current { value, .. } =
            index.boards(&snapshot.source_revision, scope).unwrap()
        else {
            panic!("boards");
        };
        assert_eq!(value, expected);
    }
}

#[test]
fn failed_replacement_keeps_published_index_and_prior_schema_rebuilds_as_missing() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let external = TempDir::new().unwrap();
    let path = external.path().join("index.sqlite");
    let index = SqliteIndex::open(&path, root.path()).unwrap();
    let snapshot = current(&index, &store);
    let original = fs::read(&path).unwrap();
    let mut duplicate_path = snapshot.clone();
    duplicate_path
        .records
        .push(duplicate_path.records[0].clone());
    assert!(matches!(
        index.prepare(&duplicate_path),
        Err(SqliteIndexError::Sql(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(
        matches!(index.records(&snapshot.source_revision, None, None).unwrap(), Indexed::Current { value, .. } if value == snapshot.records)
    );

    let old_path = external.path().join("old.sqlite");
    let old = rusqlite::Connection::open(&old_path).unwrap();
    old.execute_batch("CREATE TABLE metadata (source_revision TEXT NOT NULL); CREATE TABLE records (path TEXT PRIMARY KEY, project TEXT, investigation TEXT, identity TEXT, classification TEXT NOT NULL, kind TEXT, title TEXT NOT NULL, document TEXT NOT NULL);").unwrap();
    old.execute(
        "INSERT INTO metadata VALUES (?)",
        [&snapshot.source_revision.0],
    )
    .unwrap();
    drop(old);
    let bytes_before = fs::read(&old_path).unwrap();
    let old_index = SqliteIndex::open(&old_path, root.path()).unwrap();
    assert!(matches!(
        old_index.state(&snapshot.source_revision).unwrap(),
        Indexed::Missing
    ));
    assert!(matches!(
        old_index
            .records(&snapshot.source_revision, None, Some("minimum"))
            .unwrap(),
        Indexed::Missing
    ));
    assert_eq!(fs::read(&old_path).unwrap(), bytes_before);
    let provider = casefile_store::Provider::new(store, old_index);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if matches!(
            provider.refresh_full_cache().unwrap(),
            casefile_store::CacheState::Current { .. }
        ) {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_ne!(fs::read(&old_path).unwrap(), bytes_before);
}
