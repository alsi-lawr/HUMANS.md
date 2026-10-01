use casefile_core::{Kind, ProgressEntry, ProgressLog, ProgressStatus, RecordDraft};
use casefile_store::{InvestigationScope, Provider, ProviderQuery, ProviderQueryResult, Store};
use std::{fs, path::Path};
use tempfile::TempDir;

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn full_and_scoped_boards_share_scope_filters_rank_and_duplicate_card_semantics() {
    let root = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
        root.path(),
    );
    let base = "projects/demo/investigations/sample";
    let path = format!("{base}/tickets/accepted/HMD-011.md");
    let source = fs::read_to_string(root.path().join(&path)).unwrap();
    let RecordDraft::Ticket(mut item) =
        casefile_core::parse_draft(&path, Kind::Ticket, &source).unwrap()
    else {
        panic!("ticket")
    };
    for (id, rank) in [
        ("HMD-011", Some(3)),
        ("HMD-012", Some(1)),
        ("HMD-013", None),
    ] {
        item.id = id.into();
        item.rank = rank;
        fs::write(
            root.path().join(format!("{base}/tickets/accepted/{id}.md")),
            casefile_core::render_draft(
                &format!("{base}/tickets/accepted/{id}.md"),
                &RecordDraft::Ticket(item.clone()),
            )
            .unwrap(),
        )
        .unwrap();
    }
    // Cross-disposition duplicate identities remain separate cards, not silently deduplicated.
    fs::create_dir_all(root.path().join(format!("{base}/tickets/rejected"))).unwrap();
    item.id = "HMD-012".into();
    item.status = "rejected".into();
    item.rank = Some(1);
    fs::write(
        root.path()
            .join(format!("{base}/tickets/rejected/HMD-012.md")),
        casefile_core::render_draft(
            &format!("{base}/tickets/rejected/HMD-012.md"),
            &RecordDraft::Ticket(item),
        )
        .unwrap(),
    )
    .unwrap();
    let disposition = "schema_version=1\nid='HMD-mixed'\ntitle='Mixed'\nfilter_kinds=['ticket']\n[[columns]]\nname='All'\nstatuses=['accepted','rejected']\n[[columns]]\nname='Other'\nstatuses=['provisional']\n";
    fs::write(
        root.path().join(format!("{base}/boards/mixed.toml")),
        disposition,
    )
    .unwrap();
    fs::write(root.path().join(format!("{base}/boards/progress.toml")), "schema_version=1\nid='HMD-progress'\ntitle='Progress'\nstatus_source='progress'\nfilter_statuses=['in_progress','unknown']\n[[columns]]\nname='Active'\nstatuses=['in_progress','unknown']\n").unwrap();
    fs::create_dir_all(root.path().join(format!("{base}/progress"))).unwrap();
    let log = ProgressLog {
        entries: vec![ProgressEntry::Transition {
            id: "start".into(),
            recorded_at: "2026-09-30T10:00:00Z".into(),
            recorded_by: "root".into(),
            ticket_id: "HMD-011".into(),
            from: ProgressStatus::Unknown,
            to: ProgressStatus::InProgress,
        }],
    };
    fs::write(
        root.path().join(format!("{base}/progress/log.toml")),
        casefile_core::render_progress_log(&log),
    )
    .unwrap();
    let other = "projects/demo/investigations/other";
    fs::create_dir_all(root.path().join(format!("{other}/tickets/accepted"))).unwrap();
    fs::create_dir_all(root.path().join(format!("{other}/boards"))).unwrap();
    fs::write(
        root.path()
            .join(format!("{other}/tickets/accepted/HMD-777.md")),
        source
            .replace("HMD-011", "HMD-777")
            .replace("investigation: \"sample\"", "investigation: \"other\""),
    )
    .unwrap();
    fs::write(root.path().join(format!("{other}/boards/main.toml")), "schema_version=1\nid='HMD-foreign'\ntitle='Other scope'\n[[columns]]\nname='Accepted'\nstatuses=['accepted']\n").unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version=1\n[projects.demo]\nprefix='HMD'\ninvestigations=['{base}','{other}']\n"
        ),
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let snapshot = store.derived_snapshot().unwrap();
    let ProviderQueryResult::Boards { boards, .. } = Provider::without_cache(store.clone())
        .query(ProviderQuery::Boards {
            scope: InvestigationScope {
                project: "demo".into(),
                investigation: "sample".into(),
            },
        })
        .unwrap()
    else {
        panic!("boards")
    };
    assert_eq!(
        snapshot
            .boards
            .iter()
            .filter(|board| board.identity.scope.investigation.as_deref() == Some("sample"))
            .cloned()
            .collect::<Vec<_>>(),
        boards
    );
    let foreign = snapshot
        .boards
        .iter()
        .find(|board| board.identity.identity == "HMD-foreign")
        .unwrap();
    assert_eq!(
        foreign.columns[0]
            .cards
            .iter()
            .map(|card| card.identity.identity.as_str())
            .collect::<Vec<_>>(),
        ["HMD-777"]
    );
    let mixed = boards
        .iter()
        .find(|board| board.identity.identity == "HMD-mixed")
        .unwrap();
    assert_eq!(
        mixed.columns[0]
            .cards
            .iter()
            .map(|card| (card.identity.identity.as_str(), card.status.as_str()))
            .collect::<Vec<_>>(),
        [
            ("HMD-012", "accepted"),
            ("HMD-012", "rejected"),
            ("HMD-011", "accepted"),
            ("HMD-013", "accepted")
        ]
    );
    assert!(mixed.columns[1].cards.is_empty());
    let progress = boards
        .iter()
        .find(|board| board.identity.identity == "HMD-progress")
        .unwrap();
    assert_eq!(progress.columns[0].cards.len(), 3);
    assert!(
        progress.columns[0]
            .cards
            .iter()
            .all(|card| card.kind == Kind::Ticket)
    );
    let identity = casefile_store::InvestigationScopedIdentity {
        scope: InvestigationScope {
            project: "demo".into(),
            investigation: "sample".into(),
        },
        identity: "HMD-012".into(),
    };
    assert!(matches!(
        Provider::without_cache(store).query(ProviderQuery::RecordDetail { identity }),
        Err(casefile_store::ProviderError::AmbiguousRecordIdentity { .. })
    ));
}

#[test]
fn display_accessors_preserve_requested_source_and_rendering_without_duplicate_draft_sections() {
    let root = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
        root.path(),
    );
    let store = Store::open(root.path()).unwrap();
    let scan = store.scan().unwrap();
    let direct = store.derived_snapshot().unwrap();
    assert_eq!(direct, store.derive_snapshot(&scan));
    let record = direct
        .records
        .iter()
        .find(|record| record.path.ends_with("tickets/accepted/HMD-011.md"))
        .unwrap();
    let source = fs::read_to_string(root.path().join(&record.path)).unwrap();
    assert_eq!(record.content.as_deref(), Some(source.as_str()));
    assert_eq!(record.search_text(), format!("{}\n{source}", record.title));
    assert_eq!(
        record.rendered_markdown().unwrap(),
        casefile_core::render_markdown_html(&source)
    );
    let draft = casefile_core::parse_draft(&record.path, Kind::Ticket, &source).unwrap();
    let RecordDraft::Ticket(item) = draft else {
        panic!("ticket")
    };
    assert_eq!(
        record.work_item.as_ref().unwrap().decision_refs,
        item.decision_refs
    );
}
