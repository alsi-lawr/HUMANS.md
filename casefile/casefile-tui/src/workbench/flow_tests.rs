use super::*;
use crate::test_support;
use casefile_core::{Classification, Kind, parse_strategy, parse_strategy_projection};
use casefile_store::{
    DerivedStrategy, EffectiveWriterBinding, StrategyBindingState, WriterBindingSource,
};

fn app() -> App {
    let source =
        include_str!("../../../adapters/codex/matrices/casefile-implement-ticket-batch.toml");
    let mut scan = test_support::scan();
    let entry = test_support::entry(
        "projects/demo/investigations/sample/strategy/implementation.toml",
        Classification::Governed,
        Some(Kind::Strategy),
        Some(parse_strategy("implementation.toml", source).unwrap()),
        source.as_bytes(),
    );
    let strategy = DerivedStrategy {
        matrix: parse_strategy_projection("implementation.toml", source)
            .unwrap()
            .unwrap(),
        binding: Some(StrategyBindingState::Resolved {
            effective: EffectiveWriterBinding {
                model: "selected-effective-model".into(),
                reasoning_effort: "high".into(),
                source: WriterBindingSource::Binding,
            },
        }),
    };
    let record = test_support::strategy_record(&entry, strategy);
    scan.snapshot.entries.push(entry);
    let mut derived = test_support::derived(&scan);
    derived.records.push(record);
    let mut app = App::new(scan, derived);
    app.handle(KeyCode::Char('5'));
    app.handle(KeyCode::Right);
    app.handle(KeyCode::Tab);
    app
}

#[test]
fn chart_navigation_reaches_tail_resizes_and_files_preserves_selected_source() {
    let mut app = app();
    for (width, height) in [(80, 24), (120, 40), (80, 24)] {
        let top = test_support::render(&app, width, height);
        app.handle(KeyCode::End);
        let tail = test_support::render(&app, width, height);
        assert_ne!(top, tail);
        assert!(app.detail.scroll_position() > 0);
        app.handle(KeyCode::Home);
        assert_eq!(top, test_support::render(&app, width, height));
    }
    let selected = app.browser.selected(&app.scan).unwrap().path.clone();
    app.handle(KeyCode::Char('4'));
    assert_eq!(app.browser.selected(&app.scan).unwrap().path, selected);
    assert!(test_support::render(&app, 120, 40).contains('▼'));
    app.handle(KeyCode::Right);
    let source = test_support::render(&app, 120, 40);
    assert!(source.contains("schema_version = 1"));
    assert_eq!(
        app.browser.selected(&app.scan).unwrap().original_bytes,
        include_bytes!("../../../adapters/codex/matrices/casefile-implement-ticket-batch.toml")
    );
    app.handle(KeyCode::Tab);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn binding_only_projection_refresh_replaces_cached_chart_without_changing_selection() {
    let mut app = app();
    let before = test_support::render(&app, 120, 60);
    let selected = app.browser.selected(&app.scan).unwrap().path.clone();
    let mut projection = UiProjection {
        relationship_updates: BTreeMap::new(),
        availability_changed: Vec::new(),
        removed: Vec::new(),
        incremental: false,
        provisional: false,
        unavailable: BTreeMap::new(),
        scan: app.scan.clone(),
        derived: app.derived.clone(),
    };
    projection.derived.records[0]
        .strategy
        .as_mut()
        .unwrap()
        .binding = Some(StrategyBindingState::Resolved {
        effective: EffectiveWriterBinding {
            model: "new-effective-model".into(),
            reasoning_effort: "low".into(),
            source: WriterBindingSource::Binding,
        },
    });
    app.apply_projection(projection, ProjectionChange::Content);
    let after = test_support::render(&app, 120, 60);
    assert_ne!(before, after);
    assert!(after.contains("new-effective-model"));
    assert_eq!(app.browser.selected(&app.scan).unwrap().path, selected);
}

#[test]
fn changing_investigation_replaces_the_chart_even_when_source_revisions_match() {
    let mut app = app();
    let mut other = app.browser.selected(&app.scan).unwrap().clone();
    other.path = other.path.replace("/sample/", "/sample-two/");
    let mut record = app.derived.records[0].clone();
    record.path = other.path.clone();
    record.strategy.as_mut().unwrap().binding = Some(StrategyBindingState::Resolved {
        effective: EffectiveWriterBinding {
            model: "other-scope-model".into(),
            reasoning_effort: "low".into(),
            source: WriterBindingSource::Binding,
        },
    });
    app.scan
        .investigation_roots
        .get_mut("demo")
        .unwrap()
        .push("sample-two".into());
    app.scan.snapshot.entries.push(other);
    app.derived.records.push(record);
    let first = test_support::render(&app, 120, 60);
    assert!(first.contains("selected-effective-model"));
    app.handle(KeyCode::Tab);
    app.handle(KeyCode::Backspace);
    app.handle(KeyCode::Down);
    app.handle(KeyCode::Char('5'));
    let second = test_support::render(&app, 120, 60);
    assert!(second.contains("other-scope-model"));
    assert!(!second.contains("selected-effective-model"));
}
