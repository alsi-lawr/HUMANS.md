use super::*;
use casefile_core::{AssignmentPhase, RoleAssignment};
use casefile_store::{
    RoleAssignmentSource, StoreError, StrategyAssignmentProjection, StrategyAssignmentRequest,
    StrategyAssignmentTarget,
};

const BATCH: &str = "casefile-implement-ticket-batch";
const PIPELINE: &str = "casefile-implement-pipeline";
const WRITER: &str = "implementation-writer";

fn target(strategy_id: &str) -> StrategyAssignmentTarget {
    StrategyAssignmentTarget {
        investigation: INVESTIGATION.into(),
        phase: AssignmentPhase::Implementation,
        strategy_id: strategy_id.into(),
    }
}

fn request(
    projection: StrategyAssignmentProjection,
    writer_model: &str,
) -> StrategyAssignmentRequest {
    StrategyAssignmentRequest {
        target: projection.target,
        expected_revisions: projection.revisions,
        roles: projection
            .roles
            .into_keys()
            .map(|role| RoleAssignment {
                model: if role == WRITER {
                    writer_model
                } else {
                    "test-reviewer"
                }
                .into(),
                reasoning_effort: "medium".into(),
                role,
            })
            .collect(),
    }
}

fn assert_stale<T: std::fmt::Debug>(result: Result<T, ProviderError>) {
    assert!(
        matches!(
            result,
            Err(ProviderError::Store(StoreError::StaleTargetRevision))
        ),
        "{result:?}"
    );
}

#[test]
fn role_assignments_uses_one_complete_retryable_write_and_rejects_stale_updates() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let provider = Provider::without_cache(store.clone());
    let phase_matrix = fs::read(strategy(root.path())).unwrap();
    let projection = provider
        .project_strategy_assignments(target(BATCH))
        .unwrap();
    assert!(projection.roles.contains_key(WRITER));
    assert!(
        projection
            .roles
            .values()
            .all(|role| role.selection.is_none())
    );

    let proposal = request(projection, "first-writer");
    let mut partial = proposal.clone();
    partial.roles.pop().unwrap();
    assert!(provider.preview_strategy_assignments(partial).is_err());
    let mut duplicate = proposal.clone();
    duplicate.roles.push(proposal.roles[0].clone());
    assert!(provider.preview_strategy_assignments(duplicate).is_err());
    let first = provider
        .preview_strategy_assignments(proposal.clone())
        .unwrap();
    let mut alternative = proposal.clone();
    alternative
        .roles
        .iter_mut()
        .find(|row| row.role == WRITER)
        .unwrap()
        .model = "chosen-writer".into();
    let chosen = provider
        .preview_strategy_assignments(alternative.clone())
        .unwrap();
    assert_eq!(chosen.operations.len(), 1);
    assert!(!root.path().join(&chosen.operations[0].path).exists());
    let applied = provider
        .apply_strategy_assignments(&chosen.preview_id)
        .unwrap()
        .result;
    assert_eq!(
        applied.operation,
        GovernedOperationKind::StrategyAssignments
    );
    assert!(!applied.no_op);
    assert_stale(provider.apply_strategy_assignments(&first.preview_id));
    assert_stale(provider.preview_strategy_assignments(proposal));
    assert!(
        provider
            .apply_strategy_assignments(&chosen.preview_id)
            .unwrap()
            .result
            .no_op
    );
    assert!(
        provider
            .preview_strategy_assignments(alternative.clone())
            .unwrap()
            .no_op
    );
    assert_eq!(fs::read(strategy(root.path())).unwrap(), phase_matrix);

    let reloaded = Provider::without_cache(Store::open(root.path()).unwrap())
        .project_strategy_assignments(target(BATCH))
        .unwrap();
    for row in alternative.roles {
        let saved = reloaded.roles[&row.role].selection.as_ref().unwrap();
        assert_eq!(saved.source, RoleAssignmentSource::StrategyAssignment);
        assert_eq!(
            (&saved.model, &saved.reasoning_effort),
            (&row.model, &row.reasoning_effort)
        );
    }
}

#[test]
fn role_assignments_match_legacy_scope_and_invalidate_a_changed_binding_or_matrix() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).unwrap());
    let legacy_path = root
        .path()
        .join(INVESTIGATION)
        .join("strategy/bindings.toml");
    let source = BINDING.replace("ticket-batch=", &format!("{BATCH}="));
    let save_legacy = |source: &str| {
        let preview = provider
            .preview_writer_binding(WriterBindingRequest {
                investigation: INVESTIGATION.into(),
                binding_source: source.into(),
            })
            .unwrap();
        provider.apply_writer_binding(&preview.preview_id).unwrap();
    };
    save_legacy(&source);
    let legacy = provider
        .project_strategy_assignments(target(BATCH))
        .unwrap();
    assert_eq!(
        legacy.roles[WRITER].selection.as_ref().unwrap().source,
        RoleAssignmentSource::LegacyWriterBinding
    );
    assert!(
        legacy
            .roles
            .iter()
            .filter(|(name, _)| *name != WRITER)
            .all(|(_, role)| role.selection.is_none())
    );
    let selected_writer = legacy.roles[WRITER]
        .selection
        .as_ref()
        .unwrap()
        .model
        .clone();
    let request_from_legacy = request(legacy, &selected_writer);
    let stale = provider
        .preview_strategy_assignments(request_from_legacy.clone())
        .unwrap();
    let revised_source = source.replace("gpt-5.6-terra", "changed-legacy-writer");
    save_legacy(&revised_source);
    assert_stale(provider.apply_strategy_assignments(&stale.preview_id));
    assert_stale(provider.preview_strategy_assignments(request_from_legacy));

    let legacy = provider
        .project_strategy_assignments(target(BATCH))
        .unwrap();
    let selected_writer = legacy.roles[WRITER]
        .selection
        .as_ref()
        .unwrap()
        .model
        .clone();
    let proposal = request(legacy, &selected_writer);
    let saved = provider
        .preview_strategy_assignments(proposal.clone())
        .unwrap();
    provider
        .apply_strategy_assignments(&saved.preview_id)
        .unwrap();
    let before_switch = provider.preview_strategy_assignments(proposal).unwrap();

    let switch = provider
        .preview_strategy_transition(transition_request())
        .unwrap();
    provider
        .apply_strategy_transition(&switch.preview_id)
        .unwrap();
    assert_stale(provider.apply_strategy_assignments(&before_switch.preview_id));
    assert!(
        provider
            .project_strategy_assignments(target(BATCH))
            .is_err()
    );
    let pipeline = provider
        .project_strategy_assignments(target(PIPELINE))
        .unwrap();
    assert!(pipeline.roles.values().all(|role| role.selection.is_none()));

    let mut return_to_batch = transition_request();
    return_to_batch.operation_id = "return-to-batch".into();
    return_to_batch.recorded_at = "2026-07-27T12:00:01Z".into();
    return_to_batch.selected_matrix_origin = format!("adapters/codex/matrices/{BATCH}.toml");
    return_to_batch.selected_matrix_source =
        fs::read_to_string(matrix_path(&format!("{BATCH}.toml"))).unwrap();
    return_to_batch.active_ownership.clear();
    let switch = provider
        .preview_strategy_transition(return_to_batch)
        .unwrap();
    provider
        .apply_strategy_transition(&switch.preview_id)
        .unwrap();
    let batch = provider
        .project_strategy_assignments(target(BATCH))
        .unwrap();
    assert_eq!(
        batch.roles[WRITER].selection.as_ref().unwrap().model,
        selected_writer
    );
    assert_eq!(fs::read_to_string(legacy_path).unwrap(), revised_source);
}

#[test]
fn role_assignments_leave_solo_and_claude_strategies_without_role_selections() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).unwrap());
    let solo = "casefile-investigate-solo";
    fs::copy(
        matrix_path(&format!("{solo}.toml")),
        root.path()
            .join(INVESTIGATION)
            .join("strategy/investigation.toml"),
    )
    .unwrap();
    let proj = provider
        .project_strategy_assignments(StrategyAssignmentTarget {
            phase: AssignmentPhase::Investigation,
            ..target(solo)
        })
        .unwrap();
    assert!(proj.roles.is_empty());
    let preview = provider
        .preview_strategy_assignments(request(proj, "unused"))
        .unwrap();
    assert!(
        provider
            .apply_strategy_assignments(&preview.preview_id)
            .unwrap()
            .result
            .no_op
    );
    assert!(
        !root
            .path()
            .join(INVESTIGATION)
            .join("strategy/assignments")
            .exists()
    );

    let claude = fs::read_to_string(
        matrix_path(&format!("{BATCH}.toml"))
            .parent()
            .unwrap()
            .join(format!("../../claude/matrices/{BATCH}.toml")),
    )
    .unwrap();
    fs::write(strategy(root.path()), &claude).unwrap();
    let unaffected = provider
        .project_strategy_assignments(target(BATCH))
        .unwrap();
    assert_eq!(unaffected.adapter, "claude");
    assert!(unaffected.roles.is_empty());
    assert!(
        provider
            .preview_strategy_assignments(request(unaffected, "unused"))
            .is_err()
    );
    assert_eq!(fs::read_to_string(strategy(root.path())).unwrap(), claude);
}
