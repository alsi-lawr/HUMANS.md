use super::*;
use crate::{EditIntent, test_support};
use casefile_core::{CasefileSnapshot, Classification, Diagnostic, Kind, RecordSummary, Revision};
use casefile_store::{ActivationState, ScanResult};
use std::collections::BTreeMap;

const TICKET_PATH: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-013.md";

#[test]
fn project_investigation_ticket_drill_down_selects_a_canonical_path() {
    let mut app = App::new(test_support::scan());
    let projects = test_support::render(&app, 120, 32);
    assert!(projects.contains("[1] PROJECTS 1"));
    assert!(projects.contains("demo"));

    app.handle(KeyCode::Enter);
    let investigations = test_support::render(&app, 120, 32);
    assert!(investigations.contains("[2] INVESTIGATIONS 1"));
    assert!(investigations.contains("sample"));

    app.handle(KeyCode::Enter);
    let tickets = test_support::render(&app, 120, 32);
    assert!(tickets.contains("[3] TICKETS 1"));
    assert!(tickets.contains("HMD-013"));
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some(TICKET_PATH),
    );
}

#[test]
fn nested_investigations_with_the_same_leaf_are_selectable_independently() {
    let mut scan = test_support::scan();
    scan.investigation_roots = BTreeMap::from([(
        "demo".into(),
        vec!["alpha/shared".into(), "beta/shared".into()],
    )]);
    scan.snapshot.entries = vec![
        test_support::entry(
            "projects/demo/investigations/alpha/shared/tickets/accepted/HMD-101.md",
            Classification::Governed,
            Some(Kind::Ticket),
            Some(RecordSummary::WorkItem {
                id: "HMD-101".into(),
                title: "Alpha ticket".into(),
                status: "accepted".into(),
                rank: None,
            }),
            b"alpha",
        ),
        test_support::entry(
            "projects/demo/investigations/beta/shared/tickets/accepted/HMD-102.md",
            Classification::Governed,
            Some(Kind::Ticket),
            Some(RecordSummary::WorkItem {
                id: "HMD-102".into(),
                title: "Beta ticket".into(),
                status: "accepted".into(),
                rank: None,
            }),
            b"beta",
        ),
    ];
    let mut app = App::new(scan);

    app.handle(KeyCode::Enter);
    let investigations = test_support::render(&app, 120, 32);
    assert!(investigations.contains("alpha/shared"));
    assert!(investigations.contains("beta/shared"));

    app.handle(KeyCode::Enter);
    let alpha = test_support::render(&app, 120, 32);
    assert!(alpha.contains("HMD-101"));
    assert!(!alpha.contains("HMD-102"));

    app.handle(KeyCode::Backspace);
    app.handle(KeyCode::Down);
    app.handle(KeyCode::Enter);
    let beta = test_support::render(&app, 120, 32);
    assert!(beta.contains("HMD-102"));
    assert!(!beta.contains("HMD-101"));
}

#[test]
fn rendered_and_source_tabs_keep_markdown_readable_and_exact() {
    let mut app = App::new(test_support::scan());
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Right);
    let rendered = test_support::render(&app, 120, 40);
    assert!(rendered.contains("Rendered"));
    assert!(rendered.contains("Navigator"));
    assert!(rendered.contains("• first line"));
    assert!(rendered.contains("Safe"));
    assert!(!rendered.contains("# Navigator"));
    assert!(!rendered.contains('\x1b'));

    app.handle(KeyCode::Right);
    let source = test_support::render(&app, 120, 40);
    assert!(source.contains("Source"));
    assert!(source.contains("# Navigator"));
    assert!(source.contains(r"\u{1b}[31mnot a colour"));
}

#[test]
fn files_are_grouped_relative_to_the_selected_scope_and_include_project_files() {
    let mut app = App::new(test_support::scan());
    app.handle(KeyCode::Char('4'));
    let output = test_support::render(&app, 120, 40);
    for directory in ["decision-log/", "boards/", "evidence/", "review/"] {
        assert!(output.contains(directory), "missing {directory}");
    }
    assert!(output.contains("e-raw.txt"));
    assert!(output.contains("c-legacy.txt"));
    assert!(!output.contains("projects/demo/investigations/sample/evidence/"));
}

#[test]
fn filtering_and_empty_hierarchy_states_remain_predictable() {
    let mut app = App::new(test_support::scan());
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Char('/'));
    for key in "missing".chars().map(KeyCode::Char) {
        app.handle(key);
    }
    app.handle(KeyCode::Enter);
    assert!(app.browser.selected(&app.scan).is_none());
    assert!(test_support::render(&app, 90, 28).contains("Nothing matches the active filter"));
    app.handle(KeyCode::Char('c'));
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some(TICKET_PATH),
    );

    let empty = ScanResult {
        activation: ActivationState::Unactivated,
        investigation_roots: BTreeMap::new(),
        snapshot: CasefileSnapshot {
            revision: Revision("sha256:empty".into()),
            entries: Vec::new(),
        },
        diagnostics: Vec::new(),
    };
    let app = App::new(empty);
    let output = test_support::render(&app, 160, 28);
    assert!(output.contains("No projects are present"));
    assert!(output.contains("UNACTIVATED"));
}

#[test]
fn focus_navigation_help_and_go_up_are_visible() {
    let mut source = test_support::scan();
    source.snapshot.entries[0].original_bytes = "wrapped content ".repeat(300).into_bytes();
    let mut app = App::new(source);
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Right);
    test_support::render(&app, 70, 24);
    app.handle(KeyCode::Tab);
    app.handle(KeyCode::PageDown);
    assert!(app.detail.scroll_position() > 0);
    app.handle(KeyCode::Tab);
    app.handle(KeyCode::Backspace);
    assert!(test_support::render(&app, 100, 30).contains("Investigations"));
    app.handle(KeyCode::Char('?'));
    let output = test_support::render(&app, 100, 30);
    assert!(output.contains("Keyboard help"));
    assert!(output.contains("Drill into the selected scope"));
}

#[test]
fn diagnostics_and_editing_remain_governed_path_only() {
    let control = "\x1b]0;metadata\x07";
    let path = format!("projects/demo/investigations/sample/review/{control}-ticket.md");
    let scan = ScanResult {
        activation: ActivationState::Active,
        investigation_roots: BTreeMap::from([("demo".into(), vec!["sample".into()])]),
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
    app.handle(KeyCode::Char('4'));
    app.handle(KeyCode::Right);
    app.handle(KeyCode::Right);
    app.handle(KeyCode::Right);
    let output = test_support::render(&app, 160, 32);
    assert!(output.contains(r"code-\u{1b}]0;metadata\u{7}"));
    assert!(!output.contains('\x1b'));
    assert!(!output.contains('\x07'));
    app.handle(KeyCode::Char('e'));
    assert_eq!(app.interaction, None);

    let mut governed = App::new(test_support::scan());
    governed.handle(KeyCode::Char('3'));
    governed.handle(KeyCode::Char('e'));
    assert_eq!(
        governed.interaction,
        Some(Interaction::Edit(EditIntent {
            path: TICKET_PATH.into(),
            kind: Kind::Ticket,
        }))
    );
}
