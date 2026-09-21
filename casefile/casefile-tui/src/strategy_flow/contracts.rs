use super::{Flow, Node, Stage, node};
use crate::ui::safe_inline;
use casefile_core::StrategyProjection;
use casefile_store::{DerivedStrategy, StrategyBindingState};

// Execution edges belong to the shipped skill references, not to workers-array order.
pub(super) fn build(id: &str, phase: &str, strategy: &DerivedStrategy) -> Option<Flow> {
    let suffix = id.strip_prefix("casefile-")?;
    let (family, name) = suffix.split_once('-')?;
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
    let mut flow = Flow::default();
    match (family, name) {
        ("investigate", "solo") => flow.chain(vec![node(
            Stage::Root,
            format!(
                "{}\nRead-only investigation\nEvidence · duplicates · ticket disposition",
                safe_inline(&matrix.root_binding)
            ),
        )]),
        ("investigate", "atomic" | "inspector-tree") => {
            flow.row(vec![node(Stage::Assign, "Root · assign disjoint domains")]);
            let mut previous = Stage::Assign;
            if name == "inspector-tree" {
                flow.row(vec![worker(
                    strategy,
                    Stage::Inspector,
                    "inspector",
                    "Decompose domains",
                )]);
                flow.edge(previous, Stage::Inspector);
                previous = Stage::Inspector;
            }
            flow.row(vec![worker(
                strategy,
                Stage::Detective,
                "detective",
                "Independent questions · return evidence",
            )]);
            flow.edge(previous, Stage::Detective);
            if name == "inspector-tree" {
                flow.row(vec![node(
                    Stage::Reconcile,
                    "Inspectors · verify and recommend",
                )]);
                flow.edge(Stage::Detective, Stage::Reconcile);
                previous = Stage::Reconcile;
            } else {
                previous = Stage::Detective;
            }
            flow.row(vec![node(
                Stage::Root,
                "Root · verify evidence\nDuplicates · final ticket disposition",
            )]);
            flow.edge(previous, Stage::Root);
        }
        ("review", "dialogue") => {
            flow.chain(vec![
                node(Stage::Assign, "Root · assign ticket batch"),
                worker(
                    strategy,
                    Stage::Chair,
                    "dialogue-review-chair",
                    "Inspect independently · spawn challenger",
                ),
                worker(
                    strategy,
                    Stage::Challenger,
                    "dialogue-review-challenger",
                    "Inspect independently",
                ),
            ]);
            flow.row(vec![node(
                Stage::Reconcile,
                "Chair + challenger\nReconcile · at most two rounds\nJoint verdict",
            )]);
            flow.edge(Stage::Chair, Stage::Reconcile);
            flow.edge(Stage::Challenger, Stage::Reconcile);
            flow.row(vec![node(Stage::Root, "Root · reconcile findings")]);
            flow.edge(Stage::Reconcile, Stage::Root);
            review_routes(&mut flow);
        }
        ("review", "atomic" | "two-stage") => {
            flow.chain(vec![
                node(Stage::Assign, "Root · assign ticket groups"),
                worker(
                    strategy,
                    Stage::Primary,
                    "atomic-ticket-reviewer",
                    "Independent review · evidence only",
                ),
            ]);
            review(&mut flow, strategy);
            review_routes(&mut flow);
        }
        ("implement", _) => implementation(&mut flow, strategy, name),
        _ => unreachable!("recognized contracts were checked above"),
    }
    Some(flow)
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

fn worker(strategy: &DerivedStrategy, stage: Stage, role: &str, task: &str) -> Node {
    let worker = strategy
        .matrix
        .workers
        .iter()
        .find(|worker| worker.role == role)
        .expect("compatible role");
    let mut label = format!("{}\n{task}", safe_inline(&role.replace('-', " ")));
    if worker.maximum_count > 1 {
        label.push_str(&format!(
            "\n{}..{} workers · capacity batches",
            worker.minimum_count, worker.maximum_count
        ));
    }
    let runtime = if role == "implementation-writer" {
        match &strategy.binding {
            Some(
                StrategyBindingState::Absent { effective }
                | StrategyBindingState::Resolved { effective },
            ) => Some(format!(
                "{} / {}",
                effective.model, effective.reasoning_effort
            )),
            Some(StrategyBindingState::Pending) => Some("Binding pending".into()),
            Some(StrategyBindingState::Unresolved) => Some("Binding unresolved".into()),
            Some(StrategyBindingState::Invalid) => Some("Binding invalid".into()),
            None => None,
        }
    } else {
        worker
            .model
            .as_ref()
            .zip(worker.reasoning_effort.as_ref())
            .map(|(model, effort)| format!("{model} / {effort}"))
    };
    if let Some(runtime) = runtime {
        label.push('\n');
        label.push_str(&safe_inline(&runtime));
    }
    node(stage, label)
}

fn review(flow: &mut Flow, strategy: &DerivedStrategy) {
    let mut previous = Stage::Primary;
    if strategy
        .matrix
        .workers
        .iter()
        .any(|worker| worker.role == "verification-reviewer")
    {
        flow.row(vec![worker(
            strategy,
            Stage::Verifier,
            "verification-reviewer",
            "Check primary findings",
        )]);
        flow.edge(previous, Stage::Verifier);
        previous = Stage::Verifier;
    }
    flow.row(vec![node(Stage::Root, "Root · classify findings")]);
    flow.edge(previous, Stage::Root);
}

fn review_routes(flow: &mut Flow) {
    flow.row(vec![
        node(Stage::Correct, "Corrections\nRoot routes to ticket author"),
        node(Stage::Accepted, "Accepted\nRoot records disposition"),
    ]);
    flow.row(vec![node(
        Stage::Human,
        "Contention\nHuman · resolve scope",
    )]);
    for to in [Stage::Correct, Stage::Accepted, Stage::Human] {
        flow.edge(Stage::Root, to);
    }
}

fn implementation(flow: &mut Flow, strategy: &DerivedStrategy, name: &str) {
    let lookahead = name != "ticket-batch";
    let pipeline = name == "pipeline";
    flow.row(vec![node(Stage::Assign, "Root · assign accepted batch")]);
    let mut writers = vec![worker(
        strategy,
        Stage::Writer,
        "implementation-writer",
        "Batch N · exclusive writes",
    )];
    if lookahead {
        writers.push(worker(
            strategy,
            Stage::LookAhead,
            "look-ahead-investigator",
            "Optional N+1 preflight\nRead-only",
        ));
        flow.edge(Stage::Assign, Stage::LookAhead);
    }
    flow.row(writers);
    flow.edge(Stage::Assign, Stage::Writer);
    flow.row(vec![worker(
        strategy,
        Stage::Primary,
        "atomic-ticket-reviewer",
        "Review immutable commit N",
    )]);
    flow.edge(Stage::Writer, Stage::Primary);
    if pipeline {
        flow.rows.last_mut().expect("review row").push(node(
            Stage::Independence,
            "Root · overlap gate\nAfter immutable commit N\nIndependent tickets + disjoint paths",
        ));
        flow.edge(Stage::Writer, Stage::Independence);
        flow.row(vec![node(Stage::NextWriter, "If safe: same writer · N+1\nDuring exact-commit review N\nOtherwise: serial\nNo N+2 before N accepted")]);
        flow.edge(Stage::Independence, Stage::NextWriter);
    }
    if lookahead {
        flow.rows
            .last_mut()
            .expect("review or next-writer row")
            .push(node(
                Stage::AdvisoryReceipt,
                "Root · advisory receipt\nMay discard or repeat",
            ));
        flow.edge(Stage::LookAhead, Stage::AdvisoryReceipt);
    }
    review(flow, strategy);
    flow.row(vec![
        node(
            Stage::Correct,
            if pipeline {
                "Correction\nPreempt N+1 · same writer"
            } else {
                "Correction\nSame writer"
            },
        ),
        node(Stage::Accepted, "Accepted\nComplete N · next batch"),
    ]);
    flow.row(vec![node(
        Stage::Human,
        "Contention / repeated concern\nHuman · resolve scope\nMutation stopped",
    )]);
    for to in [Stage::Correct, Stage::Accepted, Stage::Human] {
        flow.edge(Stage::Root, to);
    }
    flow.edge(Stage::Correct, Stage::Writer);
}
