use super::*;
use casefile_store::{StrategyAssignmentRequest, StrategyAssignmentTarget};

pub(super) fn dispatch(provider: &Provider, name: &str, arguments: Value) -> Result<Value> {
    match name {
        "casefile_project_strategy_assignments" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Arguments {
                target: StrategyAssignmentTarget,
            }
            serialize(provider.project_strategy_assignments(parse::<Arguments>(arguments)?.target)?)
        }
        "casefile_preview_strategy_assignments" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Arguments {
                request: StrategyAssignmentRequest,
            }
            review_envelope(
                &provider.preview_strategy_assignments(parse::<Arguments>(arguments)?.request)?,
            )
        }
        "casefile_apply_strategy_assignments" => {
            serialize(provider.apply_strategy_assignments(&preview_id(arguments)?)?)
        }
        _ => unreachable!("strategy assignment tools are selected by their caller"),
    }
}

pub(super) fn definitions() -> [Value; 3] {
    [
        tool(
            "casefile_project_strategy_assignments",
            "Read explicit model and effort selections for one exact selected strategy in an activated investigation.",
            object_schema(json!({"target": target_schema()}), &["target"]),
        ),
        tool(
            "casefile_preview_strategy_assignments",
            "Preview all explicit role assignments for one selected Codex strategy. Use the exact target and revisions underlying the confirmed choices.",
            object_schema(json!({"request": request_schema()}), &["request"]),
        ),
        tool(
            "casefile_apply_strategy_assignments",
            "Apply a confirmed set of strategy assignments using an exact preview ID from this MCP session.",
            apply_schema(),
        ),
    ]
}

fn target_schema() -> Value {
    object_schema(
        json!({
            "investigation": non_empty_string(),
            "phase": {"type": "string", "enum": ["investigation", "review", "implementation"]},
            "strategy_id": non_empty_string(),
        }),
        &["investigation", "phase", "strategy_id"],
    )
}

fn revisions_schema() -> Value {
    object_schema(
        json!({
            "matrix": non_empty_string(),
            "assignments": nullable(non_empty_string()),
            "legacy_writer_binding": nullable(non_empty_string()),
        }),
        &["matrix", "assignments", "legacy_writer_binding"],
    )
}

fn request_schema() -> Value {
    let role = object_schema(
        json!({
            "role": non_empty_string(),
            "model": non_empty_string(),
            "reasoning_effort": non_empty_string(),
        }),
        &["role", "model", "reasoning_effort"],
    );
    object_schema(
        json!({
            "target": target_schema(),
            "expected_revisions": revisions_schema(),
            "roles": {"type": "array", "items": role},
        }),
        &["target", "expected_revisions", "roles"],
    )
}

pub(super) fn projection_schema() -> Value {
    let selection = object_schema(
        json!({
            "model": non_empty_string(),
            "reasoning_effort": non_empty_string(),
            "source": {"type": "string", "enum": ["strategy_assignment", "legacy_writer_binding"]},
        }),
        &["model", "reasoning_effort", "source"],
    );
    let role = object_schema(
        json!({
            "platform_profiles": string_array(),
            "selection": nullable(selection),
        }),
        &["platform_profiles", "selection"],
    );
    object_schema(
        json!({
            "target": target_schema(),
            "adapter": non_empty_string(),
            "revisions": revisions_schema(),
            "roles": {"type": "object", "additionalProperties": role},
        }),
        &["target", "adapter", "revisions", "roles"],
    )
}
