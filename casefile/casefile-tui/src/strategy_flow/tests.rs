use super::*;
use casefile_core::{parse_strategy, parse_strategy_projection};
use casefile_store::{EffectiveWriterBinding, WriterBindingSource};

const BATCH: &str =
    include_str!("../../../adapters/codex/matrices/casefile-implement-ticket-batch.toml");
const LOOKAHEAD: &str = include_str!(
    "../../../adapters/codex/matrices/casefile-implement-ticket-batch-look-ahead.toml"
);

fn strategy(text: &str) -> DerivedStrategy {
    DerivedStrategy {
        matrix: parse_strategy_projection("implementation.toml", text)
            .unwrap()
            .unwrap(),
        binding: Some(casefile_store::StrategyBindingState::Resolved {
            effective: EffectiveWriterBinding {
                model: "effective-model".into(),
                reasoning_effort: "high".into(),
                source: WriterBindingSource::Binding,
            },
        }),
    }
}

#[test]
fn selected_membership_not_declaration_order_controls_optional_review_and_correction_routing() {
    let mut selected = strategy(BATCH);
    selected
        .matrix
        .workers
        .retain(|worker| worker.role != "verification-reviewer");
    let original = contracts::build(
        "casefile-implement-ticket-batch",
        "implementation",
        &selected,
    )
    .unwrap();
    selected.matrix.workers.reverse();
    let reordered = contracts::build(
        "casefile-implement-ticket-batch",
        "implementation",
        &selected,
    )
    .unwrap();
    assert_eq!(original, reordered);
    assert!(!reordered.main.contains(&Stage::Verify));
    let verified = contracts::build(
        "casefile-implement-ticket-batch",
        "implementation",
        &strategy(BATCH),
    )
    .unwrap();
    assert_eq!(
        reordered.main,
        verified
            .main
            .into_iter()
            .filter(|stage| *stage != Stage::Verify)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        layout::render(&original, 54),
        layout::render(&reordered, 54)
    );
}

#[test]
fn resize_keeps_every_chart_cell_inside_the_pane_and_recovers_after_too_narrow() {
    let pipeline =
        include_str!("../../../adapters/codex/matrices/casefile-implement-pipeline.toml");
    let flow = contracts::build(
        "casefile-implement-pipeline",
        "implementation",
        &strategy(pipeline),
    )
    .unwrap();
    let normal = layout::render(&flow, 54);
    for width in [0, 1, 15, 16, 20, 30, 46, 54, 78] {
        let lines = layout::render(&flow, width);
        assert!(lines.iter().all(|line| line.width() <= usize::from(width)));
    }
    assert_eq!(normal, layout::render(&flow, 54));
}

#[test]
fn selected_cache_refreshes_on_binding_only_change_scope_change_and_resize() {
    let source = BATCH.as_bytes();
    let entry = crate::test_support::entry(
        "projects/demo/investigations/a/strategy/implementation.toml",
        Classification::Governed,
        Some(Kind::Strategy),
        Some(parse_strategy("implementation.toml", BATCH).unwrap()),
        source,
    );
    let mut record = crate::test_support::strategy_record(&entry, strategy(BATCH));
    let mut cache = Cache::default();
    let initial = cache.lines(&entry, Some(&record), 54);
    record.strategy.as_mut().unwrap().binding =
        Some(casefile_store::StrategyBindingState::Resolved {
            effective: EffectiveWriterBinding {
                model: "replacement-model".into(),
                reasoning_effort: "low".into(),
                source: WriterBindingSource::Binding,
            },
        });
    let replaced = cache.lines(&entry, Some(&record), 54);
    assert_eq!(initial, replaced);
    assert_eq!(replaced, cache.lines(&entry, Some(&record), 54));
    assert_ne!(replaced, cache.lines(&entry, Some(&record), 30));
    let mut other = entry.clone();
    other.path = other.path.replace("/a/", "/b/");
    assert_ne!(replaced, cache.lines(&other, None, 54));
    assert_eq!(replaced, cache.lines(&entry, Some(&record), 54));
    record.strategy.as_mut().unwrap().matrix.workers[0].role = "custom-worker".into();
    assert_ne!(replaced, cache.lines(&entry, Some(&record), 54));
}

#[test]
fn optional_preflight_does_not_change_the_execution_diagram() {
    let pipeline =
        include_str!("../../../adapters/codex/matrices/casefile-implement-pipeline.toml");
    for (id, source) in [
        ("casefile-implement-ticket-batch-look-ahead", LOOKAHEAD),
        ("casefile-implement-pipeline", pipeline),
    ] {
        let mut flow = contracts::build(id, "implementation", &strategy(source)).unwrap();
        let advisory = layout::render(&flow, 54);
        flow.preflight = false;
        let execution = layout::render(&flow, 54);
        assert_eq!(&advisory[..execution.len()], &execution);
        assert!(advisory.len() > execution.len());
    }
}

#[test]
fn declaration_refresh_revokes_contradictory_flows_without_rejecting_runtime_variants() {
    let atomic = include_str!("../../../adapters/codex/matrices/casefile-investigate-atomic.toml");
    for (path, source) in [
        ("investigation.toml", atomic),
        ("implementation.toml", BATCH),
    ] {
        let entry = crate::test_support::entry(
            path,
            Classification::Governed,
            Some(Kind::Strategy),
            Some(parse_strategy(path, source).unwrap()),
            source.as_bytes(),
        );
        let mut selected = strategy(source);
        selected.matrix.workers.iter_mut().for_each(|worker| {
            worker.platform_profile = "selected-profile".into();
            worker.model = Some("selected-runtime-model".into());
            worker.reasoning_effort = Some("high".into());
        });
        let mut record = crate::test_support::strategy_record(&entry, selected.clone());
        let mut cache = Cache::default();
        let unavailable = cache.lines(&entry, None, 54);
        let supported = cache.lines(&entry, Some(&record), 54);
        assert_ne!(supported, unavailable);
        let matrix = &mut record.strategy.as_mut().unwrap().matrix;
        if path == "investigation.toml" {
            matrix.coordination.candidate_review_before_ticket = false;
        } else {
            matrix
                .requirements
                .capabilities
                .retain(|capability| capability != "exclusive_writer");
        }
        assert_eq!(cache.lines(&entry, Some(&record), 54), unavailable);
        record.strategy = Some(selected);
        assert_eq!(cache.lines(&entry, Some(&record), 54), supported);
    }
}

#[test]
fn batching_compatibility_tracks_the_work_group_that_the_chart_will_actually_render() {
    let atomic = include_str!("../../../adapters/codex/matrices/casefile-investigate-atomic.toml");
    let entry = crate::test_support::entry(
        "investigation.toml",
        Classification::Governed,
        Some(Kind::Strategy),
        Some(parse_strategy("investigation.toml", atomic).unwrap()),
        atomic.as_bytes(),
    );
    let mut record = crate::test_support::strategy_record(&entry, strategy(atomic));
    let mut cache = Cache::default();
    let unavailable = cache.lines(&entry, None, 54);
    let grouped = cache.lines(&entry, Some(&record), 54);
    record
        .strategy
        .as_mut()
        .unwrap()
        .matrix
        .coordination
        .batch_when_capacity_exceeded = false;
    assert_eq!(cache.lines(&entry, Some(&record), 54), unavailable);
    record.strategy.as_mut().unwrap().matrix.workers[0].maximum_count = 1;
    let single = cache.lines(&entry, Some(&record), 54);
    assert_ne!(single, unavailable);
    assert_eq!(single, grouped);
}
