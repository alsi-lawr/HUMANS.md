use casefile_core::{Classification, Kind};
use casefile_store::{DerivedIndex, Indexed, RecordScope, ScopedIdentity, Store};
use casefile_store_sqlite::SqliteIndex;
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
