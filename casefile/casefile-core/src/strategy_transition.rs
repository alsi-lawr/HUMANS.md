use std::collections::BTreeSet;

use regex::Regex;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{Diagnostic, Revision, SCHEMA_VERSION};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveOwnership {
    pub owner: String,
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyTransitionRecord {
    pub operation_id: String,
    pub recorded_at: String,
    pub phase: String,
    pub previous_strategy_id: String,
    pub selected_strategy_id: String,
    pub selected_matrix_origin: String,
    pub selected_matrix_sha256: String,
    pub expected_store_revision: Revision,
    pub expected_matrix_revision: Revision,
    pub proposed_matrix_revision: Revision,
    pub root_binding: String,
    pub governed_state_updated: bool,
    pub rationale: String,
    pub available_capabilities: Vec<String>,
    pub preserved_work_paths: Vec<String>,
    pub active_ownership: Vec<ActiveOwnership>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionWire {
    schema_version: i64,
    operation_id: String,
    recorded_at: String,
    phase: String,
    previous_strategy_id: String,
    selected_strategy_id: String,
    selected_matrix_origin: String,
    selected_matrix_sha256: String,
    expected_store_revision: Revision,
    expected_matrix_revision: Revision,
    proposed_matrix_revision: Revision,
    root_binding: String,
    governed_state_updated: bool,
    rationale: String,
    available_capabilities: Vec<String>,
    preserved_work_paths: Vec<String>,
    #[serde(default)]
    active_ownership: Vec<ActiveOwnership>,
}

pub fn parse_strategy_transition(
    path: &str,
    text: &str,
) -> Result<StrategyTransitionRecord, Vec<Diagnostic>> {
    let wire: TransitionWire = toml::from_str(text).map_err(|error| {
        vec![Diagnostic::new(
            path,
            "invalid_strategy_transition",
            error.to_string(),
        )]
    })?;
    if wire.schema_version != i64::from(SCHEMA_VERSION) {
        return Err(vec![Diagnostic::new(
            path,
            "invalid_schema_version",
            "schema_version must be 1",
        )]);
    }
    let record = StrategyTransitionRecord {
        operation_id: wire.operation_id,
        recorded_at: wire.recorded_at,
        phase: wire.phase,
        previous_strategy_id: wire.previous_strategy_id,
        selected_strategy_id: wire.selected_strategy_id,
        selected_matrix_origin: wire.selected_matrix_origin,
        selected_matrix_sha256: wire.selected_matrix_sha256,
        expected_store_revision: wire.expected_store_revision,
        expected_matrix_revision: wire.expected_matrix_revision,
        proposed_matrix_revision: wire.proposed_matrix_revision,
        root_binding: wire.root_binding,
        governed_state_updated: wire.governed_state_updated,
        rationale: wire.rationale,
        available_capabilities: wire.available_capabilities,
        preserved_work_paths: wire.preserved_work_paths,
        active_ownership: wire.active_ownership,
    };
    validate_strategy_transition(path, &record).map_err(|diagnostic| vec![diagnostic])?;
    Ok(record)
}

#[allow(clippy::result_large_err)]
pub fn validate_strategy_transition(
    path: &str,
    record: &StrategyTransitionRecord,
) -> Result<(), Diagnostic> {
    let safe_id = Regex::new(r"^[a-z0-9][a-z0-9-]*$").expect("fixed expression");
    if !safe_id.is_match(&record.operation_id)
        || !safe_id.is_match(&record.previous_strategy_id)
        || !safe_id.is_match(&record.selected_strategy_id)
    {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition_identity",
            "operation and strategy identities must be lowercase hyphenated identifiers",
        ));
    }
    if OffsetDateTime::parse(&record.recorded_at, &Rfc3339).is_err() {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition_timestamp",
            "recorded_at must be RFC 3339",
        ));
    }
    let expected_name = format!(
        "{}-{}.toml",
        record
            .recorded_at
            .chars()
            .filter(|character| character.is_ascii_digit()
                || *character == 'T'
                || *character == 'Z')
            .collect::<String>(),
        record.operation_id
    );
    if path.rsplit('/').next() != Some(expected_name.as_str()) {
        return Err(Diagnostic::new(
            path,
            "strategy_transition_path",
            "transition filename must be deterministic from recorded_at and operation_id",
        ));
    }
    if !matches!(
        record.phase.as_str(),
        "planning" | "investigation" | "review" | "implementation" | "closeout"
    ) {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition_phase",
            "phase is not supported",
        ));
    }
    if record.selected_matrix_origin.trim().is_empty()
        || record.rationale.trim().is_empty()
        || record.expected_store_revision.0.trim().is_empty()
        || record.expected_matrix_revision.0.trim().is_empty()
        || record.proposed_matrix_revision.0.trim().is_empty()
    {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition",
            "origin, rationale, and revisions must be non-empty",
        ));
    }
    if record.root_binding != "root" || !record.governed_state_updated {
        return Err(Diagnostic::new(
            path,
            "strategy_transition_authority",
            "governed transitions must preserve the root binding and update governed state",
        ));
    }
    if record.selected_matrix_sha256.len() != 64
        || !record
            .selected_matrix_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition_digest",
            "selected_matrix_sha256 must be a lowercase SHA-256 digest",
        ));
    }
    let mut capabilities = BTreeSet::new();
    if record
        .available_capabilities
        .iter()
        .any(|value| value.trim().is_empty() || !capabilities.insert(value.as_str()))
    {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition_capabilities",
            "available capabilities must be non-empty and unique",
        ));
    }
    if record
        .preserved_work_paths
        .iter()
        .any(|path| !safe_path(path))
    {
        return Err(Diagnostic::new(
            path,
            "invalid_strategy_transition_work",
            "preserved work paths must be safe relative paths",
        ));
    }
    let mut claims = Vec::new();
    for ownership in &record.active_ownership {
        if ownership.owner.trim().is_empty()
            || ownership.paths.is_empty()
            || ownership.paths.iter().any(|path| !safe_path(path))
        {
            return Err(Diagnostic::new(
                path,
                "invalid_strategy_transition_ownership",
                "active ownership requires an owner and safe relative paths",
            ));
        }
        claims.extend(
            ownership
                .paths
                .iter()
                .map(|claimed| (ownership.owner.as_str(), claimed.as_str())),
        );
    }
    for (index, (owner, claimed)) in claims.iter().enumerate() {
        if claims[index + 1..]
            .iter()
            .any(|(other_owner, other)| owner != other_owner && paths_overlap(claimed, other))
        {
            return Err(Diagnostic::new(
                path,
                "overlapping_strategy_ownership",
                "active writers may not claim overlapping paths",
            ));
        }
    }
    Ok(())
}

pub fn render_strategy_transition(record: &StrategyTransitionRecord) -> String {
    let mut output = String::from("schema_version = 1\n");
    for (name, value) in [
        ("operation_id", record.operation_id.as_str()),
        ("recorded_at", record.recorded_at.as_str()),
        ("phase", record.phase.as_str()),
        ("previous_strategy_id", record.previous_strategy_id.as_str()),
        ("selected_strategy_id", record.selected_strategy_id.as_str()),
        (
            "selected_matrix_origin",
            record.selected_matrix_origin.as_str(),
        ),
        (
            "selected_matrix_sha256",
            record.selected_matrix_sha256.as_str(),
        ),
        (
            "expected_store_revision",
            record.expected_store_revision.0.as_str(),
        ),
        (
            "expected_matrix_revision",
            record.expected_matrix_revision.0.as_str(),
        ),
        (
            "proposed_matrix_revision",
            record.proposed_matrix_revision.0.as_str(),
        ),
        ("root_binding", record.root_binding.as_str()),
    ] {
        output.push_str(&format!("{name} = {}\n", toml::Value::String(value.into())));
    }
    output.push_str(&format!(
        "governed_state_updated = {}\n",
        record.governed_state_updated
    ));
    output.push_str(&format!(
        "rationale = {}\n",
        toml::Value::String(record.rationale.clone())
    ));
    output.push_str(&format!(
        "available_capabilities = {}\n",
        toml_array(&record.available_capabilities)
    ));
    output.push_str(&format!(
        "preserved_work_paths = {}\n",
        toml_array(&record.preserved_work_paths)
    ));
    for ownership in &record.active_ownership {
        output.push_str("\n[[active_ownership]]\n");
        output.push_str(&format!(
            "owner = {}\npaths = {}\n",
            toml::Value::String(ownership.owner.clone()),
            toml_array(&ownership.paths)
        ));
    }
    output
}

fn toml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml::Value::String(value.clone()).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn safe_path(value: &str) -> bool {
    !value.trim().is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|component| !component.is_empty() && !matches!(component, "." | ".."))
}

fn paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|rest| rest.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_transition_round_trips_and_rejects_unknown_fields() {
        let path = "strategy/transitions/20260727T120000Z-switch.toml";
        let record = StrategyTransitionRecord {
            operation_id: "switch".into(),
            recorded_at: "2026-07-27T12:00:00Z".into(),
            phase: "implementation".into(),
            previous_strategy_id: "old".into(),
            selected_strategy_id: "new".into(),
            selected_matrix_origin: "adapter/matrix.toml".into(),
            selected_matrix_sha256: "a".repeat(64),
            expected_store_revision: Revision("sha256:before".into()),
            expected_matrix_revision: Revision("sha256:old".into()),
            proposed_matrix_revision: Revision("sha256:new".into()),
            root_binding: "root".into(),
            governed_state_updated: true,
            rationale: "Human selected it.".into(),
            available_capabilities: vec!["subagents".into()],
            preserved_work_paths: vec!["tickets/accepted/HMD-001.md".into()],
            active_ownership: Vec::new(),
        };
        let rendered = render_strategy_transition(&record);
        assert_eq!(
            record,
            parse_strategy_transition(path, &rendered).expect("parse")
        );
        assert!(parse_strategy_transition(path, &(rendered + "unknown = true\n")).is_err());
    }
}
