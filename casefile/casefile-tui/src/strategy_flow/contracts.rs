use super::{Flow, Runtime, Stage, Treatment};
use casefile_core::StrategyProjection;
use casefile_store::{DerivedStrategy, StrategyBindingState};

// The shipped skill contracts supply sequencing; worker-array order never does.
pub(super) fn build(id: &str, phase: &str, strategy: &DerivedStrategy) -> Option<Flow> {
    let (family, name) = id.strip_prefix("casefile-")?.split_once('-')?;
    if !matches!(
        (family, phase),
        ("investigate", "investigation") | ("review", "review") | ("implement", "implementation")
    ) {
        return None;
    }
    let (required, optional): (&[&str], &[&str]) = match (family, name) {
        ("investigate", "solo") => (&[], &[]),
        ("investigate", "atomic") => (&["detective"], &[]),
        ("investigate", "inspector-tree") => (&["inspector", "detective"], &[]),
        ("review", "atomic") => (&["atomic-ticket-reviewer"], &[]),
        ("review", "dialogue") => (
            &["dialogue-review-chair", "dialogue-review-challenger"],
            &[],
        ),
        ("review", "two-stage") => (&["atomic-ticket-reviewer", "verification-reviewer"], &[]),
        ("implement", "ticket-batch") => (
            &["implementation-writer", "atomic-ticket-reviewer"],
            &["verification-reviewer"],
        ),
        ("implement", "ticket-batch-look-ahead" | "pipeline") => (
            &[
                "implementation-writer",
                "atomic-ticket-reviewer",
                "look-ahead-investigator",
            ],
            &["verification-reviewer"],
        ),
        _ => return None,
    };
    let matrix = &strategy.matrix;
    if !compatible(matrix, required, optional, name == "pipeline")
        || !compatible_guarantees(matrix, family)
    {
        return None;
    }
    let verified = matrix
        .workers
        .iter()
        .any(|worker| worker.role == "verification-reviewer");
    let (title, mut main, treatment) = match (family, name) {
        ("investigate", "solo") => ("Solo", vec![Stage::Investigate], Treatment::Linear),
        ("investigate", "atomic") => (
            "Atomic investigation",
            vec![Stage::Detectives, Stage::Assess, Stage::Tickets],
            Treatment::Linear,
        ),
        ("investigate", "inspector-tree") => (
            "Inspector tree",
            vec![
                Stage::Inspectors,
                Stage::Detectives,
                Stage::Assess,
                Stage::Tickets,
            ],
            Treatment::Linear,
        ),
        ("review", "atomic") => ("Atomic review", vec![Stage::Review], Treatment::Review),
        ("review", "two-stage") => ("Two-stage review", vec![Stage::Review], Treatment::Review),
        ("review", "dialogue") => (
            "Dialogue review",
            vec![Stage::Chair, Stage::Reconcile, Stage::Done],
            Treatment::Dialogue,
        ),
        ("implement", "ticket-batch") => (
            "Ticket batch",
            vec![Stage::Implement, Stage::Review],
            Treatment::Correction,
        ),
        ("implement", "ticket-batch-look-ahead") => (
            "Look-ahead",
            vec![Stage::Implement, Stage::Review],
            Treatment::Correction,
        ),
        ("implement", "pipeline") => (
            "Pipeline",
            vec![Stage::Implement, Stage::Review],
            Treatment::Pipeline,
        ),
        _ => unreachable!("recognized contract"),
    };
    if matches!(
        treatment,
        Treatment::Review | Treatment::Correction | Treatment::Pipeline
    ) {
        if verified {
            main.push(Stage::Verify);
        }
        main.push(Stage::Done);
    }
    let mut models = Vec::new();
    for (stage, role) in [
        (Stage::Detectives, "detective"),
        (Stage::Inspectors, "inspector"),
        (Stage::Review, "atomic-ticket-reviewer"),
        (Stage::Verify, "verification-reviewer"),
        (Stage::Chair, "dialogue-review-chair"),
        (Stage::Challenger, "dialogue-review-challenger"),
        (Stage::Preflight, "look-ahead-investigator"),
    ] {
        if let Some(worker) = matrix.workers.iter().find(|worker| worker.role == role) {
            let model = worker
                .model
                .as_ref()
                .map_or(Runtime::Unavailable, |value| Runtime::Model(value.clone()));
            if stage == Stage::Inspectors {
                models.push((Stage::Assess, model.clone()));
            }
            models.push((stage, model));
        }
    }
    if family == "implement" {
        let model = match &strategy.binding {
            Some(
                StrategyBindingState::Resolved { effective }
                | StrategyBindingState::Absent { effective },
            ) => Runtime::Model(effective.model.clone()),
            Some(StrategyBindingState::Pending) => Runtime::Pending,
            Some(StrategyBindingState::Invalid | StrategyBindingState::Unresolved) | None => {
                Runtime::Unavailable
            }
        };
        models.push((Stage::Implement, model));
    }
    Some(Flow {
        title,
        main,
        treatment,
        preflight: family == "implement" && name != "ticket-batch",
        models,
    })
}

fn compatible(
    matrix: &StrategyProjection,
    required: &[&str],
    optional: &[&str],
    pipeline: bool,
) -> bool {
    let workers = &matrix.workers;
    if required
        .iter()
        .any(|role| !workers.iter().any(|worker| worker.role == *role))
    {
        return false;
    }
    for (index, worker) in workers.iter().enumerate() {
        if !required.contains(&worker.role.as_str()) && !optional.contains(&worker.role.as_str())
            || workers[..index]
                .iter()
                .any(|other| other.role == worker.role)
            || (matches!(
                worker.role.as_str(),
                "implementation-writer"
                    | "look-ahead-investigator"
                    | "verification-reviewer"
                    | "dialogue-review-chair"
                    | "dialogue-review-challenger"
            ) && worker.maximum_count != 1)
            || worker.minimum_count == 0
            || worker.maximum_count == 0
            || worker.can_spawn_subagents
                != matches!(worker.role.as_str(), "inspector" | "dialogue-review-chair")
        {
            return false;
        }
    }
    if pipeline {
        matrix.coordination.pipeline.as_ref().is_some_and(|p| {
            p.maximum_active_tickets == 2
                && p.look_ahead_read_only
                && p.require_dependency_independence
                && p.require_disjoint_write_paths
                && p.immutable_review_commits
                && p.corrections_preempt_forward_work
        })
    } else {
        matrix.coordination.pipeline.is_none()
    }
}

fn compatible_guarantees(matrix: &StrategyProjection, family: &str) -> bool {
    let has = |capability| {
        matrix
            .requirements
            .capabilities
            .iter()
            .any(|value| value == capability)
    };
    if !matrix.workers.is_empty() && !has("subagents") {
        return false;
    }
    if matrix
        .workers
        .iter()
        .any(|worker| worker.can_spawn_subagents)
        && (!has("nested_subagents") || matrix.limits.max_depth < 2)
    {
        return false;
    }
    if matrix.workers.iter().any(|worker| worker.maximum_count > 1)
        && !matrix.coordination.batch_when_capacity_exceeded
    {
        return false;
    }
    match family {
        "investigate" => matrix.coordination.candidate_review_before_ticket,
        "implement" => has("exclusive_writer"),
        "review" => true,
        _ => unreachable!("recognized strategy family"),
    }
}
