use super::*;
use crate::test_support;
use casefile_core::ProgressStatus;
use casefile_store::Store;
use std::{fs, path::Path, time::Instant};
use tempfile::TempDir;

const ROOT: &str = "projects/demo/investigations/sample";
const BOARD: &str = "projects/demo/investigations/sample/boards/main.toml";
const TICKET: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";

#[test]
fn section_header_counts_keep_their_scope_across_views_filters_and_scoped_refresh() {
    let root = fixture();
    let activation = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        activation.replace(
            r#"sample""#,
            r#"sample", "projects/demo/investigations/z-other""#,
        ),
    )
    .unwrap();
    copy_tree(
        &root.path().join(ROOT),
        &root.path().join("projects/demo/investigations/z-other"),
    );
    fs::write(
        root.path().join("projects/demo/project-notes.txt"),
        "Project-level file",
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let mut coordinator = Coordinator::start(store.presentation_session(), None).unwrap();
    let mut app = App::from_projection(coordinator.projection(), None);
    finish(&mut coordinator, &mut app);
    assert_header_counts_across_views(&mut app, [2, 12, 3, 1]);

    app.set_view(View::Tickets);
    app.handle(KeyCode::Char('/'));
    for character in "HMD-011".chars() {
        app.handle(KeyCode::Char(character));
    }
    app.handle(KeyCode::Enter);
    assert_eq!(app.browser.entries(&app.scan).len(), 1);
    assert_header_counts_across_views(&mut app, [2, 12, 3, 1]);
    app.set_view(View::Boards);
    let ticket = fs::read_to_string(root.path().join(TICKET)).unwrap();
    fs::write(
        root.path().join(TICKET.replace("HMD-011", "HMD-099")),
        ticket.replace("HMD-011", "HMD-099"),
    )
    .unwrap();
    fs::write(
        root.path().join(ROOT).join("evidence/new.txt"),
        "New evidence",
    )
    .unwrap();
    fs::remove_file(root.path().join(BOARD)).unwrap();
    fs::remove_file(root.path().join(ROOT).join("strategy/review.toml")).unwrap();
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: ROOT.into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert_eq!(app.browser.selected_project(), Some("demo"));
    assert_eq!(app.browser.selected_investigation(), Some("sample"));
    // Removing the board may clear its card selection, but retains hierarchy and filter.
    assert!(test_support::render(&app, 200, 40).contains("Filter \"HMD-011\""));
    assert_header_counts_across_views(&mut app, [3, 11, 2, 0]);
    app.clear_filter();
    assert_header_counts_across_views(&mut app, [3, 11, 2, 0]);
}

fn assert_header_counts_across_views(app: &mut App, counts: [usize; 4]) {
    for view in [
        View::Projects,
        View::Investigations,
        View::Tickets,
        View::Files,
        View::Strategies,
        View::Boards,
    ] {
        app.set_view(view);
        let rendered = test_support::render(app, 200, 40);
        let header = rendered.lines().next().unwrap();
        for (section, count) in ["TICKETS", "FILES", "STRATEGIES", "BOARDS"]
            .into_iter()
            .zip(counts)
        {
            assert!(
                header.contains(&format!("{section} {count} ")),
                "{view:?}: {header}"
            );
        }
    }
}

#[test]
fn scoped_refresh_updates_rendered_boards_and_progress_without_losing_selection_or_other_rows() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let mut coordinator = Coordinator::start(store.presentation_session(), None).unwrap();
    let mut app = App::from_projection(coordinator.projection(), None);
    finish(&mut coordinator, &mut app);
    app.set_view(View::Boards);
    app.focus = Focus::Detail;
    assert!(test_support::render(&app, 180, 40).contains("HMD-011"));
    let selection = app.browser.state();
    let other_rows = app
        .scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| !entry.path.starts_with(ROOT))
        .cloned()
        .collect::<Vec<_>>();

    fs::create_dir_all(root.path().join(ROOT).join("progress")).unwrap();
    fs::write(root.path().join(ROOT).join("progress/log.toml"), "schema_version = 1\n[[entries]]\nid = 'start'\nrecorded_at = '2026-07-26T10:00:00Z'\nrecorded_by = 'root'\nticket_id = 'HMD-011'\nkind = 'transition'\nfrom = 'unknown'\nto = 'in_progress'\n").unwrap();
    fs::write(root.path().join(BOARD), "schema_version = 1\nid = 'HMD-board'\ntitle = 'Updated delivery'\nstatus_source = 'progress'\nfilter_kinds = ['ticket']\n[[columns]]\nname = 'Working'\nstatuses = ['in_progress']\n").unwrap();
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: ROOT.into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    let rendered = test_support::render(&app, 180, 40);
    assert!(rendered.contains("Updated delivery"));
    assert!(rendered.contains("HMD-011"));
    assert_eq!(
        app.board_records.values().next().unwrap().columns[0].cards[0].status,
        "in_progress"
    );
    assert_eq!(
        app.derived
            .records
            .iter()
            .find(|record| record.path == TICKET)
            .unwrap()
            .progress
            .as_ref()
            .unwrap()
            .status,
        ProgressStatus::InProgress
    );
    assert_eq!(app.browser.state(), selection);
    assert_eq!(app.focus, Focus::Detail);
    assert_eq!(
        app.scan
            .snapshot
            .entries
            .iter()
            .filter(|entry| !entry.path.starts_with(ROOT))
            .cloned()
            .collect::<Vec<_>>(),
        other_rows
    );

    let board = fs::read_to_string(root.path().join(BOARD)).unwrap();
    fs::write(
        root.path().join(BOARD),
        board
            .replace("HMD-board", "HMD-renamed-board")
            .replace("Updated delivery", "Renamed delivery"),
    )
    .unwrap();
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: ROOT.into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    let renamed = test_support::render(&app, 180, 40);
    assert!(renamed.contains("Renamed delivery"));
    assert!(!renamed.contains("Updated delivery"));
    assert!(renamed.lines().next().unwrap().contains("BOARDS 1"));
    assert_eq!(app.browser.state(), selection);

    let log_path = root.path().join(ROOT).join("progress/log.toml");
    let valid_log = fs::read(&log_path).unwrap();
    fs::write(&log_path, "malformed progress").unwrap();
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: ROOT.into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    let invalid = test_support::render(&app, 180, 40);
    assert!(
        invalid.contains("Board definitions or the progress log are invalid."),
        "{invalid}"
    );
    assert!(!invalid.contains("Renamed delivery"));
    fs::write(&log_path, valid_log).unwrap();
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: ROOT.into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert!(test_support::render(&app, 180, 40).contains("Renamed delivery"));

    fs::remove_file(root.path().join(BOARD)).unwrap();
    coordinator
        .refresh(PresentationTarget::Project {
            project: "demo".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert!(app.board_records.is_empty());
    assert!(
        !app.scan
            .snapshot
            .entries
            .iter()
            .any(|entry| entry.path == BOARD)
    );
    assert!(!test_support::render(&app, 180, 40).contains("Renamed delivery"));
}

#[test]
fn eager_and_lazy_detail_survive_promotion_refresh_and_external_edit() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let mut coordinator = Coordinator::start(store.presentation_session(), None).unwrap();
    let mut app = App::from_projection(coordinator.projection(), None);
    finish(&mut coordinator, &mut app);
    let original = fs::read(root.path().join(TICKET)).unwrap();
    let evidence = format!("{ROOT}/evidence/observation.md");
    assert!(coordinator.request_content(Some(&evidence)));
    finish(&mut coordinator, &mut app);
    assert_eq!(
        app.body_bytes(&evidence).unwrap(),
        fs::read(root.path().join(&evidence)).unwrap()
    );
    assert_eq!(app.body_bytes(TICKET).unwrap(), original);
    coordinator.refresh(PresentationTarget::Store).unwrap();
    finish(&mut coordinator, &mut app);
    assert_eq!(app.body_bytes(TICKET).unwrap(), original);
    app.set_view(View::Strategies);
    app.focus = Focus::Detail;
    app.handle(KeyCode::Right);
    assert!(test_support::render(&app, 120, 40).contains("Flow unavailable"));
    app.handle(KeyCode::Right);
    assert!(test_support::render(&app, 120, 40).contains("schema_version = 1"));

    fs::write(
        root.path().join(&evidence),
        "# Changed lazy evidence\n\nExternal edit wins",
    )
    .unwrap();
    coordinator.refresh(PresentationTarget::Store).unwrap();
    finish(&mut coordinator, &mut app);
    assert!(coordinator.request_content(Some(&evidence)));
    finish(&mut coordinator, &mut app);
    assert_eq!(
        app.body_bytes(&evidence).unwrap(),
        b"# Changed lazy evidence\n\nExternal edit wins"
    );
    assert_eq!(app.body_bytes(TICKET).unwrap(), original);
}

#[test]
fn parent_scope_refresh_keeps_nested_body_and_removes_only_parent_members() {
    let root = fixture();
    let nested = format!("{ROOT}/child");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../casefile-store/tests/fixtures/minimum")
            .join(ROOT),
        &root.path().join(&nested),
    );
    let activation = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        activation.replace(
            "sample\"]",
            "sample\", \"projects/demo/investigations/sample/child\"]",
        ),
    )
    .unwrap();
    let store = Store::open(root.path()).unwrap();
    let mut coordinator = Coordinator::start(store.presentation_session(), None).unwrap();
    let mut app = App::from_projection(coordinator.projection(), None);
    finish(&mut coordinator, &mut app);
    let child = format!("{nested}/evidence/observation.md");
    assert!(coordinator.request_content(Some(&child)));
    finish(&mut coordinator, &mut app);
    let child_bytes = fs::read(root.path().join(&child)).unwrap();
    fs::remove_file(root.path().join(TICKET)).unwrap();
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: ROOT.into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert!(!app.entry_indices.contains_key(TICKET));
    assert_eq!(app.body_bytes(&child).unwrap(), child_bytes);
    assert_eq!(
        coordinator.investigation_target("demo", "sample/child"),
        Some(PresentationTarget::Investigation {
            project: "demo".into(),
            path: nested
        })
    );
}

pub(super) fn finish(coordinator: &mut Coordinator, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while coordinator.loading() && Instant::now() < deadline {
        let update = coordinator.drain();
        if update.projection != ProjectionChange::None {
            app.apply_projection(coordinator.take_projection(), update.projection);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!coordinator.loading());
}

pub(super) fn fixture() -> TempDir {
    let root = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../casefile-store/tests/fixtures/minimum"),
        root.path(),
    );
    root
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let output = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&output).unwrap();
            copy_tree(&entry.path(), &output);
        } else {
            fs::copy(entry.path(), output).unwrap();
        }
    }
}
