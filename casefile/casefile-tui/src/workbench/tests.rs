use super::*;
use crate::{EditIntent, test_support};
use casefile_core::{
    BindingResolution, CasefileSnapshot, Classification, Diagnostic, Kind, RecordSummary, Revision,
    StrategyBinding, StrategyCoordination, StrategyLimits, StrategyProjection,
    StrategyRequirements, StrategyWorker,
};
use casefile_store::{
    ActivationState, DerivedBoard, DerivedBoardColumn, DerivedCard, DerivedRecord, DerivedStrategy,
    DerivedStrategyBinding, EffectiveWriterBinding, RecordScope, ScanResult, StrategyBindingState,
    WriterBindingSource,
};
use std::collections::BTreeMap;

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
            name: "Unknown".into(),
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
    assert!(output.contains("Unknown (1)"));
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
            name: "Unknown".into(),
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
