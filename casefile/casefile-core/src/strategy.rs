use crate::{SCHEMA_VERSION, diagnostic::Diagnostic, record::RecordSummary};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyProjection {
    pub root_binding: String,
    pub limits: StrategyLimits,
    pub requirements: StrategyRequirements,
    pub workers: Vec<StrategyWorker>,
    pub coordination: StrategyCoordination,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyLimits {
    pub max_concurrent_subagents: u64,
    pub max_depth: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyRequirements {
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyWorker {
    pub role: String,
    pub platform_profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    pub minimum_count: u64,
    pub maximum_count: u64,
    pub can_spawn_subagents: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyCoordination {
    pub batch_when_capacity_exceeded: bool,
    pub candidate_review_before_ticket: bool,
    pub shared_ticket_storage_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<StrategyPipeline>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyPipeline {
    pub maximum_active_tickets: u64,
    pub look_ahead_read_only: bool,
    pub require_dependency_independence: bool,
    pub require_disjoint_write_paths: bool,
    pub immutable_review_commits: bool,
    pub corrections_preempt_forward_work: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyBinding {
    pub adapter: String,
    pub role: String,
    pub model: String,
    pub reasoning_effort: String,
    pub resolution: BindingResolution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BindingResolution {
    pub mode: String,
    pub value: String,
}

pub fn parse(path: &str, text: &str) -> Result<RecordSummary, Vec<Diagnostic>> {
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = table(path, &value, "strategy")?;
    schema(path, table)?;
    let phase = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".toml"))
        .unwrap_or_default();
    let parsed_phase = string(path, table, "phase", "invalid_strategy")?;
    if parsed_phase != phase {
        return Err(vec![
            Diagnostic::new(path, "strategy_phase", "phase must match filename").field("phase"),
        ]);
    }
    if [
        "orchestrator",
        "limits",
        "requirements",
        "workers",
        "coordination",
    ]
    .iter()
    .any(|key| table.contains_key(*key))
    {
        parse_projection_table(path, table)?;
    }
    Ok(RecordSummary::Strategy {
        strategy_id: string(path, table, "strategy_id", "invalid_strategy")?,
        phase: parsed_phase,
        adapter: string(path, table, "adapter", "invalid_strategy")?,
    })
}

pub fn parse_projection(
    path: &str,
    text: &str,
) -> Result<Option<StrategyProjection>, Vec<Diagnostic>> {
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = table(path, &value, "strategy")?;
    schema(path, table)?;
    if [
        "orchestrator",
        "limits",
        "requirements",
        "workers",
        "coordination",
    ]
    .iter()
    .any(|key| table.contains_key(*key))
    {
        parse_projection_table(path, table).map(Some)
    } else {
        Ok(None)
    }
}

pub fn parse_binding(path: &str, text: &str) -> Result<RecordSummary, Vec<Diagnostic>> {
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = table(path, &value, "strategy binding")?;
    schema(path, table)?;
    let role = string(path, table, "role", "invalid_strategy_binding")?;
    if role != "implementation-writer" {
        return Err(vec![
            Diagnostic::new(path, "binding_role", "role must be implementation-writer")
                .field("role"),
        ]);
    }
    let resolution = table
        .get("resolution")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            vec![
                Diagnostic::new(
                    path,
                    "invalid_strategy_binding",
                    "resolution must be a TOML table",
                )
                .field("resolution"),
            ]
        })?;
    Ok(RecordSummary::StrategyBinding {
        binding: StrategyBinding {
            adapter: string(path, table, "adapter", "invalid_strategy_binding")?,
            role,
            model: string(path, table, "model", "invalid_strategy_binding")?,
            reasoning_effort: string(path, table, "reasoning_effort", "invalid_strategy_binding")?,
            resolution: BindingResolution {
                mode: string(path, resolution, "mode", "invalid_strategy_binding")?,
                value: string(path, resolution, "value", "invalid_strategy_binding")?,
            },
        },
    })
}

fn parse_projection_table(
    path: &str,
    root: &toml::map::Map<String, toml::Value>,
) -> Result<StrategyProjection, Vec<Diagnostic>> {
    let orchestrator = required_table(path, root, "orchestrator", "invalid_strategy")?;
    let limits = required_table(path, root, "limits", "invalid_strategy")?;
    let requirements = required_table(path, root, "requirements", "invalid_strategy")?;
    let coordination = required_table(path, root, "coordination", "invalid_strategy")?;
    let max_concurrent_subagents = positive(path, limits, "max_concurrent_subagents")?;
    let max_depth = integer(path, limits, "max_depth")?;
    let capabilities = requirements
        .get("capabilities")
        .and_then(toml::Value::as_array)
        .filter(|values| {
            values
                .iter()
                .all(|value| value.as_str().is_some_and(|value| !value.is_empty()))
        })
        .map(|values| {
            values
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .ok_or_else(|| {
            vec![
                Diagnostic::new(
                    path,
                    "invalid_strategy",
                    "capabilities must be a string array",
                )
                .field("requirements.capabilities"),
            ]
        })?;
    let workers = match root.get("workers") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| {
                vec![
                    Diagnostic::new(path, "invalid_strategy", "workers must be an array")
                        .field("workers"),
                ]
            })?
            .iter()
            .enumerate()
            .map(|(index, value)| parse_worker(path, value, index, max_depth))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let minimum_total = workers
        .iter()
        .map(|worker| worker.minimum_count)
        .sum::<u64>();
    if minimum_total > max_concurrent_subagents {
        return Err(vec![Diagnostic::new(
            path,
            "strategy_capacity",
            "worker minima exceed max_concurrent_subagents",
        )]);
    }
    let root_binding = required_string(path, orchestrator, "binding", "invalid_strategy")?;
    if root_binding != "root" {
        return Err(vec![
            Diagnostic::new(path, "strategy_root", "orchestrator binding must be root")
                .field("orchestrator.binding"),
        ]);
    }
    let pipeline = coordination
        .get("pipeline")
        .map(|value| parse_pipeline(path, value))
        .transpose()?;
    Ok(StrategyProjection {
        root_binding,
        limits: StrategyLimits {
            max_concurrent_subagents,
            max_depth,
        },
        requirements: StrategyRequirements { capabilities },
        workers,
        coordination: StrategyCoordination {
            batch_when_capacity_exceeded: boolean(
                path,
                coordination,
                "batch_when_capacity_exceeded",
            )?,
            candidate_review_before_ticket: boolean(
                path,
                coordination,
                "candidate_review_before_ticket",
            )?,
            shared_ticket_storage_required: boolean(
                path,
                coordination,
                "shared_ticket_storage_required",
            )?,
            pipeline,
        },
    })
}

fn parse_worker(
    path: &str,
    value: &toml::Value,
    index: usize,
    depth: u64,
) -> Result<StrategyWorker, Vec<Diagnostic>> {
    let table = value.as_table().ok_or_else(|| {
        vec![
            Diagnostic::new(path, "invalid_strategy", "worker must be a TOML table")
                .field(&format!("workers.{index}")),
        ]
    })?;
    let minimum_count = positive(path, table, "minimum_count")?;
    let maximum_count = positive(path, table, "maximum_count")?;
    if minimum_count > maximum_count {
        return Err(vec![
            Diagnostic::new(
                path,
                "invalid_strategy",
                "worker minimum_count must not exceed maximum_count",
            )
            .field(&format!("workers.{index}")),
        ]);
    }
    let can_spawn_subagents = boolean(path, table, "can_spawn_subagents")?;
    if can_spawn_subagents && depth < 2 {
        return Err(vec![
            Diagnostic::new(
                path,
                "strategy_depth",
                "spawning worker requires max_depth of at least 2",
            )
            .field(&format!("workers.{index}.can_spawn_subagents")),
        ]);
    }
    let model = optional_string(path, table, "model", "invalid_strategy")?;
    let reasoning_effort = optional_string(path, table, "reasoning", "invalid_strategy")?;
    if model.is_some() != reasoning_effort.is_some() {
        return Err(vec![
            Diagnostic::new(
                path,
                "invalid_strategy",
                "worker model and reasoning must be supplied together",
            )
            .field(&format!("workers.{index}")),
        ]);
    }
    Ok(StrategyWorker {
        role: required_string(path, table, "role", "invalid_strategy")?,
        platform_profile: required_string(path, table, "platform_profile", "invalid_strategy")?,
        model,
        reasoning_effort,
        minimum_count,
        maximum_count,
        can_spawn_subagents,
    })
}

fn parse_pipeline(path: &str, value: &toml::Value) -> Result<StrategyPipeline, Vec<Diagnostic>> {
    let table = value.as_table().ok_or_else(|| {
        vec![
            Diagnostic::new(path, "invalid_strategy", "pipeline must be a TOML table")
                .field("coordination.pipeline"),
        ]
    })?;
    Ok(StrategyPipeline {
        maximum_active_tickets: positive(path, table, "maximum_active_tickets")?,
        look_ahead_read_only: boolean(path, table, "look_ahead_read_only")?,
        require_dependency_independence: boolean(path, table, "require_dependency_independence")?,
        require_disjoint_write_paths: boolean(path, table, "require_disjoint_write_paths")?,
        immutable_review_commits: boolean(path, table, "immutable_review_commits")?,
        corrections_preempt_forward_work: boolean(path, table, "corrections_preempt_forward_work")?,
    })
}

fn table<'a>(
    path: &str,
    value: &'a toml::Value,
    name: &str,
) -> Result<&'a toml::map::Map<String, toml::Value>, Vec<Diagnostic>> {
    value.as_table().ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_strategy",
            format!("{name} must be a TOML table"),
        )]
    })
}
fn required_table<'a>(
    path: &str,
    table: &'a toml::map::Map<String, toml::Value>,
    name: &str,
    code: &str,
) -> Result<&'a toml::map::Map<String, toml::Value>, Vec<Diagnostic>> {
    table
        .get(name)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            vec![Diagnostic::new(path, code, format!("{name} must be a TOML table")).field(name)]
        })
}
fn string(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
    code: &str,
) -> Result<String, Vec<Diagnostic>> {
    required_string(path, table, name, code)
}
fn required_string(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
    code: &str,
) -> Result<String, Vec<Diagnostic>> {
    table
        .get(name)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            vec![Diagnostic::new(path, code, "field must be a non-empty string").field(name)]
        })
}
fn optional_string(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
    code: &str,
) -> Result<Option<String>, Vec<Diagnostic>> {
    match table.get(name) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|value| Some(value.into()))
            .ok_or_else(|| {
                vec![Diagnostic::new(path, code, "field must be a non-empty string").field(name)]
            }),
    }
}
fn integer(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
) -> Result<u64, Vec<Diagnostic>> {
    table
        .get(name)
        .and_then(toml::Value::as_integer)
        .filter(|value| *value >= 0)
        .map(|value| value as u64)
        .ok_or_else(|| {
            vec![
                Diagnostic::new(
                    path,
                    "invalid_strategy",
                    "field must be a non-negative integer",
                )
                .field(name),
            ]
        })
}
fn positive(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
) -> Result<u64, Vec<Diagnostic>> {
    integer(path, table, name).and_then(|value| {
        if value > 0 {
            Ok(value)
        } else {
            Err(vec![
                Diagnostic::new(path, "invalid_strategy", "field must be a positive integer")
                    .field(name),
            ])
        }
    })
}
fn boolean(
    path: &str,
    table: &toml::map::Map<String, toml::Value>,
    name: &str,
) -> Result<bool, Vec<Diagnostic>> {
    table
        .get(name)
        .and_then(toml::Value::as_bool)
        .ok_or_else(|| {
            vec![Diagnostic::new(path, "invalid_strategy", "field must be a boolean").field(name)]
        })
}
fn schema(path: &str, table: &toml::map::Map<String, toml::Value>) -> Result<(), Vec<Diagnostic>> {
    if table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        == Some(i64::from(SCHEMA_VERSION))
    {
        Ok(())
    } else {
        Err(vec![Diagnostic::new(
            path,
            "invalid_schema_version",
            "schema_version must be 1",
        )])
    }
}

/// Validates a complete selectable matrix without relying on its filesystem name.
/// Workflow selection uses this authority before copying the matrix into its selected path.
pub fn validate_matrix(text: &str) -> Result<(), Vec<Diagnostic>> {
    let path = "strategy matrix";
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = table(path, &value, "strategy")?;
    schema(path, table)?;
    let strategy_id = string(path, table, "strategy_id", "invalid_strategy")?;
    if !Regex::new(r"^[a-z0-9][a-z0-9-]*$")
        .expect("fixed expression")
        .is_match(&strategy_id)
    {
        return Err(vec![
            Diagnostic::new(path, "invalid_strategy", "strategy_id is invalid")
                .field("strategy_id"),
        ]);
    }
    let phase = string(path, table, "phase", "invalid_strategy")?;
    if !matches!(
        phase.as_str(),
        "planning" | "investigation" | "review" | "implementation" | "closeout"
    ) {
        return Err(vec![
            Diagnostic::new(path, "strategy_phase", "phase is not supported").field("phase"),
        ]);
    }
    string(path, table, "adapter", "invalid_strategy")?;
    parse_projection_table(path, table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, parse_binding, parse_projection, validate_matrix};
    use crate::RecordSummary;

    const COMPLETE: &str = r#"schema_version = 1
strategy_id = "casefile-implement-ticket-batch"
phase = "implementation"
adapter = "codex"
[orchestrator]
binding = "root"
[limits]
max_concurrent_subagents = 1
max_depth = 1
[requirements]
capabilities = ["subagents"]
[[workers]]
role = "implementation-writer"
platform_profile = "writer"
model = "gpt-5.6-sol"
reasoning = "high"
minimum_count = 1
maximum_count = 1
can_spawn_subagents = false
[coordination]
batch_when_capacity_exceeded = true
candidate_review_before_ticket = false
shared_ticket_storage_required = true
"#;

    #[test]
    fn complete_and_legacy_strategies_are_both_governed_shapes() {
        let path = "strategy/implementation.toml";
        assert!(matches!(
            parse(path, COMPLETE),
            Ok(RecordSummary::Strategy { .. })
        ));
        let projection = parse_projection(path, COMPLETE)
            .expect("projection")
            .expect("complete projection");
        assert_eq!("root", projection.root_binding);
        assert_eq!("implementation-writer", projection.workers[0].role);
        let legacy = "schema_version = 1\nstrategy_id = 'legacy'\nphase = 'implementation'\nadapter = 'codex'\n";
        assert!(matches!(
            parse(path, legacy),
            Ok(RecordSummary::Strategy { .. })
        ));
        assert_eq!(None, parse_projection(path, legacy).expect("legacy"));
    }

    #[test]
    fn workflow_matrix_validation_rejects_incomplete_pipeline() {
        assert!(validate_matrix(COMPLETE).is_ok());
        assert!(
            validate_matrix(
                &(String::from(COMPLETE)
                    + "\n[coordination.pipeline]\nmaximum_active_tickets = 1\n")
            )
            .is_err()
        );
    }

    #[test]
    fn binding_requires_the_writer_role_and_resolution_metadata() {
        let binding = "schema_version = 1\nadapter = 'codex'\nrole = 'implementation-writer'\nmodel = 'gpt-5.6-sol'\nreasoning_effort = 'high'\n[resolution]\nmode = 'profile'\nvalue = 'writer'\n";
        assert!(matches!(
            parse_binding("strategy/bindings.toml", binding),
            Ok(RecordSummary::StrategyBinding { .. })
        ));
        let invalid = binding.replace("implementation-writer", "reviewer");
        assert!(parse_binding("strategy/bindings.toml", &invalid).is_err());
        assert!(
            parse_binding(
                "strategy/bindings.toml",
                &binding.replace("value = 'writer'", "")
            )
            .is_err()
        );
    }
}
