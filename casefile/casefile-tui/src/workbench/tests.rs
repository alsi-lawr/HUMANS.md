use super::*;
use crate::{EditIntent, test_support};
use casefile_core::{
    BindingResolution, CasefileSnapshot, Classification, Diagnostic, Kind, RecordSummary, Revision,
    StrategyBinding, StrategyCoordination, StrategyLimits, StrategyProjection,
    StrategyRequirements, StrategyWorker,
};
use casefile_store::{
    ActivationState, DerivedBoard, DerivedBoardColumn, DerivedCard, DerivedRecord, DerivedStrategy,
    DerivedStrategyBinding, EffectiveWriterBinding, RecordScope, ScanResult, Store,
    StrategyBindingState, WriterBindingSource,
};
use std::{collections::BTreeMap, fs, path::Path, time::Duration};
use tempfile::TempDir;

const TICKET_PATH: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-013.md";
const STRATEGY_PATH: &str = "projects/demo/investigations/sample/strategy/implementation.toml";
const BINDING_PATH: &str = "projects/demo/investigations/sample/strategy/bindings.toml";
const INVALID_STRATEGY_PATH: &str = "projects/demo/investigations/sample/strategy/review.toml";

fn strategy_app() -> App {
    let mut scan = test_support::scan();
    let binding = StrategyBinding {
        adapter: "codex".into(),
        role: "implementation-writer".into(),
        model: "gpt-5.6-terra".into(),
        reasoning_effort: "xhigh".into(),
        resolution: BindingResolution {
            mode: "catalog_id".into(),
            value: "gpt-5.6-terra/xhigh".into(),
        },
    };
    scan.snapshot.entries.extend([
        test_support::entry(
            STRATEGY_PATH,
            Classification::Governed,
            Some(Kind::Strategy),
            Some(RecordSummary::Strategy {
                strategy_id: "casefile-implement-pipeline".into(),
                phase: "implementation".into(),
                adapter: "codex".into(),
            }),
            b"strategy_id = \"casefile-implement-pipeline\"\nexact_strategy_source = true",
        ),
        test_support::entry(
            BINDING_PATH,
            Classification::Governed,
            Some(Kind::StrategyBinding),
            Some(RecordSummary::StrategyBinding {
                binding: binding.clone(),
            }),
            b"role = \"implementation-writer\"\nexact_binding_source = true",
        ),
        test_support::entry(
            INVALID_STRATEGY_PATH,
            Classification::Invalid,
            Some(Kind::Strategy),
            None,
            b"phase = \"review\"\ninvalid = [",
        ),
    ]);
    scan.diagnostics.push(
        Diagnostic::new(
            INVALID_STRATEGY_PATH,
            "invalid_toml",
            "invalid strategy syntax",
        )
        .field("invalid"),
    );

    let mut derived = test_support::derived(&scan);
    let effective = EffectiveWriterBinding {
        model: binding.model.clone(),
        reasoning_effort: binding.reasoning_effort.clone(),
        source: WriterBindingSource::Binding,
    };
    let mut strategy_record = derived_record(STRATEGY_PATH, Kind::Strategy);
    strategy_record.strategy = Some(DerivedStrategy {
        matrix: StrategyProjection {
            root_binding: "root".into(),
            limits: StrategyLimits {
                max_concurrent_subagents: 4,
                max_depth: 2,
            },
            requirements: StrategyRequirements {
                capabilities: vec!["ticket-review".into(), "implementation".into()],
            },
            workers: vec![StrategyWorker {
                role: "implementation-writer".into(),
                platform_profile: "casefile-writer".into(),
                model: Some("gpt-5.6-sol".into()),
                reasoning_effort: Some("high".into()),
                minimum_count: 1,
                maximum_count: 2,
                can_spawn_subagents: false,
            }],
            coordination: StrategyCoordination {
                batch_when_capacity_exceeded: true,
                candidate_review_before_ticket: true,
                shared_ticket_storage_required: true,
                pipeline: None,
            },
        },
        binding: Some(StrategyBindingState::Resolved {
            effective: effective.clone(),
        }),
    });
    let mut binding_record = derived_record(BINDING_PATH, Kind::StrategyBinding);
    binding_record.strategy_binding = Some(DerivedStrategyBinding {
        binding,
        state: StrategyBindingState::Resolved { effective },
    });
    derived.records.extend([strategy_record, binding_record]);
    App::new(scan, derived)
}

fn derived_record(path: &str, kind: Kind) -> DerivedRecord {
    DerivedRecord {
        path: path.into(),
        scope: Some(RecordScope {
            project: "demo".into(),
            investigation: Some("sample".into()),
        }),
        classification: Classification::Governed,
        kind: Some(kind),
        identity: None,
        title: path.into(),
        content: None,
        rendered_markdown: None,
        search_text: String::new(),
        work_item: None,
        progress: None,
        board: None,
        strategy: None,
        strategy_binding: None,
    }
}

#[test]
fn project_investigation_ticket_drill_down_selects_a_canonical_path() {
    let mut app = test_support::app(test_support::scan());
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
    let mut app = test_support::app(scan);

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
    let mut app = test_support::app(test_support::scan());
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Right);
    let rendered = test_support::render(&app, 120, 40);
    assert!(rendered.contains("Rendered"));
    assert!(rendered.contains("Navigator"));
    assert!(rendered.contains("\u{2022} first line"));
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
    let mut app = test_support::app(test_support::scan());
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
fn strategy_key_and_tab_cycle_preserve_existing_view_targets() {
    let mut app = strategy_app();
    for (key, view) in [
        ('1', View::Projects),
        ('2', View::Investigations),
        ('3', View::Tickets),
        ('4', View::Files),
        ('5', View::Strategies),
    ] {
        app.handle(KeyCode::Char(key));
        assert_eq!(app.browser.view(), view);
    }

    app.handle(KeyCode::Char('4'));
    app.handle(KeyCode::Char('t'));
    assert_eq!(app.browser.view(), View::Strategies);
    app.handle(KeyCode::Char('t'));
    assert_eq!(app.browser.view(), View::Boards);
    app.handle(KeyCode::Char('t'));
    assert_eq!(app.browser.view(), View::Projects);

    let output = test_support::render(&app, 180, 32);
    assert!(output.contains("[1] PROJECTS"));
    assert!(output.contains("[4] FILES"));
    assert!(output.contains("[5] STRATEGIES"));
    assert!(output.contains("[6] BOARDS"));
}

#[test]
fn boards_are_read_only_unfiltered_and_open_a_canonical_ticket_detail() {
    let scan = test_support::scan();
    let mut derived = test_support::derived(&scan);
    derived.boards.push(DerivedBoard {
        identity: casefile_store::ScopedIdentity {
            scope: RecordScope {
                project: "demo".into(),
                investigation: Some("sample".into()),
            },
            identity: "HMD-board".into(),
        },
        title: "Delivery".into(),
        status_source: casefile_core::BoardStatusSource::Progress,
        filter_statuses: None,
        filter_kinds: None,
        columns: vec![DerivedBoardColumn {
            name: "TODO".into(),
            statuses: vec!["unknown".into()],
            cards: vec![DerivedCard {
                identity: casefile_store::ScopedIdentity {
                    scope: RecordScope {
                        project: "demo".into(),
                        investigation: Some("sample".into()),
                    },
                    identity: "HMD-013".into(),
                },
                kind: Kind::Ticket,
                title: "Navigator".into(),
                status: "unknown".into(),
                rank: Some(3),
            }],
        }],
    });
    let mut app = App::new(scan, derived);

    app.handle(KeyCode::Char('6'));
    app.handle(KeyCode::Char('/'));
    for key in "no-match".chars().map(KeyCode::Char) {
        app.handle(key);
    }
    app.handle(KeyCode::Enter);
    let output = test_support::render(&app, 160, 28);
    assert!(output.contains("[6] BOARDS 1"));
    assert!(output.contains("Delivery"));
    assert!(output.contains("TODO (1)"));
    assert!(output.contains("HMD-013  unknown  Navigator"));
    assert!(output.contains("record filter does not alter cards"));
    assert!(test_support::render(&app, 70, 28).contains("Delivery"));
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some(TICKET_PATH),
    );
    app.handle(KeyCode::Char('e'));
    assert!(test_support::render(&app, 160, 28).contains("Read-only"));
}

#[test]
fn strategies_are_scoped_by_full_nested_investigation_identity() {
    let project_strategy = ScanResult {
        activation: ActivationState::Active,
        investigation_roots: BTreeMap::new(),
        snapshot: CasefileSnapshot {
            revision: Revision("sha256:project-strategy".into()),
            entries: vec![test_support::entry(
                "projects/demo/strategy/implementation.toml",
                Classification::Governed,
                Some(Kind::Strategy),
                Some(RecordSummary::Strategy {
                    strategy_id: "project-level-strategy".into(),
                    phase: "implementation".into(),
                    adapter: "codex".into(),
                }),
                b"project",
            )],
        },
        diagnostics: Vec::new(),
    };
    let mut no_investigation = test_support::app(project_strategy);
    no_investigation.handle(KeyCode::Char('5'));
    let empty = test_support::render(&no_investigation, 140, 32);
    assert!(empty.contains("This investigation has no strategy records"));
    assert!(!empty.contains("project-level-strategy"));

    let mut scan = test_support::scan();
    scan.investigation_roots = BTreeMap::from([(
        "demo".into(),
        vec!["alpha/shared".into(), "beta/shared".into()],
    )]);
    scan.snapshot.entries = vec![
        test_support::entry(
            "projects/demo/investigations/alpha/shared/strategy/investigation.toml",
            Classification::Governed,
            Some(Kind::Strategy),
            Some(RecordSummary::Strategy {
                strategy_id: "alpha-strategy".into(),
                phase: "investigation".into(),
                adapter: "codex".into(),
            }),
            b"alpha",
        ),
        test_support::entry(
            "projects/demo/investigations/beta/shared/strategy/investigation.toml",
            Classification::Governed,
            Some(Kind::Strategy),
            Some(RecordSummary::Strategy {
                strategy_id: "beta-strategy".into(),
                phase: "investigation".into(),
                adapter: "codex".into(),
            }),
            b"beta",
        ),
    ];
    let mut app = test_support::app(scan);

    app.handle(KeyCode::Char('5'));
    let alpha = test_support::render(&app, 140, 32);
    assert!(alpha.contains("alpha-strategy"));
    assert!(!alpha.contains("beta-strategy"));

    app.handle(KeyCode::Backspace);
    app.handle(KeyCode::Down);
    app.handle(KeyCode::Char('5'));
    let beta = test_support::render(&app, 140, 32);
    assert!(beta.contains("beta-strategy"));
    assert!(!beta.contains("alpha-strategy"));
}

#[test]
fn strategy_records_expose_typed_overview_exact_source_and_diagnostics() {
    let mut app = strategy_app();
    app.handle(KeyCode::Char('5'));
    let strategy = test_support::render(&app, 160, 44);
    for expected in [
        "IMPLEMENTATION",
        "casefile-implement-pipeline",
        "Root binding  root",
        "4 concurrent subagents, depth 2",
        "implementation-writer",
        "Effective writer  gpt-5.6-terra",
        "Effective source  binding",
    ] {
        assert!(strategy.contains(expected), "missing {expected}");
    }

    app.handle(KeyCode::Right);
    app.handle(KeyCode::Right);
    let strategy_source = test_support::render(&app, 160, 44);
    assert!(strategy_source.contains("exact_strategy_source = true"));
    app.handle(KeyCode::Left);
    app.handle(KeyCode::Left);

    app.handle(KeyCode::Down);
    let binding = test_support::render(&app, 160, 44);
    assert!(binding.contains("Implementation writer binding"));
    assert!(binding.contains("Catalog value  gpt-5.6-terra/xhigh"));
    assert!(binding.contains("Binding state  resolved"));

    app.handle(KeyCode::Right);
    app.handle(KeyCode::Right);
    let source = test_support::render(&app, 160, 44);
    assert!(source.contains("exact_binding_source = true"));

    app.handle(KeyCode::Down);
    app.handle(KeyCode::Right);
    let invalid = test_support::render(&app, 160, 44);
    assert!(invalid.contains("invalid_toml"));
    assert!(invalid.contains("invalid strategy syntax"));
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some(INVALID_STRATEGY_PATH),
    );
}

#[test]
fn strategies_filter_remain_in_files_and_are_read_only() {
    let mut app = strategy_app();
    app.handle(KeyCode::Char('5'));
    app.handle(KeyCode::Char('e'));
    assert_eq!(app.interaction, None);
    assert!(test_support::render(&app, 150, 36).contains("Read-only"));

    app.handle(KeyCode::Char('/'));
    for key in "implementation-writer".chars().map(KeyCode::Char) {
        app.handle(key);
    }
    app.handle(KeyCode::Enter);
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some(BINDING_PATH),
    );
    app.handle(KeyCode::Char('e'));
    assert_eq!(app.interaction, None);
    assert!(test_support::render(&app, 150, 36).contains("Read-only"));

    app.handle(KeyCode::Char('c'));
    app.handle(KeyCode::Char('4'));
    let files = test_support::render(&app, 150, 44);
    for name in ["implementation.toml", "bindings.toml", "review.toml"] {
        assert!(files.contains(name), "Files omitted {name}");
    }
}

#[test]
fn filtering_and_empty_hierarchy_states_remain_predictable() {
    let mut app = test_support::app(test_support::scan());
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
    let app = test_support::app(empty);
    let output = test_support::render(&app, 160, 28);
    assert!(output.contains("No projects are present"));
    assert!(output.contains("UNACTIVATED"));
}

#[test]
fn focus_navigation_help_and_go_up_are_visible() {
    let mut source = test_support::scan();
    source.snapshot.entries[0].original_bytes = "wrapped content ".repeat(300).into_bytes();
    let mut app = test_support::app(source);
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
    let mut app = test_support::app(scan);
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

    let mut governed = test_support::app(test_support::scan());
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

#[test]
fn boards_distinguish_no_definition_invalid_empty_and_stale_projections() {
    let scan = test_support::scan();
    let mut no_board = App::new(scan.clone(), test_support::derived(&scan));
    no_board.handle(KeyCode::Char('6'));
    assert!(test_support::render(&no_board, 120, 28).contains("no board definitions"));

    let mut invalid_scan = scan.clone();
    invalid_scan.diagnostics.extend([
        Diagnostic::new(
            "projects/demo/investigations/sample/boards/invalid.toml",
            "invalid_toml",
            "board syntax is malformed",
        ),
        Diagnostic::new(
            "projects/demo/investigations/sample/progress/log.toml",
            "invalid_progress_log",
            "progress syntax is malformed",
        ),
    ]);
    let mut invalid = App::new(invalid_scan.clone(), test_support::derived(&invalid_scan));
    invalid.handle(KeyCode::Char('6'));
    let invalid_output = test_support::render(&invalid, 120, 28);
    assert!(invalid_output.contains("Board definitions or the progress log are invalid"));
    assert!(invalid_output.contains("invalid_toml: board syntax is malformed"));
    assert!(invalid_output.contains("invalid_progress_log: progress syntax is malformed"));
    assert!(invalid_output.contains("Files or Diagnostics"));

    let mut derived = test_support::derived(&scan);
    derived.boards.push(board_with_cards("Empty", Vec::new()));
    let mut empty = App::new(scan.clone(), derived);
    empty.handle(KeyCode::Char('6'));
    assert!(test_support::render(&empty, 120, 28).contains("No cards."));

    let mut stale_derived = test_support::derived(&scan);
    stale_derived.source_revision = Revision("sha256:stale".into());
    let mut stale = App::new(scan, stale_derived);
    stale.handle(KeyCode::Char('6'));
    assert!(test_support::render(&stale, 120, 28).contains("Board projection is stale"));
}

#[test]
fn board_keyboard_selection_marks_the_card_changes_detail_and_skips_unresolved_identities() {
    let mut scan = test_support::scan();
    scan.snapshot.entries.extend([
        test_support::entry(
            "projects/demo/investigations/sample/tickets/accepted/HMD-014.md",
            Classification::Governed,
            Some(Kind::Ticket),
            Some(RecordSummary::WorkItem {
                id: "HMD-014".into(),
                title: "Follow-up".into(),
                status: "accepted".into(),
                rank: Some(4),
            }),
            b"follow-up",
        ),
        test_support::entry(
            "projects/demo/investigations/sample/tickets/accepted/HMD-099.md",
            Classification::Governed,
            Some(Kind::Ticket),
            Some(RecordSummary::WorkItem {
                id: "HMD-099".into(),
                title: "Duplicate one".into(),
                status: "accepted".into(),
                rank: Some(9),
            }),
            b"duplicate-one",
        ),
        test_support::entry(
            "projects/demo/investigations/sample/tickets/rejected/HMD-099.md",
            Classification::Governed,
            Some(Kind::Ticket),
            Some(RecordSummary::WorkItem {
                id: "HMD-099".into(),
                title: "Duplicate two".into(),
                status: "rejected".into(),
                rank: Some(10),
            }),
            b"duplicate-two",
        ),
    ]);
    let mut derived = test_support::derived(&scan);
    derived.boards.push(board_with_cards(
        "Delivery",
        vec![
            board_card("HMD-013", "Navigator"),
            board_card("HMD-014", "Follow-up"),
            board_card("HMD-404", "Missing ticket"),
            board_card("HMD-099", "Ambiguous ticket"),
        ],
    ));
    let mut app = App::new(scan, derived);

    app.handle(KeyCode::Char('6'));
    let initial = test_support::render(&app, 160, 56);
    assert!(initial.contains("> HMD-013  unknown  Navigator  [selected]"));
    assert!(initial.contains("Missing ticket"));
    assert!(initial.contains("missing identity]"));
    assert!(initial.contains("Ambiguous ticket"));
    assert!(initial.contains("ambiguous identity]"));
    assert!(initial.contains("Navigator"));

    app.handle(KeyCode::Down);
    let selected_next = test_support::render(&app, 160, 56);
    assert!(selected_next.contains("> HMD-014  unknown  Follow-up  [selected]"));
    assert!(selected_next.contains("tickets/accepted/HMD-014.md"));
    assert_eq!(
        app.browser
            .selected(&app.scan)
            .map(|entry| entry.path.as_str()),
        Some("projects/demo/investigations/sample/tickets/accepted/HMD-014.md"),
    );
}

#[test]
fn board_card_selection_survives_complete_projection_with_deletion_and_ambiguity_controls() {
    let second_path = "projects/demo/investigations/sample/tickets/accepted/HMD-014.md";
    let mut scan = test_support::scan();
    scan.snapshot.entries.push(ticket_entry(
        second_path,
        "HMD-014",
        "Follow-up",
        b"follow-up",
    ));
    let mut derived = test_support::derived(&scan);
    derived.boards.push(board_with_cards(
        "Delivery",
        vec![
            board_card("HMD-013", "Navigator"),
            board_card("HMD-014", "Follow-up"),
        ],
    ));
    let mut app = App::new(scan.clone(), derived.clone());
    app.handle(KeyCode::Char('6'));
    app.handle(KeyCode::Down);
    assert_eq!(app.browser.selected_path(), Some(second_path));

    app.apply_projection(
        UiProjection {
            scan: scan.clone(),
            derived: derived.clone(),
            provisional: false,
            unavailable: BTreeMap::new(),
        },
        ProjectionChange::Complete,
    );
    assert_eq!(app.browser.selected_path(), Some(second_path));
    assert!(
        test_support::render(&app, 160, 40).contains("> HMD-014  unknown  Follow-up  [selected]")
    );

    let mut deleted = test_support::derived(&scan);
    deleted.boards.push(board_with_cards(
        "Delivery",
        vec![board_card("HMD-013", "Navigator")],
    ));
    app.apply_projection(
        UiProjection {
            scan: scan.clone(),
            derived: deleted,
            provisional: false,
            unavailable: BTreeMap::new(),
        },
        ProjectionChange::Complete,
    );
    assert_eq!(app.browser.selected_path(), Some(TICKET_PATH));

    let mut ambiguous = App::new(scan.clone(), derived.clone());
    ambiguous.handle(KeyCode::Char('6'));
    ambiguous.handle(KeyCode::Down);
    let mut ambiguous_scan = scan;
    ambiguous_scan.snapshot.entries.push(ticket_entry(
        "projects/demo/investigations/sample/tickets/rejected/HMD-014.md",
        "HMD-014",
        "Duplicate",
        b"duplicate",
    ));
    ambiguous.apply_projection(
        UiProjection {
            scan: ambiguous_scan,
            derived,
            provisional: false,
            unavailable: BTreeMap::new(),
        },
        ProjectionChange::Complete,
    );
    assert!(ambiguous.browser.selected_path().is_none());
    assert!(
        ambiguous
            .feedback
            .as_deref()
            .is_some_and(|feedback| feedback.contains("ambiguous"))
    );
}

#[test]
fn refresh_target_matrix_uses_project_full_investigation_and_store_fallback() {
    let (_root, mut coordinator) = presentation_coordinator();
    let projection = coordinator.projection();
    let mut app = App::from_projection(projection, None);

    app.handle(KeyCode::Char('1'));
    assert_eq!(
        app.refresh_target(&coordinator, RefreshIntent::Current),
        PresentationTarget::Project {
            project: "demo".into()
        }
    );
    for key in ['2', '3', '4', '5', '6'] {
        app.handle(KeyCode::Char(key));
        assert_eq!(
            app.refresh_target(&coordinator, RefreshIntent::Current),
            PresentationTarget::Investigation {
                project: "demo".into(),
                path: "projects/demo/investigations/sample".into(),
            },
            "view {key}"
        );
    }
    assert_eq!(
        app.refresh_target(&coordinator, RefreshIntent::Store),
        PresentationTarget::Store
    );
    assert!(coordinator.observe(crate::RefreshObservation {
        generation: 8,
        minimum_scope: crate::RefreshMinimumScope::Store {
            reason: "activation changed".into(),
        },
    }));
    let narrow_target = app.refresh_target(&coordinator, RefreshIntent::Current);
    let rejected = coordinator
        .refresh(narrow_target)
        .expect_err("narrow refresh");
    app.feedback = Some(rejected);
    assert!(test_support::render(&app, 140, 32).contains("press R"));
    let store_target = app.refresh_target(&coordinator, RefreshIntent::Store);
    coordinator
        .refresh(store_target)
        .expect("Store refresh remains available");

    let empty = ScanResult {
        activation: ActivationState::Unactivated,
        investigation_roots: BTreeMap::new(),
        snapshot: CasefileSnapshot {
            revision: Revision("empty".into()),
            entries: Vec::new(),
        },
        diagnostics: Vec::new(),
    };
    let empty_projection = ui_projection(empty, false);
    let mut empty_app = App::from_projection(empty_projection, None);
    for key in ['1', '2', '3', '4', '5', '6'] {
        empty_app.handle(KeyCode::Char(key));
        assert_eq!(
            empty_app.refresh_target(&coordinator, RefreshIntent::Current),
            PresentationTarget::Store,
            "empty view {key}"
        );
    }
}

#[test]
fn promotion_preserves_pending_exact_and_semantic_anchors_and_handles_deletion() {
    let mut source = multi_ticket_scan();
    let mut app = App::new(source.clone(), test_support::derived(&source));
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Down);
    let selected = app.browser.selected_path().expect("selected").to_owned();

    let mut partial = source.clone();
    partial
        .snapshot
        .entries
        .retain(|entry| entry.path != selected);
    app.apply_projection(ui_projection(partial, true), ProjectionChange::Partial);
    assert_eq!(app.browser.selected_path(), Some(selected.as_str()));
    assert!(test_support::render(&app, 140, 32).contains("Selected item is still loading"));

    source.snapshot.entries.insert(
        0,
        ticket_entry("HMD-009.md", "HMD-009", "Inserted", b"inserted"),
    );
    app.apply_projection(
        ui_projection(source.clone(), false),
        ProjectionChange::Complete,
    );
    assert_eq!(app.browser.selected_path(), Some(selected.as_str()));

    let moved = "projects/demo/investigations/sample/tickets/accepted/moved-HMD-014.md";
    let entry = source
        .snapshot
        .entries
        .iter_mut()
        .find(|entry| entry.path == selected)
        .expect("selected entry");
    entry.path = moved.into();
    app.apply_projection(
        ui_projection(source.clone(), false),
        ProjectionChange::Complete,
    );
    assert_eq!(app.browser.selected_path(), Some(moved));

    source.snapshot.entries.push(ticket_entry(
        "projects/demo/investigations/sample/tickets/rejected/duplicate-HMD-014.md",
        "HMD-014",
        "Ambiguous",
        b"duplicate",
    ));
    source.snapshot.entries.push(ticket_entry(
        "projects/demo/investigations/sample/tickets/provisional/duplicate-HMD-014.md",
        "HMD-014",
        "Also ambiguous",
        b"duplicate-two",
    ));
    source.snapshot.entries.retain(|entry| entry.path != moved);
    app.apply_projection(ui_projection(source, false), ProjectionChange::Complete);
    assert!(app.browser.selected_path().is_none());
    assert!(
        app.feedback
            .as_deref()
            .is_some_and(|value| value.contains("ambiguous"))
    );

    let mut deletion = multi_ticket_scan();
    let mut app = App::new(deletion.clone(), test_support::derived(&deletion));
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Down);
    let deleted = app.browser.selected_path().expect("middle").to_owned();
    let following = deletion.snapshot.entries[2].path.clone();
    deletion
        .snapshot
        .entries
        .retain(|entry| entry.path != deleted);
    app.apply_projection(ui_projection(deletion, false), ProjectionChange::Complete);
    assert_eq!(app.browser.selected_path(), Some(following.as_str()));
}

#[test]
fn unchanged_filter_exact_disappearance_is_explained_and_resume_is_round_tripped() {
    let scan = test_support::scan();
    let mut app = App::new(scan.clone(), test_support::derived(&scan));
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Char('/'));
    for character in "Navigator".chars() {
        app.handle(KeyCode::Char(character));
    }
    app.handle(KeyCode::Enter);
    app.handle(KeyCode::Right);
    app.handle(KeyCode::Tab);
    let resume = app.resume();

    let restored = App::from_projection(ui_projection(scan.clone(), false), Some(resume.clone()));
    assert_eq!(restored.resume(), resume);

    let empty = ScanResult {
        activation: ActivationState::Active,
        investigation_roots: scan.investigation_roots.clone(),
        snapshot: CasefileSnapshot {
            revision: Revision("partial".into()),
            entries: Vec::new(),
        },
        diagnostics: Vec::new(),
    };
    let mut resumed = App::from_projection(ui_projection(empty, true), Some(resume.clone()));
    let mut moved = scan.clone();
    moved
        .snapshot
        .entries
        .iter_mut()
        .find(|entry| entry.path == TICKET_PATH)
        .expect("ticket")
        .path = "projects/demo/investigations/sample/tickets/accepted/moved-HMD-013.md".into();
    resumed.apply_projection(ui_projection(moved, false), ProjectionChange::Complete);
    assert_eq!(
        resumed.browser.selected_path(),
        Some("projects/demo/investigations/sample/tickets/accepted/moved-HMD-013.md")
    );

    let mut changed = scan;
    let selected = changed
        .snapshot
        .entries
        .iter_mut()
        .find(|entry| entry.path == TICKET_PATH)
        .expect("ticket");
    selected.summary = Some(RecordSummary::WorkItem {
        id: "HMD-013".into(),
        title: "Renamed away".into(),
        status: "accepted".into(),
        rank: Some(3),
    });
    app.apply_projection(ui_projection(changed, false), ProjectionChange::Complete);
    assert!(app.browser.selected_path().is_none());
    assert_eq!(
        app.feedback.as_deref(),
        Some("Selected item no longer matches the filter.")
    );
}

#[test]
fn detail_scroll_resets_only_when_selected_content_revision_changes() {
    let mut scan = test_support::scan();
    scan.snapshot.entries[0].original_bytes = "long content ".repeat(500).into_bytes();
    scan.snapshot.entries[0].content_revision = Revision("content-one".into());
    let mut app = App::new(scan.clone(), test_support::derived(&scan));
    app.handle(KeyCode::Char('3'));
    app.handle(KeyCode::Tab);
    test_support::render(&app, 80, 24);
    app.handle(KeyCode::PageDown);
    assert!(app.detail.scroll_position() > 0);
    let scroll = app.detail.scroll_position();

    let mut inserted = scan.clone();
    inserted.snapshot.entries.push(ticket_entry(
        "projects/demo/investigations/sample/tickets/accepted/HMD-999.md",
        "HMD-999",
        "Inserted",
        b"inserted",
    ));
    app.apply_projection(
        ui_projection(inserted.clone(), false),
        ProjectionChange::Complete,
    );
    assert_eq!(app.detail.scroll_position(), scroll);

    let moved_path = "projects/demo/investigations/sample/tickets/accepted/moved-HMD-013.md";
    inserted
        .snapshot
        .entries
        .iter_mut()
        .find(|entry| entry.path == TICKET_PATH)
        .expect("ticket")
        .path = moved_path.into();
    app.apply_projection(
        ui_projection(inserted.clone(), false),
        ProjectionChange::Complete,
    );
    assert_eq!(app.browser.selected_path(), Some(moved_path));
    assert_eq!(app.detail.scroll_position(), scroll);

    inserted
        .snapshot
        .entries
        .iter_mut()
        .find(|entry| entry.path == moved_path)
        .expect("moved ticket")
        .content_revision = Revision("content-two".into());
    app.apply_projection(ui_projection(inserted, false), ProjectionChange::Content);
    assert_eq!(app.detail.scroll_position(), 0);
}

#[test]
fn project_and_investigation_deletions_use_following_then_preceding_fallback() {
    let mut scan = test_support::scan();
    scan.investigation_roots = BTreeMap::from([
        (
            "demo".into(),
            vec!["alpha".into(), "beta".into(), "gamma".into()],
        ),
        ("other".into(), vec!["only".into()]),
    ]);
    scan.snapshot.entries = vec![
        ticket_entry(
            "projects/demo/investigations/alpha/tickets/accepted/HMD-101.md",
            "HMD-101",
            "Alpha",
            b"alpha",
        ),
        ticket_entry(
            "projects/demo/investigations/beta/tickets/accepted/HMD-102.md",
            "HMD-102",
            "Beta",
            b"beta",
        ),
        ticket_entry(
            "projects/demo/investigations/gamma/tickets/accepted/HMD-103.md",
            "HMD-103",
            "Gamma",
            b"gamma",
        ),
        ticket_entry(
            "projects/other/investigations/only/tickets/accepted/OTH-001.md",
            "OTH-001",
            "Other",
            b"other",
        ),
    ];

    let mut investigation = App::new(scan.clone(), test_support::derived(&scan));
    investigation.handle(KeyCode::Char('2'));
    investigation.handle(KeyCode::Down);
    assert_eq!(investigation.browser.selected_investigation(), Some("beta"));
    let mut without_beta = scan.clone();
    without_beta
        .investigation_roots
        .get_mut("demo")
        .expect("demo")
        .retain(|value| value != "beta");
    without_beta
        .snapshot
        .entries
        .retain(|entry| !entry.path.contains("/beta/"));
    investigation.apply_projection(
        ui_projection(without_beta, false),
        ProjectionChange::Complete,
    );
    assert_eq!(
        investigation.browser.selected_investigation(),
        Some("gamma")
    );

    let mut project = App::new(scan.clone(), test_support::derived(&scan));
    project.handle(KeyCode::Down);
    assert_eq!(project.browser.selected_project(), Some("other"));
    scan.investigation_roots.remove("other");
    scan.snapshot
        .entries
        .retain(|entry| !entry.path.starts_with("projects/other/"));
    project.apply_projection(ui_projection(scan, false), ProjectionChange::Complete);
    assert_eq!(project.browser.selected_project(), Some("demo"));
    assert_eq!(project.browser.selected_investigation(), Some("alpha"));
}

#[test]
fn provisional_projection_and_refresh_help_are_visible() {
    let mut app = App::from_projection(ui_projection(test_support::scan(), true), None);
    app.set_status("facts are unavailable while loading");
    let rendered = test_support::render(&app, 140, 32);
    assert!(rendered.contains("PROVISIONAL"));
    assert!(rendered.contains("facts are unavailable"));
    app.handle(KeyCode::Char('?'));
    let help = test_support::render(&app, 140, 32);
    assert!(help.contains("Refresh current scope or the whole Store"));
}

fn board_with_cards(title: &str, cards: Vec<DerivedCard>) -> DerivedBoard {
    DerivedBoard {
        identity: casefile_store::ScopedIdentity {
            scope: RecordScope {
                project: "demo".into(),
                investigation: Some("sample".into()),
            },
            identity: format!("HMD-{title}"),
        },
        title: title.into(),
        status_source: casefile_core::BoardStatusSource::Progress,
        filter_statuses: None,
        filter_kinds: None,
        columns: vec![DerivedBoardColumn {
            name: "TODO".into(),
            statuses: vec!["unknown".into()],
            cards,
        }],
    }
}

fn board_card(id: &str, title: &str) -> DerivedCard {
    DerivedCard {
        identity: casefile_store::ScopedIdentity {
            scope: RecordScope {
                project: "demo".into(),
                investigation: Some("sample".into()),
            },
            identity: id.into(),
        },
        kind: Kind::Ticket,
        title: title.into(),
        status: "unknown".into(),
        rank: None,
    }
}

fn presentation_coordinator() -> (TempDir, Coordinator) {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../casefile-store/tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
    let store = Store::open(temporary.path()).expect("store");
    let mut coordinator =
        Coordinator::start(store.presentation_session(), None).expect("coordinator");
    for _ in 0..1000 {
        coordinator.drain();
        if coordinator.investigation_target("demo", "sample").is_some() {
            return (temporary, coordinator);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("catalogue did not arrive");
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

fn ui_projection(scan: ScanResult, provisional: bool) -> UiProjection {
    UiProjection {
        derived: test_support::derived(&scan),
        scan,
        provisional,
        unavailable: BTreeMap::new(),
    }
}

fn multi_ticket_scan() -> ScanResult {
    let mut scan = test_support::scan();
    scan.snapshot.entries = vec![
        ticket_entry(
            "projects/demo/investigations/sample/tickets/accepted/HMD-013.md",
            "HMD-013",
            "First",
            b"first",
        ),
        ticket_entry(
            "projects/demo/investigations/sample/tickets/accepted/HMD-014.md",
            "HMD-014",
            "Middle",
            b"middle",
        ),
        ticket_entry(
            "projects/demo/investigations/sample/tickets/accepted/HMD-015.md",
            "HMD-015",
            "Following",
            b"following",
        ),
    ];
    scan
}

fn ticket_entry(path: &str, id: &str, title: &str, bytes: &[u8]) -> casefile_core::EntrySnapshot {
    test_support::entry(
        path,
        Classification::Governed,
        Some(Kind::Ticket),
        Some(RecordSummary::WorkItem {
            id: id.into(),
            title: title.into(),
            status: "accepted".into(),
            rank: None,
        }),
        bytes,
    )
}
