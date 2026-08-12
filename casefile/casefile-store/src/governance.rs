use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
};

use casefile_core::{
    ActiveOwnership, Classification, Diagnostic, Kind, ProgressEntry, ProgressStatus,
    RecordSummary, Revision, StrategyTransitionRecord, parse_progress_log, parse_strategy,
    parse_strategy_binding, parse_strategy_projection, parse_strategy_transition,
    render_strategy_transition, stable, validate_strategy_matrix,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{
    activation::{ActivationState, activation},
    derived::{StrategyBindingState, derive_snapshot},
    layout::{checked_path, kind_for_path},
    revision::{require_target_revision, synthetic_revision},
    scanning::{ScanResult, scan},
    store::{StoreError, require_safe_target_parent},
    writing::{ensure_worktree, git_diff, introduced_diagnostics},
};

type PriorFileState = (String, Option<Vec<u8>>);

const UNSELECTED_STRATEGY_ID: &str = "unselected";
const ABSENT_MATRIX_REVISION: &str = "absent";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernedOperationKind {
    StrategyTransition,
    WriterBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyTransitionRequest {
    pub investigation: String,
    pub operation_id: String,
    pub recorded_at: String,
    pub selected_matrix_origin: String,
    pub selected_matrix_source: String,
    pub available_capabilities: Vec<String>,
    pub preserved_work_paths: Vec<String>,
    #[serde(default)]
    pub active_ownership: Vec<ActiveOwnership>,
    pub rationale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterBindingRequest {
    pub investigation: String,
    pub binding_source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernedChange {
    pub path: String,
    pub expected_target_revision: Option<Revision>,
    pub proposed_target_revision: Option<Revision>,
    pub rendered_bytes: Vec<u8>,
    pub diff: String,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyTransitionPreview {
    pub operation: GovernedOperationKind,
    pub request: StrategyTransitionRequest,
    pub changes: Vec<GovernedChange>,
    pub transition_record: StrategyTransitionRecord,
    pub diagnostics: Vec<Diagnostic>,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterBindingPreview {
    pub operation: GovernedOperationKind,
    pub request: WriterBindingRequest,
    pub changes: Vec<GovernedChange>,
    pub diagnostics: Vec<Diagnostic>,
    pub no_op: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernedApplyResult {
    pub operation: GovernedOperationKind,
    pub paths: Vec<String>,
    pub resulting_store_revision: Revision,
    pub resulting_target_revisions: BTreeMap<String, Option<Revision>>,
    pub diffs: BTreeMap<String, String>,
    pub no_op: bool,
}

pub(super) fn preview_strategy_transition(
    root: &Path,
    request: StrategyTransitionRequest,
) -> Result<StrategyTransitionPreview, StoreError> {
    let request = canonical_strategy_request(request)?;
    ensure_worktree(root)?;
    let investigation = activated_investigation(root, &request.investigation)?;
    validate_strategy_matrix(&request.selected_matrix_source).map_err(diagnostics_error)?;
    let before = scan(root, &BTreeMap::new())?;
    let selected_value: toml::Value = toml::from_str(&request.selected_matrix_source)
        .map_err(|error| StoreError::Invalid(error.to_string()))?;
    let phase = selected_value
        .get("phase")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| StoreError::Invalid("selected matrix phase is missing".into()))?;
    let selected_strategy_id = selected_value
        .get("strategy_id")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| StoreError::Invalid("selected matrix strategy_id is missing".into()))?;
    let matrix_path = format!("{investigation}/strategy/{phase}.toml");
    if kind_for_path(&matrix_path, &activation(root)?.1) != Some(Kind::Strategy) {
        return Err(StoreError::Invalid(
            "selected matrix phase is not a governed strategy target".into(),
        ));
    }
    let current = before
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == matrix_path);
    require_regular_target(root, &matrix_path, false)?;
    let selected_summary =
        parse_strategy(&matrix_path, &request.selected_matrix_source).map_err(diagnostics_error)?;
    let selected_projection =
        parse_strategy_projection(&matrix_path, &request.selected_matrix_source)
            .map_err(diagnostics_error)?
            .ok_or_else(|| StoreError::Invalid("selected matrix must be complete".into()))?;
    if selected_projection.root_binding != "root" {
        return Err(StoreError::Invalid(
            "strategy transition must preserve the root binding".into(),
        ));
    }
    let (parsed_selected_id, selected_phase) = match selected_summary {
        RecordSummary::Strategy {
            strategy_id, phase, ..
        } => (strategy_id, phase),
        _ => unreachable!("strategy parser returns a strategy summary"),
    };
    if selected_phase != phase || parsed_selected_id != selected_strategy_id {
        return Err(StoreError::Invalid(
            "selected matrix phase does not match governed phase state".into(),
        ));
    }
    let (previous_strategy_id, expected_matrix_revision) = match current {
        Some(current) => {
            let current_text = std::str::from_utf8(&current.original_bytes)
                .map_err(|_| StoreError::Invalid("governed phase matrix must be UTF-8".into()))?;
            let current_summary =
                parse_strategy(&matrix_path, current_text).map_err(diagnostics_error)?;
            let current_projection = parse_strategy_projection(&matrix_path, current_text)
                .map_err(diagnostics_error)?
                .ok_or_else(|| {
                    StoreError::Invalid("governed phase matrix must be complete".into())
                })?;
            if current_projection.root_binding != "root" {
                return Err(StoreError::Invalid(
                    "strategy transition must preserve the root binding".into(),
                ));
            }
            let (strategy_id, current_phase) = match current_summary {
                RecordSummary::Strategy {
                    strategy_id, phase, ..
                } => (strategy_id, phase),
                _ => unreachable!("strategy parser returns a strategy summary"),
            };
            if current_phase != phase {
                return Err(StoreError::Invalid(
                    "selected matrix phase does not match governed phase state".into(),
                ));
            }
            (
                strategy_id,
                digest(&normalized_eol(&current.original_bytes)),
            )
        }
        None => {
            let transition_prefix = format!("{investigation}/strategy/transitions/");
            let has_phase_history = before.snapshot.entries.iter().any(|entry| {
                entry.path.starts_with(&transition_prefix)
                    && matches!(
                        entry.summary.as_ref(),
                        Some(RecordSummary::StrategyTransition { record }) if record.phase == phase
                    )
            });
            if has_phase_history {
                return Err(StoreError::Invalid(
                    "governed phase matrix is missing after a recorded transition".into(),
                ));
            }
            (
                UNSELECTED_STRATEGY_ID.into(),
                Revision(ABSENT_MATRIX_REVISION.into()),
            )
        }
    };
    let available = request
        .available_capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let missing = selected_projection
        .requirements
        .capabilities
        .iter()
        .filter(|required| !available.contains(required.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(StoreError::Invalid(format!(
            "unavailable capabilities: {}",
            missing.join(", ")
        )));
    }
    let timestamp_token = request
        .recorded_at
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == 'T' || *character == 'Z')
        .collect::<String>();
    let transition_path = format!(
        "{investigation}/strategy/transitions/{timestamp_token}-{}.toml",
        request.operation_id
    );
    require_regular_target(root, &transition_path, false)?;
    let selected_source_bytes = request.selected_matrix_source.as_bytes().to_vec();
    let normalized_selected_bytes = normalized_eol(&selected_source_bytes);
    let selected_bytes = current
        .filter(|entry| eol_equivalent(&entry.original_bytes, &selected_source_bytes))
        .map_or_else(
            || selected_source_bytes.clone(),
            |entry| entry.original_bytes.clone(),
        );
    let proposed_matrix_revision = digest(&normalized_selected_bytes);
    let existing_transition = before
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == transition_path);
    let fresh_record = StrategyTransitionRecord {
        operation_id: request.operation_id.clone(),
        recorded_at: request.recorded_at.clone(),
        phase: phase.into(),
        previous_strategy_id,
        selected_strategy_id: selected_strategy_id.into(),
        selected_matrix_origin: request.selected_matrix_origin.clone(),
        selected_matrix_sha256: raw_sha256(&normalized_selected_bytes),
        expected_store_revision: before.snapshot.revision.clone(),
        expected_matrix_revision,
        proposed_matrix_revision: proposed_matrix_revision.clone(),
        root_binding: "root".into(),
        governed_state_updated: true,
        rationale: request.rationale.clone(),
        available_capabilities: request.available_capabilities.clone(),
        preserved_work_paths: request.preserved_work_paths.clone(),
        active_ownership: request.active_ownership.clone(),
    };
    let transition_record = match existing_transition {
        Some(entry) => {
            if current.is_none_or(|current| {
                !eol_equivalent(&current.original_bytes, &selected_source_bytes)
            }) {
                return Err(StoreError::Invalid(
                    "strategy transition identity cannot be replayed over different governed state"
                        .into(),
                ));
            }
            let text = std::str::from_utf8(&entry.original_bytes).map_err(|_| {
                StoreError::Invalid("existing strategy transition must be UTF-8".into())
            })?;
            let existing =
                parse_strategy_transition(&transition_path, text).map_err(diagnostics_error)?;
            if !transition_matches_request(&existing, &request, phase, selected_strategy_id)
                || !matrix_revision_matches(
                    &existing.proposed_matrix_revision,
                    &selected_source_bytes,
                    current.map(|entry| entry.original_bytes.as_slice()),
                )
                || !matrix_hash_matches(
                    &existing.selected_matrix_sha256,
                    &selected_source_bytes,
                    current.map(|entry| entry.original_bytes.as_slice()),
                )
            {
                return Err(StoreError::Invalid(
                    "strategy transition identity already has different content".into(),
                ));
            }
            existing
        }
        None => fresh_record,
    };
    let rendered_record_bytes = render_strategy_transition(&transition_record).into_bytes();
    let record_bytes = match existing_transition {
        Some(existing) if eol_equivalent(&existing.original_bytes, &rendered_record_bytes) => {
            existing.original_bytes.clone()
        }
        Some(_) => {
            return Err(StoreError::Invalid(
                "strategy transition record differs from its governed representation".into(),
            ));
        }
        None => rendered_record_bytes,
    };
    let mut overlay = BTreeMap::new();
    overlay.insert(matrix_path.clone(), Some(selected_bytes.clone()));
    overlay.insert(transition_path.clone(), Some(record_bytes.clone()));
    let proposed = scan(root, &overlay)?;
    let diagnostics = scoped_introduced(&before, &proposed, &investigation);
    let matrix_change = change(root, matrix_path, current, selected_bytes)?;
    let record_change = change(root, transition_path, existing_transition, record_bytes)?;
    let no_op = matrix_change.no_op && record_change.no_op && diagnostics.is_empty();
    Ok(StrategyTransitionPreview {
        operation: GovernedOperationKind::StrategyTransition,
        request,
        changes: vec![matrix_change, record_change],
        transition_record,
        diagnostics,
        no_op,
    })
}

pub(super) fn apply_strategy_transition(
    root: &Path,
    mut preview: StrategyTransitionPreview,
) -> Result<GovernedApplyResult, StoreError> {
    preview.request = canonical_strategy_request(preview.request)?;
    ensure_worktree(root)?;
    if preview.operation != GovernedOperationKind::StrategyTransition {
        return Err(StoreError::Invalid("wrong governed operation kind".into()));
    }
    if !preview.diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "strategy transition preview contains diagnostics".into(),
        ));
    }
    validate_strategy_preview(&preview)?;
    let current = scan(root, &BTreeMap::new())?;
    for change in &preview.changes {
        require_target_revision(
            &root.join(&change.path),
            change.expected_target_revision.as_ref(),
        )?;
    }
    if preview.no_op {
        return result_from_scan(
            GovernedOperationKind::StrategyTransition,
            &preview.changes,
            &current,
            true,
        );
    }
    let prior = apply_multi_file_transaction(root, &preview.changes)?;
    let resulting = match scan(root, &BTreeMap::new()) {
        Ok(resulting) => resulting,
        Err(error) => {
            restore_all(root, &prior)?;
            return Err(error);
        }
    };
    if preview.changes.iter().any(|change| {
        resulting
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == change.path)
            .is_none_or(|entry| entry.original_bytes != change.rendered_bytes)
    }) {
        restore_all(root, &prior)?;
        return Err(StoreError::Invalid(
            "strategy transition post-write verification failed".into(),
        ));
    }
    result_from_scan(
        GovernedOperationKind::StrategyTransition,
        &preview.changes,
        &resulting,
        false,
    )
}

pub(super) fn preview_writer_binding(
    root: &Path,
    request: WriterBindingRequest,
) -> Result<WriterBindingPreview, StoreError> {
    let request = canonical_writer_binding_request(request)?;
    ensure_worktree(root)?;
    let investigation = activated_investigation(root, &request.investigation)?;
    let path = format!("{investigation}/strategy/bindings.toml");
    if kind_for_path(&path, &activation(root)?.1) != Some(Kind::StrategyBinding) {
        return Err(StoreError::Invalid(
            "binding target is outside the activated investigation".into(),
        ));
    }
    let binding =
        match parse_strategy_binding(&path, &request.binding_source).map_err(diagnostics_error)? {
            RecordSummary::StrategyBinding { binding } => binding,
            _ => unreachable!("binding parser returns a binding summary"),
        };
    require_regular_target(root, &path, false)?;
    let before = scan(root, &BTreeMap::new())?;
    let implementation_path = format!("{investigation}/strategy/implementation.toml");
    let implementation = before
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == implementation_path)
        .filter(|entry| entry.classification == Classification::Governed)
        .ok_or_else(|| StoreError::Invalid("selected implementation matrix is invalid".into()))?;
    let (adapter, projection) = match &implementation.summary {
        Some(RecordSummary::Strategy { adapter, phase, .. }) if phase == "implementation" => {
            let text = std::str::from_utf8(&implementation.original_bytes).map_err(|_| {
                StoreError::Invalid("selected implementation matrix must be UTF-8".into())
            })?;
            let projection = parse_strategy_projection(&implementation_path, text)
                .map_err(diagnostics_error)?
                .ok_or_else(|| {
                    StoreError::Invalid("selected implementation matrix must be complete".into())
                })?;
            (adapter, projection)
        }
        _ => {
            return Err(StoreError::Invalid(
                "selected implementation matrix is invalid".into(),
            ));
        }
    };
    if binding.adapter != *adapter
        || projection
            .workers
            .iter()
            .filter(|worker| worker.role == "implementation-writer")
            .count()
            != 1
    {
        return Err(StoreError::Invalid(
            "binding does not match the selected implementation strategy".into(),
        ));
    }
    ensure_binding_inactive(&before, &investigation)?;
    let existing = before
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path);
    let bytes = request.binding_source.as_bytes().to_vec();
    let mut overlay = BTreeMap::new();
    overlay.insert(path.clone(), Some(bytes.clone()));
    let proposed = scan(root, &overlay)?;
    let diagnostics = scoped_introduced(&before, &proposed, &investigation);
    let change = change(root, path, existing, bytes)?;
    let no_op = change.no_op && diagnostics.is_empty();
    Ok(WriterBindingPreview {
        operation: GovernedOperationKind::WriterBinding,
        request,
        changes: vec![change],
        diagnostics,
        no_op,
    })
}

pub(super) fn apply_writer_binding(
    root: &Path,
    mut preview: WriterBindingPreview,
) -> Result<GovernedApplyResult, StoreError> {
    preview.request = canonical_writer_binding_request(preview.request)?;
    ensure_worktree(root)?;
    if preview.operation != GovernedOperationKind::WriterBinding {
        return Err(StoreError::Invalid("wrong governed operation kind".into()));
    }
    if !preview.diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "writer binding preview contains diagnostics".into(),
        ));
    }
    validate_binding_preview(&preview)?;
    let current = scan(root, &BTreeMap::new())?;
    let change = preview
        .changes
        .first()
        .ok_or_else(|| StoreError::Invalid("binding preview has no target".into()))?;
    let entry = current
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == change.path);
    require_target_revision(
        &root.join(&change.path),
        change.expected_target_revision.as_ref(),
    )?;
    if preview.no_op {
        return result_from_scan(
            GovernedOperationKind::WriterBinding,
            &preview.changes,
            &current,
            true,
        );
    }
    let before_bytes = entry.map(|entry| entry.original_bytes.clone());
    atomic_write(root, &change.path, &change.rendered_bytes)?;
    let resulting = match scan(root, &BTreeMap::new()) {
        Ok(resulting) => resulting,
        Err(error) => {
            restore(root, &change.path, before_bytes.as_deref())?;
            return Err(error);
        }
    };
    let derived = derive_snapshot(&resulting);
    let verified = resulting
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == change.path)
        .is_some_and(|entry| entry.original_bytes == change.rendered_bytes)
        && derived.records.iter().any(|record| {
            record.path == change.path
                && record.strategy_binding.as_ref().is_some_and(|binding| {
                    matches!(&binding.state, StrategyBindingState::Resolved { .. })
                })
        });
    if !verified {
        restore(root, &change.path, before_bytes.as_deref())?;
        return Err(StoreError::Invalid(
            "writer binding post-write verification failed".into(),
        ));
    }
    result_from_scan(
        GovernedOperationKind::WriterBinding,
        &preview.changes,
        &resulting,
        false,
    )
}

pub(super) fn require_writer_progress(
    root: &Path,
    investigation: &str,
    ticket_id: &str,
) -> Result<(), StoreError> {
    let investigation = activated_investigation(root, investigation)?;
    let scan = scan(root, &BTreeMap::new())?;
    let ticket_prefix = format!("{investigation}/tickets/accepted/");
    let tickets = scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| {
            entry.path.starts_with(&ticket_prefix)
                && entry.kind == Some(Kind::Ticket)
                && entry.classification == Classification::Governed
                && entry.identity.as_deref() == Some(ticket_id)
        })
        .count();
    if tickets != 1
        || scan
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path.starts_with(&ticket_prefix))
    {
        return Err(StoreError::Invalid(
            "writer spawn requires one valid accepted ticket in the activated investigation".into(),
        ));
    }
    let log = canonical_progress(&scan, &investigation)?;
    let mut status = ProgressStatus::Unknown;
    let mut seen = false;
    for entry in log.entries {
        if let ProgressEntry::Transition {
            ticket_id: id, to, ..
        } = entry
            && id == ticket_id
        {
            status = to;
            seen = true;
        }
    }
    if !seen || status != ProgressStatus::InProgress {
        return Err(StoreError::Invalid(
            "writer spawn requires an explicit current in_progress transition for the ticket"
                .into(),
        ));
    }
    Ok(())
}

fn ensure_binding_inactive(scan: &ScanResult, investigation: &str) -> Result<(), StoreError> {
    let progress_path = format!("{investigation}/progress/log.toml");
    let progress_is_absent = !scan
        .snapshot
        .entries
        .iter()
        .any(|entry| entry.path == progress_path)
        && !scan
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == progress_path);
    if progress_is_absent {
        return Ok(());
    }
    let log = canonical_progress(scan, investigation)?;
    let prefix = format!("{investigation}/tickets/accepted/");
    if scan
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.path == progress_path || diagnostic.path.starts_with(&prefix))
    {
        return Err(StoreError::Invalid(
            "writer binding activity is ambiguous because canonical progress is conflicting".into(),
        ));
    }
    let accepted = scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| {
            entry.path.starts_with(&prefix)
                && entry.kind == Some(Kind::Ticket)
                && entry.classification == Classification::Governed
        })
        .filter_map(|entry| entry.identity.clone())
        .collect::<BTreeSet<_>>();
    let mut statuses = accepted
        .iter()
        .map(|ticket| (ticket.clone(), ProgressStatus::Unknown))
        .collect::<BTreeMap<_, _>>();
    for entry in log.entries {
        if let ProgressEntry::Transition { ticket_id, to, .. } = entry {
            let Some(status) = statuses.get_mut(&ticket_id) else {
                return Err(StoreError::Invalid(
                    "progress contains a conflicting or unsupported ticket".into(),
                ));
            };
            *status = to;
        }
    }
    if statuses.values().any(|status| {
        matches!(
            status,
            ProgressStatus::InProgress
                | ProgressStatus::InReview
                | ProgressStatus::Verifying
                | ProgressStatus::Blocked
        )
    }) {
        return Err(StoreError::Invalid(
            "cannot replace a writer binding while implementation or correction work is active"
                .into(),
        ));
    }
    Ok(())
}

fn canonical_progress(
    scan: &ScanResult,
    investigation: &str,
) -> Result<casefile_core::ProgressLog, StoreError> {
    let path = format!("{investigation}/progress/log.toml");
    let matching = scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| entry.path == path)
        .collect::<Vec<_>>();
    if matching.len() != 1
        || matching[0].classification != Classification::Governed
        || matching[0].kind != Some(Kind::Progress)
        || scan
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == path)
    {
        return Err(StoreError::Invalid(
            "writer binding activity requires one valid canonical progress log".into(),
        ));
    }
    let text = std::str::from_utf8(&matching[0].original_bytes)
        .map_err(|_| StoreError::Invalid("progress log must be UTF-8".into()))?;
    parse_progress_log(&path, text).map_err(diagnostics_error)
}

fn activated_investigation(root: &Path, value: &str) -> Result<String, StoreError> {
    let investigation = checked_path(value)?;
    let (state, active, _) = activation(root)?;
    if state != ActivationState::Active
        || !active.projects.values().any(|project| {
            project
                .investigations
                .iter()
                .any(|candidate| candidate == &investigation)
        })
    {
        return Err(StoreError::Invalid(
            "investigation is not uniquely activated".into(),
        ));
    }
    Ok(investigation)
}

fn canonical_strategy_request(
    mut request: StrategyTransitionRequest,
) -> Result<StrategyTransitionRequest, StoreError> {
    request.investigation = checked_path(&request.investigation)?;
    request.preserved_work_paths = request
        .preserved_work_paths
        .into_iter()
        .map(|path| checked_path(&path))
        .collect::<Result<Vec<_>, _>>()?;
    for ownership in &mut request.active_ownership {
        ownership.paths = ownership
            .paths
            .drain(..)
            .map(|path| checked_path(&path))
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(request)
}

fn canonical_writer_binding_request(
    mut request: WriterBindingRequest,
) -> Result<WriterBindingRequest, StoreError> {
    request.investigation = checked_path(&request.investigation)?;
    Ok(request)
}

fn normalized_eol(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"\r\n") {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }
    normalized
}

fn eol_equivalent(left: &[u8], right: &[u8]) -> bool {
    left == right || normalized_eol(left) == normalized_eol(right)
}

fn matrix_hash_matches(value: &str, selected: &[u8], current: Option<&[u8]>) -> bool {
    matrix_identity_bytes(selected, current)
        .iter()
        .any(|bytes| value == raw_sha256(bytes))
}

fn matrix_revision_matches(value: &Revision, selected: &[u8], current: Option<&[u8]>) -> bool {
    matrix_identity_bytes(selected, current)
        .iter()
        .any(|bytes| value == &digest(bytes))
}

fn matrix_identity_bytes(selected: &[u8], current: Option<&[u8]>) -> Vec<Vec<u8>> {
    let normalized_selected = normalized_eol(selected);
    let mut candidates = vec![
        selected.to_vec(),
        normalized_selected.clone(),
        crlf_eol(&normalized_selected),
    ];
    if let Some(current) = current {
        let normalized_current = normalized_eol(current);
        candidates.push(current.to_vec());
        candidates.push(normalized_current.clone());
        candidates.push(crlf_eol(&normalized_current));
    }
    candidates
}

fn crlf_eol(bytes: &[u8]) -> Vec<u8> {
    let mut crlf = Vec::with_capacity(bytes.len());
    for byte in bytes {
        if *byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(*byte);
    }
    crlf
}

fn transition_matches_request(
    record: &StrategyTransitionRecord,
    request: &StrategyTransitionRequest,
    phase: &str,
    selected_strategy_id: &str,
) -> bool {
    record.operation_id == request.operation_id
        && record.recorded_at == request.recorded_at
        && record.phase == phase
        && record.selected_strategy_id == selected_strategy_id
        && record.selected_matrix_origin == request.selected_matrix_origin
        && record.rationale == request.rationale
        && record.available_capabilities == request.available_capabilities
        && record.preserved_work_paths == request.preserved_work_paths
        && record.active_ownership == request.active_ownership
        && record.root_binding == "root"
        && record.governed_state_updated
}

fn scoped_introduced(
    before: &ScanResult,
    proposed: &ScanResult,
    investigation: &str,
) -> Vec<Diagnostic> {
    let prefix = format!("{investigation}/");
    let baseline = before
        .diagnostics
        .iter()
        .filter(|item| item.path.starts_with(&prefix))
        .cloned()
        .collect::<Vec<_>>();
    let after = proposed
        .diagnostics
        .iter()
        .filter(|item| item.path.starts_with(&prefix))
        .cloned()
        .collect::<Vec<_>>();
    stable(introduced_diagnostics(&baseline, &after))
}

fn change(
    root: &Path,
    path: String,
    existing: Option<&casefile_core::EntrySnapshot>,
    rendered_bytes: Vec<u8>,
) -> Result<GovernedChange, StoreError> {
    let no_op = existing.is_some_and(|entry| entry.original_bytes == rendered_bytes);
    let diff = if no_op {
        String::new()
    } else {
        git_diff(
            root,
            &path,
            existing.map(|entry| entry.original_bytes.as_slice()),
            Some(&rendered_bytes),
        )?
    };
    Ok(GovernedChange {
        proposed_target_revision: Some(synthetic_revision(&path, true)),
        path,
        expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
        rendered_bytes,
        diff,
        no_op,
    })
}

fn validate_strategy_preview(preview: &StrategyTransitionPreview) -> Result<(), StoreError> {
    let selected: toml::Value = toml::from_str(&preview.request.selected_matrix_source)
        .map_err(|error| StoreError::Invalid(error.to_string()))?;
    let phase = selected
        .get("phase")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| StoreError::Invalid("selected matrix phase is missing".into()))?;
    let strategy_id = selected
        .get("strategy_id")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| StoreError::Invalid("selected matrix strategy_id is missing".into()))?;
    let timestamp_token = preview
        .request
        .recorded_at
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == 'T' || *character == 'Z')
        .collect::<String>();
    let expected_paths = [
        format!("{}/strategy/{phase}.toml", preview.request.investigation),
        format!(
            "{}/strategy/transitions/{timestamp_token}-{}.toml",
            preview.request.investigation, preview.request.operation_id
        ),
    ];
    if preview.changes.len() != 2
        || preview
            .changes
            .iter()
            .zip(&expected_paths)
            .any(|(change, path)| {
                change.path != *path
                    || change.proposed_target_revision != Some(synthetic_revision(path, true))
            })
        || !eol_equivalent(
            &preview.changes[0].rendered_bytes,
            preview.request.selected_matrix_source.as_bytes(),
        )
        || preview.no_op != preview.changes.iter().all(|change| change.no_op)
        || !transition_matches_request(
            &preview.transition_record,
            &preview.request,
            phase,
            strategy_id,
        )
    {
        return Err(StoreError::Invalid(
            "strategy transition preview was altered".into(),
        ));
    }
    let record_text = std::str::from_utf8(&preview.changes[1].rendered_bytes)
        .map_err(|_| StoreError::Invalid("strategy transition preview was altered".into()))?;
    let record = parse_strategy_transition(&expected_paths[1], record_text)
        .map_err(|_| StoreError::Invalid("strategy transition preview was altered".into()))?;
    if record != preview.transition_record {
        return Err(StoreError::Invalid(
            "strategy transition preview was altered".into(),
        ));
    }
    Ok(())
}

fn validate_binding_preview(preview: &WriterBindingPreview) -> Result<(), StoreError> {
    let path = format!("{}/strategy/bindings.toml", preview.request.investigation);
    let Some(change) = preview.changes.first() else {
        return Err(StoreError::Invalid("binding preview has no target".into()));
    };
    if preview.changes.len() != 1
        || change.path != path
        || change.rendered_bytes != preview.request.binding_source.as_bytes()
        || change.proposed_target_revision != Some(synthetic_revision(&path, true))
        || preview.no_op != change.no_op
    {
        return Err(StoreError::Invalid(
            "writer binding preview was altered".into(),
        ));
    }
    Ok(())
}

fn result_from_scan(
    operation: GovernedOperationKind,
    changes: &[GovernedChange],
    scan: &ScanResult,
    no_op: bool,
) -> Result<GovernedApplyResult, StoreError> {
    Ok(GovernedApplyResult {
        operation,
        paths: changes.iter().map(|change| change.path.clone()).collect(),
        resulting_store_revision: scan.snapshot.revision.clone(),
        resulting_target_revisions: changes
            .iter()
            .map(|change| {
                (
                    change.path.clone(),
                    scan.snapshot
                        .entries
                        .iter()
                        .find(|entry| entry.path == change.path)
                        .map(|entry| entry.content_revision.clone()),
                )
            })
            .collect(),
        diffs: changes
            .iter()
            .map(|change| (change.path.clone(), change.diff.clone()))
            .collect(),
        no_op,
    })
}

fn apply_multi_file_transaction(
    root: &Path,
    changes: &[GovernedChange],
) -> Result<Vec<PriorFileState>, StoreError> {
    let prior = changes
        .iter()
        .map(|change| {
            let target = root.join(&change.path);
            let bytes = match fs::read(&target) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            Ok((change.path.clone(), bytes))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    for change in changes.iter().filter(|change| !change.no_op) {
        if let Err(error) = atomic_write(root, &change.path, &change.rendered_bytes) {
            restore_all(root, &prior)?;
            return Err(error);
        }
    }
    Ok(prior)
}

fn restore_all(root: &Path, prior: &[(String, Option<Vec<u8>>)]) -> Result<(), StoreError> {
    for (path, bytes) in prior.iter().rev() {
        restore(root, path, bytes.as_deref())?;
    }
    for (path, bytes) in prior {
        let current = fs::read(root.join(path));
        match (bytes, current) {
            (Some(expected), Ok(current)) if expected == &current => {}
            (None, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(StoreError::Invalid(
                    "strategy transition rollback verification failed".into(),
                ));
            }
        }
    }
    Ok(())
}

fn restore(root: &Path, path: &str, bytes: Option<&[u8]>) -> Result<(), StoreError> {
    match bytes {
        Some(bytes) => atomic_write(root, path, bytes),
        None => match fs::remove_file(root.join(path)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
    }
}

fn atomic_write(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), StoreError> {
    require_regular_target(root, relative, false)?;
    let target = root.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| StoreError::Invalid("governed target has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary
        .persist(&target)
        .map_err(|error| StoreError::Io(error.error))?;
    Ok(())
}

fn require_regular_target(root: &Path, relative: &str, required: bool) -> Result<(), StoreError> {
    let target = root.join(relative);
    require_safe_target_parent(
        root,
        Path::new(relative)
            .parent()
            .unwrap_or_else(|| Path::new("")),
        "governed target",
    )?;
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err(StoreError::Invalid(
            "governed target must be a regular non-symlink file".into(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(StoreError::Invalid(
            "required governed target is missing".into(),
        )),
        Err(error) => Err(error.into()),
    }
}

fn diagnostics_error(diagnostics: Vec<Diagnostic>) -> StoreError {
    StoreError::Invalid(
        diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn digest(bytes: &[u8]) -> Revision {
    Revision(format!("sha256:{}", raw_sha256(bytes)))
}

fn raw_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
