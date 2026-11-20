use crate::{Diagnostic, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentPhase {
    Investigation,
    Review,
    Implementation,
}

impl fmt::Display for AssignmentPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Investigation => "investigation",
            Self::Review => "review",
            Self::Implementation => "implementation",
        })
    }
}

impl FromStr for AssignmentPhase {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "investigation" => Ok(Self::Investigation),
            "review" => Ok(Self::Review),
            "implementation" => Ok(Self::Implementation),
            _ => Err("assignment phase must be investigation, review, or implementation"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleAssignment {
    pub role: String,
    pub model: String,
    pub reasoning_effort: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyAssignments {
    schema_version: u32,
    adapter: String,
    pub phase: AssignmentPhase,
    pub strategy_id: String,
    pub roles: Vec<RoleAssignment>,
}

impl StrategyAssignments {
    pub fn codex(
        phase: AssignmentPhase,
        strategy_id: String,
        mut roles: Vec<RoleAssignment>,
    ) -> Self {
        roles.sort_by(|a, b| a.role.cmp(&b.role));
        Self {
            schema_version: SCHEMA_VERSION,
            adapter: "codex".into(),
            phase,
            strategy_id,
            roles,
        }
    }
}

pub fn parse_strategy_assignments(
    path: &str,
    text: &str,
) -> Result<StrategyAssignments, Vec<Diagnostic>> {
    let invalid = |message| {
        vec![Diagnostic::new(
            path,
            "invalid_strategy_assignments",
            message,
        )]
    };
    let record: StrategyAssignments = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    if record.schema_version != SCHEMA_VERSION || record.adapter != "codex" {
        return Err(invalid(
            "strategy assignments require schema version 1 and adapter codex",
        ));
    }
    let suffix = format!(
        "strategy/assignments/{}/{}.toml",
        record.phase, record.strategy_id
    );
    if !crate::strategy::is_strategy_id(&record.strategy_id)
        || !(path == suffix || path.ends_with(&format!("/{suffix}")))
    {
        return Err(invalid(
            "assignment phase and strategy ID must match the path",
        ));
    }
    let mut roles = BTreeSet::new();
    for row in &record.roles {
        if [&row.role, &row.model, &row.reasoning_effort]
            .iter()
            .any(|value| {
                value.is_empty() || value.trim() != *value || value.chars().any(char::is_control)
            })
        {
            return Err(invalid(
                "role, model, and reasoning effort must be exact non-empty strings",
            ));
        }
        if !roles.insert(&row.role) {
            return Err(invalid("each role must have one assignment"));
        }
    }
    Ok(record)
}
