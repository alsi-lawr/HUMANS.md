use casefile_core::{
    BoardColumn, BoardDraft, BoardStatusSource, ChangeRequest, Classification, Kind, ProgressEntry,
    ProgressStatus, RecordDraft,
};
use casefile_store::{ActivationState, ProgressChangeRequest, RelationshipKind, Store};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn fixture() -> TempDir {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
    command(temporary.path(), ["init", "-q"]);
    command(
        temporary.path(),
        ["config", "user.email", "casefile@example.test"],
    );
    command(temporary.path(), ["config", "user.name", "Casefile Test"]);
    command(temporary.path(), ["add", "."]);
    command(temporary.path(), ["commit", "-qm", "fixture"]);
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
fn command(root: &Path, args: impl IntoIterator<Item = &'static str>) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success());
}
fn ticket(root: &Path) -> RecordDraft {
    let path = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    let text = fs::read_to_string(root.join(path)).expect("ticket");
    casefile_core::parse_draft(path, Kind::Ticket, &text).expect("draft")
}
fn path(root: &Path, name: &str) -> std::path::PathBuf {
    if matches!(name, "casefile.toml" | "projects.toml") {
        root.join(name)
    } else {
        root.join("projects/demo/investigations/sample").join(name)
    }
}
fn scan_has(root: &Path, code: &str) {
    assert!(
        Store::open(root)
            .expect("store")
            .scan()
            .expect("scan")
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code),
        "missing {code}"
    );
}

#[test]
fn scans_each_v1_kind_and_preserves_raw_material() {
    let root = fixture();
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/legacy.txt"),
        "legacy",
    )
    .expect("legacy");
    let result = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    for kind in [
        Kind::Activation,
        Kind::ProjectMap,
        Kind::Request,
        Kind::Decision,
        Kind::Evidence,
        Kind::Review,
        Kind::Plan,
        Kind::Closeout,
        Kind::Strategy,
        Kind::Ticket,
        Kind::Epic,
        Kind::Board,
    ] {
        assert!(
            result
                .snapshot
                .entries
                .iter()
                .any(|entry| entry.kind == Some(kind)
                    && entry.classification == Classification::Governed),
            "missing {kind:?}"
        );
    }
    assert_eq!(
        Some(Classification::Raw),
        result
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path.ends_with("legacy.txt"))
            .map(|entry| entry.classification)
    );
}

#[test]
fn accepted_tickets_project_unknown_and_a_valid_log_folds_progress_notes_and_progress_boards() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let initial = store.derived_snapshot().expect("derived");
    let ticket = initial
        .records
        .iter()
        .find(|record| record.path.ends_with("HMD-011.md"))
        .expect("ticket");
    assert_eq!(
        ticket.progress.as_ref().expect("unknown progress").status,
        ProgressStatus::Unknown
    );

    let log = "schema_version = 1\n\n[[entries]]\nid = \"start\"\nrecorded_at = \"2026-07-26T10:00:00Z\"\nrecorded_by = \"root\"\nticket_id = \"HMD-011\"\nkind = \"transition\"\nfrom = \"unknown\"\nto = \"in_progress\"\n\n[[entries]]\nid = \"quirk\"\nrecorded_at = \"2026-07-26T10:01:00Z\"\nrecorded_by = \"root\"\nticket_id = \"HMD-011\"\nkind = \"note\"\ncategory = \"quirk\"\nmessage = \"Fixture note.\"\n";
    fs::create_dir_all(
        root.path()
            .join("projects/demo/investigations/sample/progress"),
    )
    .expect("progress directory");
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/progress/log.toml"),
        log,
    )
    .expect("log");
    fs::write(root.path().join("projects/demo/investigations/sample/boards/main.toml"), "schema_version = 1\nid = \"HMD-board\"\ntitle = \"Main\"\nstatus_source = \"progress\"\nfilter_statuses = [\"in_progress\"]\nfilter_kinds = [\"ticket\"]\n\n[[columns]]\nname = \"Working\"\nstatuses = [\"in_progress\"]\n").expect("board");
    let derived = store.derived_snapshot().expect("derived");
    let ticket = derived
        .records
        .iter()
        .find(|record| record.path.ends_with("HMD-011.md"))
        .expect("ticket");
    let progress = ticket.progress.as_ref().expect("progress");
    assert_eq!(progress.status, ProgressStatus::InProgress);
    assert_eq!(progress.notes.len(), 1);
    assert_eq!(derived.boards[0].columns[0].cards[0].status, "in_progress");
}

#[test]
fn malformed_and_cross_scope_progress_never_become_unknown_and_store_writer_is_preview_first_and_idempotent()
 {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    fs::create_dir_all(
        root.path()
            .join("projects/demo/investigations/sample/progress"),
    )
    .expect("progress directory");
    let log_path = root
        .path()
        .join("projects/demo/investigations/sample/progress/log.toml");
    fs::write(&log_path, "schema_version = 1\n[[entries]]\nid = \"bad\"\n").expect("bad log");
    let invalid = store.derived_snapshot().expect("derived");
    let ticket = invalid
        .records
        .iter()
        .find(|record| record.path.ends_with("HMD-011.md"))
        .expect("ticket");
    assert!(ticket.progress.is_none());
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_progress_log")
    );

    let repair = store
        .preview_progress(ProgressChangeRequest {
            investigation: "projects/demo/investigations/sample".into(),
            entries: Vec::new(),
            replacement: None,
            replacement_source: Some("schema_version = 1\n".into()),
            bootstrap: false,
        })
        .expect("repair preview");
    assert!(repair.diagnostics.is_empty(), "{:#?}", repair.diagnostics);
    store.apply_progress(repair).expect("repair malformed log");
    let request = ProgressChangeRequest {
        investigation: "projects/demo/investigations/sample".into(),
        entries: vec![ProgressEntry::Transition {
            id: "start-001".into(),
            recorded_at: "2026-07-26T10:00:00Z".into(),
            recorded_by: "root".into(),
            ticket_id: "HMD-011".into(),
            from: ProgressStatus::Unknown,
            to: ProgressStatus::InProgress,
        }],
        replacement: None,
        replacement_source: None,
        bootstrap: false,
    };
    let preview = store.preview_progress(request.clone()).expect("preview");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
    assert_eq!(
        "schema_version = 1\n",
        fs::read_to_string(&log_path).expect("pre-preview log"),
        "preview must not mutate"
    );
    let applied = store.apply_progress(preview).expect("apply");
    assert!(!applied.no_op);
    let retry = store.preview_progress(request).expect("retry preview");
    assert!(retry.diagnostics.is_empty());
    assert!(retry.no_op);
    assert!(store.apply_progress(retry).expect("retry apply").no_op);
    let conflict = store
        .preview_progress(ProgressChangeRequest {
            investigation: "projects/demo/investigations/sample".into(),
            entries: vec![ProgressEntry::Transition {
                id: "start-001".into(),
                recorded_at: "2026-07-26T10:00:00Z".into(),
                recorded_by: "root".into(),
                ticket_id: "HMD-011".into(),
                from: ProgressStatus::InProgress,
                to: ProgressStatus::Complete,
            }],
            replacement: None,
            replacement_source: None,
            bootstrap: false,
        })
        .expect("conflict preview");
    assert!(
        conflict
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "conflicting_progress_operation_id")
    );
}

#[test]
fn bootstrap_creates_only_an_absent_empty_log_and_existing_valid_log_is_byte_preserving_no_op() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let investigation = "projects/demo/investigations/sample";
    let log_path = root
        .path()
        .join("projects/demo/investigations/sample/progress/log.toml");

    let absent = store
        .preview_progress(
            store
                .bootstrap_progress(investigation)
                .expect("bootstrap request"),
        )
        .expect("absent bootstrap preview");
    assert!(absent.diagnostics.is_empty(), "{:#?}", absent.diagnostics);
    assert!(!absent.no_op);
    assert_eq!(absent.bootstrap_ticket_ids, ["HMD-011"]);
    store
        .apply_progress(absent)
        .expect("absent bootstrap apply");
    assert_eq!(
        fs::read_to_string(&log_path).expect("empty log"),
        "schema_version = 1\n"
    );

    let noncanonical =
        "schema_version=1\n\n# Preserve this comment and whitespace.\nentries = []\n";
    fs::write(&log_path, noncanonical).expect("noncanonical valid log");
    let before = store.scan().expect("before scan").snapshot.revision;
    let existing = store
        .preview_progress(
            store
                .bootstrap_progress(investigation)
                .expect("bootstrap request"),
        )
        .expect("existing bootstrap preview");
    assert!(
        existing.diagnostics.is_empty(),
        "{:#?}",
        existing.diagnostics
    );
    assert!(existing.no_op);
    assert!(existing.diff.is_empty());
    assert!(existing.bootstrap_ticket_ids.is_empty());
    assert_eq!(existing.expected_store_revision, before);
    let applied = store
        .apply_progress(existing)
        .expect("existing bootstrap apply");
    assert!(applied.no_op);
    assert_eq!(
        fs::read_to_string(&log_path).expect("preserved log"),
        noncanonical
    );
    assert_eq!(applied.resulting_store_revision, before);
}

#[test]
fn progress_mutations_require_active_activation_but_ignore_unrelated_diagnostics() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let request = ProgressChangeRequest {
        investigation: "projects/demo/investigations/sample".into(),
        entries: vec![ProgressEntry::Transition {
            id: "start-activation".into(),
            recorded_at: "2026-07-26T10:00:00Z".into(),
            recorded_by: "root".into(),
            ticket_id: "HMD-011".into(),
            from: ProgressStatus::Unknown,
            to: ProgressStatus::InProgress,
        }],
        replacement: None,
        replacement_source: None,
        bootstrap: false,
    };

    fs::write(root.path().join("casefile.toml"), "schema_version = 2\n")
        .expect("invalid activation");
    assert!(matches!(
        store.preview_progress(request.clone()),
        Err(casefile_store::StoreError::Invalid(message)) if message.contains("active Casefile activation")
    ));

    let apply_root = fixture();
    let apply_store = Store::open(apply_root.path()).expect("apply store");
    let preview = apply_store
        .preview_progress(request.clone())
        .expect("active preview");
    fs::write(
        apply_root.path().join("casefile.toml"),
        "schema_version = 2\n",
    )
    .expect("invalidate activation before apply");
    assert!(matches!(
        apply_store.apply_progress(preview),
        Err(casefile_store::StoreError::Invalid(message)) if message.contains("active Casefile activation")
    ));

    let active = fixture();
    let store = Store::open(active.path()).expect("active store");
    fs::write(active.path().join("request.md"), "# Request\n").expect("unrelated diagnostic");
    let preview = store
        .preview_progress(request)
        .expect("unrelated diagnostic preview");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
}

#[test]
fn semantic_invalid_progress_suppresses_unknown_and_progress_board_cards() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let progress = root
        .path()
        .join("projects/demo/investigations/sample/progress/log.toml");
    fs::create_dir_all(progress.parent().expect("progress parent")).expect("progress directory");
    fs::write(
        &progress,
        "schema_version = 1\n\n[[entries]]\nid = \"wrong-ticket\"\nrecorded_at = \"2026-07-26T10:00:00Z\"\nrecorded_by = \"root\"\nticket_id = \"HMD-999\"\nkind = \"transition\"\nfrom = \"unknown\"\nto = \"in_progress\"\n",
    )
    .expect("cross-scope log");
    fs::write(
        root.path().join("projects/demo/investigations/sample/boards/main.toml"),
        "schema_version = 1\nid = \"HMD-board\"\ntitle = \"Progress\"\nstatus_source = \"progress\"\nfilter_kinds = [\"ticket\"]\n\n[[columns]]\nname = \"Unknown\"\nstatuses = [\"unknown\"]\n",
    )
    .expect("progress board");

    let derived = store.derived_snapshot().expect("derived");
    assert!(
        derived
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_progress_ticket")
    );
    let ticket = derived
        .records
        .iter()
        .find(|record| record.path.ends_with("HMD-011.md"))
        .expect("ticket");
    assert!(ticket.progress.is_none());
    assert!(derived.boards[0].columns[0].cards.is_empty());
}

#[cfg(unix)]
#[test]
fn progress_apply_refuses_unsafe_paths_without_replacing_existing_bytes() {
    use std::os::unix::fs::symlink;

    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let request = ProgressChangeRequest {
        investigation: "projects/demo/investigations/sample".into(),
        entries: Vec::new(),
        replacement: None,
        replacement_source: Some("schema_version = 1\n".into()),
        bootstrap: false,
    };
    let mut preview = store.preview_progress(request).expect("preview");
    let progress = root
        .path()
        .join("projects/demo/investigations/sample/progress");
    fs::create_dir_all(&progress).expect("progress parent");
    let outside = root.path().join("outside-progress.toml");
    fs::write(&outside, "outside bytes\n").expect("outside bytes");
    symlink(&outside, progress.join("log.toml")).expect("unsafe link");

    // This models a caller that refreshes its revision after detecting an out-of-band path change:
    // the Store must still reject the unsafe target at the single atomic writer boundary.
    let current = store.scan().expect("updated scan").snapshot;
    preview.expected_store_revision = current.revision;
    preview.expected_target_revision = current
        .entries
        .iter()
        .find(|entry| entry.path.ends_with("progress/log.toml"))
        .map(|entry| entry.content_revision.clone());
    let error = store
        .apply_progress(preview)
        .expect_err("unsafe target refused");
    assert!(
        matches!(
            &error,
            casefile_store::StoreError::Invalid(message) if message.contains("regular non-symlink")
        ),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(&outside).expect("outside preserved"),
        "outside bytes\n"
    );
}

#[cfg(unix)]
#[test]
fn atomic_progress_write_failure_preserves_the_previous_log() {
    use std::os::unix::fs::PermissionsExt;

    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let progress = root
        .path()
        .join("projects/demo/investigations/sample/progress");
    fs::create_dir_all(&progress).expect("progress directory");
    let log = progress.join("log.toml");
    let original = "schema_version = 1\n";
    fs::write(&log, original).expect("original log");
    let preview = store
        .preview_progress(ProgressChangeRequest {
            investigation: "projects/demo/investigations/sample".into(),
            entries: vec![ProgressEntry::Transition {
                id: "cannot-write".into(),
                recorded_at: "2026-07-26T10:00:00Z".into(),
                recorded_by: "root".into(),
                ticket_id: "HMD-011".into(),
                from: ProgressStatus::Unknown,
                to: ProgressStatus::InProgress,
            }],
            replacement: None,
            replacement_source: None,
            bootstrap: false,
        })
        .expect("preview");
    fs::set_permissions(&progress, fs::Permissions::from_mode(0o500))
        .expect("read-only progress directory");
    let result = store.apply_progress(preview);
    fs::set_permissions(&progress, fs::Permissions::from_mode(0o700))
        .expect("restore progress directory");
    assert!(matches!(result, Err(casefile_store::StoreError::Io(_))));
    assert_eq!(fs::read_to_string(&log).expect("previous log"), original);
}

#[test]
fn progress_target_paths_must_remain_contained() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let request = ProgressChangeRequest {
        investigation: "../outside".into(),
        entries: Vec::new(),
        replacement: None,
        replacement_source: None,
        bootstrap: true,
    };
    assert!(matches!(
        store.preview_progress(request),
        Err(casefile_store::StoreError::Invalid(message)) if message.contains("must be contained")
    ));
}

#[test]
fn activation_state_distinguishes_unactivated_and_invalid_roots() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    assert_eq!(
        ActivationState::Active,
        store.scan().expect("scan").activation
    );

    fs::remove_file(root.path().join("casefile.toml")).expect("remove activation");
    let unactivated = store.scan().expect("unactivated scan");
    assert_eq!(ActivationState::Unactivated, unactivated.activation);
    assert!(unactivated.diagnostics.is_empty());
    assert!(
        unactivated
            .snapshot
            .entries
            .iter()
            .all(|entry| entry.classification == Classification::Ungoverned)
    );

    fs::write(root.path().join("casefile.toml"), "not = [valid").expect("bad activation");
    let invalid = store.scan().expect("invalid scan");
    assert_eq!(ActivationState::Invalid, invalid.activation);
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_activation")
    );
}

#[test]
fn structural_faults_are_deterministic_and_drafts_round_trip() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let draft = ticket(root.path());
    let rendered = casefile_core::render_draft(
        "projects/demo/investigations/sample/tickets/accepted/HMD-011.md",
        &draft,
    )
    .expect("render");
    assert!(matches!(
        casefile_core::parse_draft(
            "projects/demo/investigations/sample/tickets/accepted/HMD-011.md",
            Kind::Ticket,
            std::str::from_utf8(&rendered).expect("UTF-8")
        ),
        Ok(RecordDraft::Ticket(_))
    ));
    let mut injected = draft;
    if let RecordDraft::Ticket(item) = &mut injected {
        item.impact = "safe\n\n## Verification\n\ninjected".into();
    }
    assert!(
        casefile_core::render_draft(
            "projects/demo/investigations/sample/tickets/accepted/HMD-011.md",
            &injected
        )
        .is_err()
    );
    fs::write(root.path().join("projects/demo/investigations/sample/boards/main.toml"), "schema_version = 1\nid = 'HMD-board'\ntitle = 'bad'\n[[columns]]\nname = 'same'\nstatuses = ['accepted']\n[[columns]]\nname = 'same'\nstatuses = ['accepted']\n").expect("bad board");
    let result = store.scan().expect("scan");
    let first = result.diagnostics.clone();
    let second = store.scan().expect("rescan").diagnostics;
    assert_eq!(first, second);
    assert!(
        first
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_board_column"
                || diagnostic.code == "overlapping_board_status")
    );
}

#[test]
fn table_rows_and_review_faults_are_structural() {
    let faults = [
        (
            "casefile.toml",
            "schema_version = 2\n",
            "invalid_schema_version",
        ),
        (
            "projects.toml",
            "[projects]\nother = 'x'\n",
            "invalid_project_map",
        ),
        ("request.md", "# Requests\n\n## Boundary\n", "request_shape"),
        (
            "decision-log/HMD-D-001-scope.md",
            "# Other\n\n## Status\n\nok\n\n## Decision\n\nok\n",
            "decision_filename_identity",
        ),
        ("evidence/observation.md", "no heading\n", "h1_count"),
        ("review/round-1.md", "no heading\n", "h1_count"),
        ("implementation-plan/PLAN.md", "# Plan\n", "missing_section"),
        ("final-disposition.md", "# Close\n", "missing_section"),
        (
            "strategy/review.toml",
            "schema_version = 1\nstrategy_id = 'x'\nphase = 'wrong'\nadapter = 'x'\n",
            "strategy_phase",
        ),
        (
            "tickets/accepted/HMD-011.md",
            "# no frontmatter\n",
            "missing_frontmatter",
        ),
        (
            "epics/accepted/HMD-E-001.md",
            "# no frontmatter\n",
            "missing_frontmatter",
        ),
        (
            "boards/main.toml",
            "schema_version = 1\nid = 'HMD-board'\ntitle = 'bad'\n",
            "missing_columns",
        ),
    ];
    for (relative, text, code) in faults {
        let root = fixture();
        fs::write(path(root.path(), relative), text).expect("fault");
        scan_has(root.path(), code);
    }
    let root = fixture();
    fs::write(
        root.path().join("projects.toml"),
        "[projects]\ndemo = 'x'\nlegacy = 'keep'\n",
    )
    .expect("extra map");
    assert!(
        Store::open(root.path())
            .expect("store")
            .scan()
            .expect("scan")
            .diagnostics
            .is_empty()
    );
}

#[test]
fn reference_cycle_attachment_prefix_symlink_and_json_diagnostics_are_checked() {
    let root = fixture();
    let ticket_path = path(root.path(), "tickets/accepted/HMD-011.md");
    let original = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(
        &ticket_path,
        original.replace("decision_refs: [HMD-D-001]", "decision_refs: [MISSING]"),
    )
    .expect("references");
    scan_has(root.path(), "unresolved_reference");
    fs::write(
        &ticket_path,
        original.replace("supersedes: []", "supersedes: [HMD-012, HMD-E-001]"),
    )
    .expect("cycle start");
    fs::write(
        path(root.path(), "tickets/accepted/HMD-012.md"),
        original.replace("HMD-011", "HMD-012"),
    )
    .expect("cycle branch");
    let epic = fs::read_to_string(path(root.path(), "epics/accepted/HMD-E-001.md")).expect("epic");
    fs::write(
        path(root.path(), "epics/accepted/HMD-E-001.md"),
        epic.replace("supersedes: []", "supersedes: [HMD-011]"),
    )
    .expect("second edge");
    scan_has(root.path(), "supersession_cycle");
    fs::write(
        path(root.path(), "evidence/observation.md"),
        "---\nattachments: [missing.txt]\n---\n\n# Evidence\n",
    )
    .expect("attachment");
    scan_has(root.path(), "missing_attachment");
    fs::write(root.path().join("casefile.toml"), "schema_version = 1\n[projects.demo]\nprefix = 'NEW'\ninvestigations = ['projects/demo/investigations/sample']\n").expect("prefix");
    scan_has(root.path(), "project_prefix");
    let first = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan")
        .diagnostics;
    let second = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan")
        .diagnostics;
    assert_eq!(
        serde_json::to_vec(&first).expect("JSON"),
        serde_json::to_vec(&second).expect("JSON")
    );
    #[cfg(unix)]
    {
        let root = fixture();
        let request = path(root.path(), "request.md");
        fs::remove_file(&request).expect("remove");
        std::os::unix::fs::symlink("elsewhere", &request).expect("symlink");
        scan_has(root.path(), "unsafe_path");
    }
}

#[test]
fn project_decisions_resolve_within_the_project_only() {
    let root = fixture();
    let ticket_path = path(root.path(), "tickets/accepted/HMD-011.md");
    fs::create_dir_all(root.path().join("projects/demo/decision-log")).expect("project decisions");
    fs::write(
        root.path()
            .join("projects/demo/decision-log/HMD-D-100-project.md"),
        "# HMD-D-100 - Project\n\n## Status\n\naccepted\n\n## Decision\n\nProject scope.\n",
    )
    .expect("project decision");
    let ticket = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(&ticket_path, ticket.replace("HMD-D-001", "HMD-D-100")).expect("project reference");
    let scan = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan");
    assert!(scan.diagnostics.is_empty(), "{:#?}", scan.diagnostics);
    assert!(scan.snapshot.entries.iter().any(|entry| entry.path
        == "projects/demo/decision-log/HMD-D-100-project.md"
        && entry.kind == Some(Kind::Decision)
        && entry.classification == Classification::Governed));

    fs::write(root.path().join("casefile.toml"), "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/sample']\n[projects.other]\nprefix = 'OTH'\ninvestigations = []\n").expect("second project");
    fs::create_dir_all(root.path().join("projects/other/decision-log")).expect("other decisions");
    fs::write(
        root.path()
            .join("projects/other/decision-log/OTH-D-001-other.md"),
        "# OTH-D-001 - Other\n\n## Status\n\naccepted\n\n## Decision\n\nOther scope.\n",
    )
    .expect("other decision");
    let ticket = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(&ticket_path, ticket.replace("HMD-D-100", "OTH-D-001"))
        .expect("cross-project reference");
    scan_has(root.path(), "unresolved_reference");
}

#[test]
fn derived_relationships_are_unique_and_directed() {
    let root = fixture();
    let ticket_path = path(root.path(), "tickets/accepted/HMD-011.md");
    let ticket = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(
        &ticket_path,
        ticket.replace(
            "related_tickets: []",
            "related_tickets: [HMD-E-001, HMD-E-001]",
        ),
    )
    .expect("duplicate ticket relationships");
    let epic_path = path(root.path(), "epics/accepted/HMD-E-001.md");
    let epic = fs::read_to_string(&epic_path).expect("epic");
    fs::write(
        &epic_path,
        epic.replace("related_tickets: []", "related_tickets: [HMD-011]"),
    )
    .expect("reciprocal epic relationship");

    let relationships = Store::open(root.path())
        .expect("store")
        .derived_snapshot()
        .expect("derived snapshot")
        .relationships
        .into_iter()
        .filter(|relationship| relationship.kind == RelationshipKind::Related)
        .collect::<Vec<_>>();

    assert_eq!(relationships.len(), 2);
    assert!(relationships.iter().any(|relationship| {
        relationship.source.identity == "HMD-011" && relationship.target.identity == "HMD-E-001"
    }));
    assert!(relationships.iter().any(|relationship| {
        relationship.source.identity == "HMD-E-001" && relationship.target.identity == "HMD-011"
    }));
}

#[test]
fn nested_investigation_roots_have_distinct_scoped_identities() {
    let root = fixture();
    let ticket =
        fs::read_to_string(path(root.path(), "tickets/accepted/HMD-011.md")).expect("ticket");
    fs::write(
        root.path().join("casefile.toml"),
        "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/alpha/shared', 'projects/demo/investigations/beta/shared']\n",
    )
    .expect("nested activation");
    for investigation in ["alpha/shared", "beta/shared"] {
        let ticket_path = root.path().join(format!(
            "projects/demo/investigations/{investigation}/tickets/accepted/HMD-011.md"
        ));
        fs::create_dir_all(ticket_path.parent().expect("ticket parent")).expect("ticket directory");
        fs::write(
            ticket_path,
            ticket.replace(
                "investigation: \"sample\"",
                &format!("investigation: \"{investigation}\""),
            ),
        )
        .expect("ticket");
    }

    let store = Store::open(root.path()).expect("store");
    let scan = store.scan().expect("scan");
    assert_eq!(
        scan.scope_for_path(
            "projects/demo/investigations/alpha/shared/tickets/accepted/HMD-011.md"
        ),
        Some(("demo", Some("alpha/shared")))
    );
    assert_eq!(
        scan.scope_for_path("projects/demo/investigations/beta/shared/tickets/accepted/HMD-011.md"),
        Some(("demo", Some("beta/shared")))
    );

    let scopes = store
        .derived_snapshot()
        .expect("derived snapshot")
        .records
        .into_iter()
        .filter(|record| {
            record
                .identity
                .as_ref()
                .is_some_and(|identity| identity.identity == "HMD-011")
        })
        .map(|record| record.identity.expect("identity").scope)
        .collect::<Vec<_>>();
    assert!(scopes.contains(&casefile_store::RecordScope {
        project: "demo".into(),
        investigation: Some("alpha/shared".into()),
    }));
    assert!(scopes.contains(&casefile_store::RecordScope {
        project: "demo".into(),
        investigation: Some("beta/shared".into()),
    }));
}

#[test]
fn previews_and_applies_one_path_without_touching_index() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let index = fs::read(root.path().join(".git/index")).expect("index");
    fs::write(root.path().join("unrelated.txt"), "dirty").expect("dirty worktree");
    let mut create = ticket(root.path());
    if let RecordDraft::Ticket(item) = &mut create {
        item.id = "HMD-012".into();
        item.status = "provisional".into();
        item.title = "Created ticket".into();
    }
    let create_path =
        "projects/demo/investigations/sample/tickets/provisional/HMD-012.md".to_owned();
    let preview = store
        .preview(ChangeRequest::Create {
            path: create_path.clone(),
            draft: create,
        })
        .expect("preview");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
    assert!(preview.diff.contains("new file mode"));
    let create_result = store.apply(preview).expect("create");
    assert!(root.path().join(&create_path).is_file());
    assert_headers(&create_result.diff, &create_path, false, true);
    assert_eq!(
        index,
        fs::read(root.path().join(".git/index")).expect("index preserved")
    );
    assert_eq!(
        "dirty",
        fs::read_to_string(root.path().join("unrelated.txt")).expect("unrelated")
    );
    let mut replacement = ticket(root.path());
    if let RecordDraft::Ticket(item) = &mut replacement {
        item.title = "Replacement".into();
    }
    let replace_path = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md".to_owned();
    let original = fs::read(root.path().join(&replace_path)).expect("original");
    let stale = store
        .preview(ChangeRequest::Replace {
            path: replace_path.clone(),
            draft: replacement,
        })
        .expect("replace preview");
    fs::write(root.path().join(&replace_path), "changed outside preview").expect("external change");
    assert!(store.apply(stale).is_err());
    fs::write(root.path().join(&replace_path), original).expect("restore fixture");
    let mut replacement = ticket(root.path());
    if let RecordDraft::Ticket(item) = &mut replacement {
        item.title = "Applied replacement".into();
    }
    let replace = store
        .preview(ChangeRequest::Replace {
            path: replace_path.clone(),
            draft: replacement,
        })
        .expect("replace");
    assert_headers(&replace.diff, &replace_path, true, true);
    let replace_result = store.apply(replace).expect("apply replacement");
    assert_headers(&replace_result.diff, &replace_path, true, true);
    assert_eq!(
        index,
        fs::read(root.path().join(".git/index")).expect("index after replace")
    );
    let delete = store
        .preview(ChangeRequest::Delete {
            path: create_path.clone(),
        })
        .expect("delete preview");
    assert!(delete.diagnostics.is_empty());
    assert_headers(&delete.diff, &create_path, true, false);
    let delete_result = store.apply(delete).expect("delete");
    assert_headers(&delete_result.diff, &create_path, true, false);
    assert_eq!(
        index,
        fs::read(root.path().join(".git/index")).expect("index after delete")
    );
    assert!(!root.path().join(create_path).exists());
}

#[test]
fn generic_preview_preserves_baseline_diagnostics_but_rejects_introduced_diagnostics() {
    let root = fixture();
    let historical = "projects/demo/investigations/z-historical";
    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"projects/demo/investigations/sample\", \"{historical}\"]\n"
        ),
    )
    .expect("activation");
    let historical_ticket = root
        .path()
        .join(historical)
        .join("tickets/accepted/HMD-011.md");
    fs::create_dir_all(historical_ticket.parent().expect("ticket parent"))
        .expect("ticket directory");
    fs::copy(
        root.path()
            .join("projects/demo/investigations/sample/tickets/accepted/HMD-011.md"),
        &historical_ticket,
    )
    .expect("historical ticket");
    let store = Store::open(root.path()).expect("store");
    let baseline = store.scan().expect("baseline scan").diagnostics;
    assert!(!baseline.is_empty());

    let board_path = "projects/demo/investigations/sample/boards/delivery.toml";
    let preview = store
        .preview(ChangeRequest::Create {
            path: board_path.into(),
            draft: delivery_board("HMD-sample-delivery"),
        })
        .expect("preview with baseline diagnostics");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
    store
        .apply(preview)
        .expect("apply with baseline diagnostics");
    assert_eq!(baseline, store.scan().expect("resulting scan").diagnostics);

    let fresh = fixture();
    let existing = fresh
        .path()
        .join("projects/demo/investigations/sample/boards/main.toml");
    let source = fs::read_to_string(&existing).expect("existing board");
    fs::write(
        &existing,
        source.replace("HMD-board", "HMD-sample-delivery"),
    )
    .expect("colliding board identity");
    let introduced = Store::open(fresh.path())
        .expect("fresh store")
        .preview(ChangeRequest::Create {
            path: board_path.into(),
            draft: delivery_board("HMD-sample-delivery"),
        })
        .expect("introduced diagnostic preview");
    assert!(introduced.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "duplicate_identity"
            && diagnostic.message.contains("identity also appears")
    }));
    assert!(
        Store::open(fresh.path())
            .expect("apply store")
            .apply(introduced)
            .is_err()
    );
    assert!(!fresh.path().join(board_path).exists());
}

fn delivery_board(id: &str) -> RecordDraft {
    RecordDraft::Board(BoardDraft {
        id: id.into(),
        title: "Delivery".into(),
        status_source: BoardStatusSource::Progress,
        filter_statuses: None,
        filter_kinds: Some(vec!["ticket".into()]),
        columns: vec![BoardColumn {
            name: "Unknown".into(),
            statuses: vec!["unknown".into()],
        }],
    })
}

#[test]
fn binary_delete_preview_has_canonical_paths() {
    let root = fixture();
    let target = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    fs::write(root.path().join(target), [0xff, 0x00]).expect("binary ticket");
    let preview = Store::open(root.path())
        .expect("store")
        .preview(ChangeRequest::Delete {
            path: target.into(),
        })
        .expect("preview");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
    assert!(
        preview
            .diff
            .contains(&format!("diff --git a/{target} b/{target}"))
    );
    assert!(
        preview
            .diff
            .contains(&format!("Binary files a/{target} and /dev/null differ")),
        "{}",
        preview.diff
    );
    assert!(
        !preview.diff.contains(".tmp") && !preview.diff.contains("/tmp/"),
        "{}",
        preview.diff
    );
}

fn assert_headers(diff: &str, path: &str, before: bool, after: bool) {
    let old = if before {
        format!("--- a/{path}")
    } else {
        "--- /dev/null".into()
    };
    let new = if after {
        format!("+++ b/{path}")
    } else {
        "+++ /dev/null".into()
    };
    assert!(
        diff.contains(&format!("diff --git a/{path} b/{path}")),
        "{diff}"
    );
    assert!(diff.contains(&old), "{diff}");
    assert!(diff.contains(&new), "{diff}");
    assert!(!diff.contains(".tmp") && !diff.contains("/tmp/"), "{diff}");
}

const FULL_IMPLEMENTATION: &str = r#"schema_version = 1
strategy_id = "casefile-implement-ticket-batch"
phase = "implementation"
adapter = "codex"
[orchestrator]
binding = "root"
[limits]
max_concurrent_subagents = 3
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
"#;

const BINDING: &str = r#"schema_version = 1
adapter = "codex"
role = "implementation-writer"
model = "gpt-5.6-terra"
reasoning_effort = "high"
[resolution]
mode = "profile"
value = "casefile-implement-ticket-batch-implementation-writer"
"#;

fn full_implementation(root: &Path) {
    fs::write(
        path(root, "strategy/implementation.toml"),
        FULL_IMPLEMENTATION,
    )
    .expect("matrix");
}

#[test]
fn strategy_binding_projection_keeps_legacy_and_failure_states_distinct() {
    use casefile_store::{StrategyBindingState, WriterBindingSource};
    let root = fixture();
    full_implementation(root.path());
    let store = Store::open(root.path()).expect("store");
    let absent = store.derived_snapshot().expect("absent");
    let implementation = absent
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("projected implementation");
    assert!(
        matches!(implementation.binding.as_ref(), Some(StrategyBindingState::Absent { effective }) if effective.model == "gpt-5.6-sol" && effective.source == WriterBindingSource::Matrix)
    );

    fs::write(path(root.path(), "strategy/bindings.toml"), BINDING).expect("binding");
    let resolved = store.derived_snapshot().expect("resolved");
    let implementation = resolved
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("projected implementation");
    assert!(
        matches!(implementation.binding.as_ref(), Some(StrategyBindingState::Resolved { effective }) if effective.model == "gpt-5.6-terra" && effective.source == WriterBindingSource::Binding)
    );
    let binding = resolved
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/bindings.toml"))
        .and_then(|record| record.strategy_binding.as_ref())
        .expect("binding projection");
    assert!(matches!(
        binding.state,
        StrategyBindingState::Resolved { .. }
    ));

    fs::write(
        path(root.path(), "strategy/bindings.toml"),
        BINDING.replace("adapter = \"codex\"", "adapter = \"claude\""),
    )
    .expect("mismatch");
    let unresolved = store.derived_snapshot().expect("unresolved");
    assert!(
        unresolved
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "binding_adapter"
                && diagnostic.path.ends_with("strategy/bindings.toml"))
    );
    let implementation = unresolved
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("projected implementation");
    assert!(matches!(
        implementation.binding,
        Some(StrategyBindingState::Unresolved)
    ));

    fs::write(
        path(root.path(), "strategy/bindings.toml"),
        BINDING.replace("implementation-writer", "reviewer"),
    )
    .expect("invalid binding");
    let invalid = store.derived_snapshot().expect("invalid");
    assert!(
        invalid
            .records
            .iter()
            .any(|record| record.path.ends_with("strategy/bindings.toml")
                && record.classification == Classification::Invalid)
    );
    let implementation = invalid
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("projected implementation");
    assert!(matches!(
        implementation.binding,
        Some(StrategyBindingState::Invalid)
    ));
}

#[test]
fn strategy_binding_is_pending_without_implementation_and_unresolved_without_exact_writer() {
    use casefile_store::StrategyBindingState;
    let root = fixture();
    fs::remove_file(path(root.path(), "strategy/implementation.toml"))
        .expect("remove implementation");
    fs::write(path(root.path(), "strategy/bindings.toml"), BINDING).expect("binding");
    let store = Store::open(root.path()).expect("store");
    let pending = store.derived_snapshot().expect("pending");
    let binding = pending
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/bindings.toml"))
        .and_then(|record| record.strategy_binding.as_ref())
        .expect("binding projection");
    assert!(matches!(binding.state, StrategyBindingState::Pending));

    fs::write(
        path(root.path(), "strategy/implementation.toml"),
        "schema_version = 1\nstrategy_id = 'legacy'\nphase = 'implementation'\nadapter = 'codex'\n",
    )
    .expect("legacy implementation");
    let legacy = store.derived_snapshot().expect("legacy unresolved");
    let binding = legacy
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/bindings.toml"))
        .and_then(|record| record.strategy_binding.as_ref())
        .expect("binding projection");
    assert!(matches!(binding.state, StrategyBindingState::Unresolved));
    assert!(
        legacy
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "binding_writer_match"
                && diagnostic.path.ends_with("strategy/bindings.toml"))
    );

    full_implementation(root.path());
    fs::write(
        path(root.path(), "strategy/implementation.toml"),
        FULL_IMPLEMENTATION.replace("implementation-writer", "reviewer"),
    )
    .expect("no writer");
    let unresolved = store.derived_snapshot().expect("unresolved");
    assert!(
        unresolved
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "binding_writer_match")
    );
    let implementation = unresolved
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("projected implementation");
    assert!(matches!(
        implementation.binding,
        Some(StrategyBindingState::Unresolved)
    ));

    fs::write(
        path(root.path(), "strategy/implementation.toml"),
        format!("{FULL_IMPLEMENTATION}\n[[workers]]\nrole = \"implementation-writer\"\nplatform_profile = \"second-writer\"\nmodel = \"gpt-5.6-sol\"\nreasoning = \"high\"\nminimum_count = 1\nmaximum_count = 1\ncan_spawn_subagents = false\n"),
    )
    .expect("ambiguous writer");
    let ambiguous = store.derived_snapshot().expect("ambiguous");
    let implementation = ambiguous
        .records
        .iter()
        .find(|record| record.path.ends_with("strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("projected implementation");
    assert!(matches!(
        implementation.binding,
        Some(StrategyBindingState::Unresolved)
    ));
}

#[test]
fn binding_replacement_is_guarded_single_file_and_atomic() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let investigation = "projects/demo/investigations/sample";
    assert!(
        store
            .replace_strategy_binding(investigation, BINDING, true)
            .is_err()
    );
    store
        .replace_strategy_binding(investigation, BINDING, false)
        .expect("create");
    let alternate = BINDING.replace("gpt-5.6-terra", "gpt-5.6-luna");
    store
        .replace_strategy_binding(investigation, &alternate, false)
        .expect("replace");
    assert_eq!(
        alternate,
        fs::read_to_string(path(root.path(), "strategy/bindings.toml")).expect("current")
    );
    let strategy = root
        .path()
        .join("projects/demo/investigations/sample/strategy");
    assert!(!strategy.join("binding-history").exists());
    assert!(!strategy.join(".binding-transaction.toml").exists());
    assert!(
        !fs::read_dir(&strategy)
            .expect("strategy entries")
            .any(|entry| {
                entry
                    .expect("strategy entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".bindings.toml.tmp-")
            })
    );
    assert!(
        store
            .replace_strategy_binding(investigation, "not = [toml", false)
            .is_err()
    );
    assert_eq!(
        alternate,
        fs::read_to_string(path(root.path(), "strategy/bindings.toml")).expect("unchanged")
    );
}

#[test]
fn strategy_bindings_are_isolated_by_active_investigation_scope() {
    use casefile_store::{StrategyBindingState, WriterBindingSource};
    let root = fixture();
    let sample = root.path().join("projects/demo/investigations/sample");
    let other = root.path().join("projects/demo/investigations/other");
    fs::create_dir_all(&other).expect("other");
    copy_tree(&sample, &other);
    fs::write(
        root.path().join("casefile.toml"),
        "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"projects/demo/investigations/sample\", \"projects/demo/investigations/other\"]\n",
    ).expect("activate other");
    full_implementation(root.path());
    fs::write(sample.join("strategy/bindings.toml"), BINDING).expect("sample binding");
    fs::write(
        other.join("strategy/implementation.toml"),
        FULL_IMPLEMENTATION.replace("gpt-5.6-sol", "gpt-5.6-luna"),
    )
    .expect("other matrix");
    let store = Store::open(root.path()).expect("store");
    let records = store.derived_snapshot().expect("states").records;
    let sample_strategy = records
        .iter()
        .find(|record| record.path.ends_with("sample/strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("sample strategy");
    assert!(
        matches!(sample_strategy.binding.as_ref(), Some(StrategyBindingState::Resolved { effective }) if effective.model == "gpt-5.6-terra")
    );
    let other_strategy = records
        .iter()
        .find(|record| record.path.ends_with("other/strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("other strategy");
    assert!(
        matches!(other_strategy.binding.as_ref(), Some(StrategyBindingState::Absent { effective }) if effective.model == "gpt-5.6-luna" && effective.source == WriterBindingSource::Matrix)
    );

    fs::write(
        other.join("strategy/bindings.toml"),
        BINDING.replace("adapter = \"codex\"", "adapter = \"claude\""),
    )
    .expect("other mismatch");
    let unresolved = store.derived_snapshot().expect("unresolved");
    let sample_strategy = unresolved
        .records
        .iter()
        .find(|record| record.path.ends_with("sample/strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("sample strategy");
    assert!(matches!(
        sample_strategy.binding,
        Some(StrategyBindingState::Resolved { .. })
    ));
    let other_strategy = unresolved
        .records
        .iter()
        .find(|record| record.path.ends_with("other/strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("other strategy");
    assert!(matches!(
        other_strategy.binding,
        Some(StrategyBindingState::Unresolved)
    ));
    assert!(unresolved.diagnostics.iter().any(|diagnostic| {
        diagnostic.path.ends_with("other/strategy/bindings.toml")
            && diagnostic.code == "binding_adapter"
    }));

    fs::write(
        other.join("strategy/bindings.toml"),
        BINDING.replace("implementation-writer", "reviewer"),
    )
    .expect("other invalid");
    let invalid = store.derived_snapshot().expect("invalid");
    let sample_strategy = invalid
        .records
        .iter()
        .find(|record| record.path.ends_with("sample/strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("sample strategy");
    assert!(matches!(
        sample_strategy.binding,
        Some(StrategyBindingState::Resolved { .. })
    ));
    let other_strategy = invalid
        .records
        .iter()
        .find(|record| record.path.ends_with("other/strategy/implementation.toml"))
        .and_then(|record| record.strategy.as_ref())
        .expect("other strategy");
    assert!(matches!(
        other_strategy.binding,
        Some(StrategyBindingState::Invalid)
    ));
}

#[test]
fn nested_active_investigations_use_the_most_specific_binding_scope() {
    use casefile_store::StrategyBindingState;
    let root = fixture();
    let sample = root.path().join("projects/demo/investigations/sample");
    let outer = root.path().join("projects/demo/investigations/outer");
    let inner = outer.join("inner");
    fs::create_dir_all(&outer).expect("outer");
    copy_tree(&sample, &outer);
    fs::create_dir_all(&inner).expect("inner");
    copy_tree(&sample, &inner);
    fs::write(root.path().join("casefile.toml"), "schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"projects/demo/investigations/outer\", \"projects/demo/investigations/outer/inner\"]\n").expect("activation");
    fs::write(
        outer.join("strategy/implementation.toml"),
        FULL_IMPLEMENTATION,
    )
    .expect("outer matrix");
    fs::write(outer.join("strategy/bindings.toml"), BINDING).expect("outer binding");
    fs::write(
        inner.join("strategy/implementation.toml"),
        FULL_IMPLEMENTATION.replace("gpt-5.6-sol", "gpt-5.6-luna"),
    )
    .expect("inner matrix");
    fs::write(
        inner.join("strategy/bindings.toml"),
        BINDING.replace("adapter = \"codex\"", "adapter = \"claude\""),
    )
    .expect("inner binding");
    let scan = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan");
    for expected in [
        "outer/strategy/bindings.toml",
        "outer/inner/strategy/bindings.toml",
    ] {
        let entry = scan
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path.ends_with(expected))
            .expect("binding entry");
        assert_eq!(Some(Kind::StrategyBinding), entry.kind);
    }
    assert_eq!(
        Some(("demo", Some("outer/inner"))),
        scan.scope_for_path("projects/demo/investigations/outer/inner/strategy/bindings.toml")
    );
    let derived = Store::open(root.path())
        .expect("store")
        .derived_snapshot()
        .expect("derived");
    let outer_strategy = derived
        .records
        .iter()
        .find(|record| {
            record.path.ends_with("outer/strategy/implementation.toml")
                && !record.path.contains("/inner/")
        })
        .and_then(|record| record.strategy.as_ref())
        .expect("outer strategy");
    assert!(
        matches!(outer_strategy.binding, Some(StrategyBindingState::Resolved { ref effective }) if effective.model == "gpt-5.6-terra")
    );
    let inner_strategy = derived
        .records
        .iter()
        .find(|record| {
            record
                .path
                .ends_with("outer/inner/strategy/implementation.toml")
        })
        .and_then(|record| record.strategy.as_ref())
        .expect("inner strategy");
    assert!(matches!(
        inner_strategy.binding,
        Some(StrategyBindingState::Unresolved)
    ));
    assert!(derived.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .path
            .ends_with("outer/inner/strategy/bindings.toml")
            && diagnostic.code == "binding_adapter"
    }));
    assert!(!derived.diagnostics.iter().any(|diagnostic| {
        diagnostic.path.ends_with("outer/strategy/bindings.toml")
            && !diagnostic
                .path
                .ends_with("outer/inner/strategy/bindings.toml")
    }));
}
