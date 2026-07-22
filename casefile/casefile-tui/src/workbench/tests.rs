use super::*;
use crate::{EditIntent, test_support};
use casefile_core::{CasefileSnapshot, Classification, Diagnostic, Kind, RecordSummary, Revision};
use casefile_store::{ActivationState, ScanResult};

#[test]
fn wide_work_queue_has_compact_navigation_and_overview() {
    let app = App::new(test_support::scan());
    let output = test_support::render(&app, 120, 32);
    assert!(output.contains("CASEFILE"));
    assert!(output.contains("[1] WORK 1"));
    assert!(output.contains("[2] RECORDS 5"));
    assert!(output.contains("ACTIVE"));
    assert!(output.contains("ACCEPTED"));
    assert!(output.contains("HMD-013"));
    assert!(output.contains("Overview"));
    assert!(output.contains("rank #3"));
    assert_eq!(layout_mode(Rect::new(0, 0, 120, 20)), LayoutMode::Wide);
}

#[test]
fn narrow_layout_stacks_complete_list_and_detail_panels() {
    let app = App::new(test_support::scan());
    let output = test_support::render(&app, 72, 38);
    let queue = output.find("Work queue").expect("queue");
    let overview = output.find("Overview").expect("overview");
    assert!(queue < overview);
    assert!(output.contains("HMD-013"));
    assert_eq!(layout_mode(Rect::new(0, 0, 72, 30)), LayoutMode::Narrow);
}

#[test]
fn records_keep_classification_diagnostics_and_binary_separate() {
    let mut app = App::new(test_support::scan());
    app.handle(KeyCode::Char('2'));
    for _ in 0..3 {
        app.handle(KeyCode::Down);
    }
    let output = test_support::render(&app, 120, 32);
    for label in ["GOVERNED", "UNGOVERNED", "INVALID", "RAW"] {
        assert!(output.contains(label), "missing {label}");
    }
    app.handle(KeyCode::Right);
    let output = test_support::render(&app, 120, 32);
    assert!(output.contains("Binary content"));
    assert!(output.contains("ff 00 10"));
    app.handle(KeyCode::Right);
    let output = test_support::render(&app, 120, 32);
    assert!(output.contains("invalid_shape"));
    assert!(output.contains("ticket is incomplete"));
    assert!(!output.contains("cross_record"));
}

#[test]
fn filter_empty_state_and_path_selection_remain_predictable() {
    let mut app = App::new(test_support::scan());
    app.handle(KeyCode::Char('2'));
    app.handle(KeyCode::Down);
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some("b-board.toml")
    );
    app.handle(KeyCode::Char('/'));
    for key in "missing".chars().map(KeyCode::Char) {
        app.handle(key);
    }
    app.handle(KeyCode::Enter);
    assert!(app.browser.selected(&app.scan).is_none());
    assert!(test_support::render(&app, 90, 28).contains("No records match"));
    app.handle(KeyCode::Char('c'));
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some("a-ticket.md")
    );
}

#[test]
fn focus_page_navigation_detail_scrolling_and_help_are_visible() {
    let mut source = test_support::scan();
    source.snapshot.entries[0].original_bytes = "wrapped content ".repeat(300).into_bytes();
    let mut app = App::new(source);
    app.handle(KeyCode::Right);
    test_support::render(&app, 70, 24);
    app.handle(KeyCode::Tab);
    app.handle(KeyCode::PageDown);
    assert!(app.detail.scroll_position() > 0);
    app.handle(KeyCode::Char('?'));
    let output = test_support::render(&app, 100, 30);
    assert!(output.contains("Keyboard help"));
    assert!(output.contains("Switch focus between list and detail"));
    app.handle(KeyCode::Esc);
    assert!(!app.show_help);
}

#[test]
fn metadata_controls_are_escaped_across_the_composed_workbench() {
    let control = "\x1b]0;metadata\x07";
    let path = format!("{control}-ticket.md");
    let scan = ScanResult {
        activation: ActivationState::Active,
        snapshot: CasefileSnapshot {
            revision: Revision("sha256:controls".into()),
            entries: vec![test_support::entry(
                &path,
                Classification::Invalid,
                Some(Kind::Ticket),
                Some(RecordSummary::WorkItem {
                    id: format!("HMD-{control}"),
                    title: format!("title-{control}"),
                    status: format!("status-{control}"),
                    rank: None,
                }),
                b"content",
            )],
        },
        diagnostics: vec![
            Diagnostic::new(
                &path,
                &format!("code-{control}"),
                format!("message-{control}"),
            )
            .field(&format!("field-{control}"))
            .section(&format!("section-{control}")),
        ],
    };
    let mut app = App::new(scan);
    app.handle(KeyCode::Char('2'));
    app.handle(KeyCode::Right);
    app.handle(KeyCode::Right);
    let output = test_support::render(&app, 160, 32);
    assert!(output.contains(r"code-\u{1b}]0;metadata\u{7}"));
    assert!(output.contains(r"field-\u{1b}]0;metadata\u{7}"));
    assert!(output.contains(r"section-\u{1b}]0;metadata\u{7}"));
    assert!(!output.contains('\x1b'));
    assert!(!output.contains('\x07'));
}

#[test]
fn empty_snapshot_has_useful_work_and_record_states() {
    let scan = ScanResult {
        activation: ActivationState::Unactivated,
        snapshot: CasefileSnapshot {
            revision: Revision("sha256:empty".into()),
            entries: Vec::new(),
        },
        diagnostics: Vec::new(),
    };
    let mut app = App::new(scan);
    assert!(test_support::render(&app, 100, 28).contains("No governed tickets or epics"));
    app.handle(KeyCode::Char('2'));
    let output = test_support::render(&app, 100, 28);
    assert!(output.contains("No records in this Casefile root"));
    assert!(output.contains("UNACTIVATED"));
}

#[test]
fn edit_yields_an_intent_only_for_supported_governed_records() {
    let mut app = App::new(test_support::scan());
    app.handle(KeyCode::Char('e'));
    assert_eq!(
        app.interaction,
        Some(Interaction::Edit(EditIntent {
            path: "a-ticket.md".into(),
            kind: Kind::Ticket,
        }))
    );

    let mut read_only = App::new(test_support::scan());
    read_only.handle(KeyCode::Char('2'));
    read_only.handle(KeyCode::Down);
    read_only.handle(KeyCode::Down);
    read_only.handle(KeyCode::Char('e'));
    assert_eq!(read_only.interaction, None);
    assert!(
        test_support::render(&read_only, 120, 32).contains("Read-only: e edits governed tickets")
    );
}
