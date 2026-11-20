use super::*;
use crate::StrategyAssignmentRequest;
use casefile_core::{StrategyAssignments, parse_strategy_assignments};

#[derive(Clone, Debug)]
pub struct StrategyAssignmentPreview {
    request: StrategyAssignmentRequest,
    pub(crate) changes: Vec<GovernedChange>,
    expected_input_revisions: BTreeMap<String, Option<Revision>>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) no_op: bool,
}

pub(crate) fn preview_strategy_assignments(
    root: &Path,
    mut request: StrategyAssignmentRequest,
) -> Result<StrategyAssignmentPreview, StoreError> {
    request.target = request.target.canonical()?;
    let (context, proposal) = capture(root, &request, false)?;
    prepare(root, request, &context, &proposal)
}

pub(crate) fn apply_strategy_assignments(
    root: &Path,
    preview: &StrategyAssignmentPreview,
) -> Result<GovernedApplyResult, StoreError> {
    if !preview.diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "strategy assignment preview contains diagnostics".into(),
        ));
    }
    let (context, proposal) = capture(root, &preview.request, true)?;
    let checked = prepare(root, preview.request.clone(), &context, &proposal)?;
    let mut expected = preview.expected_input_revisions.clone();
    if let [change] = checked.changes.as_slice()
        && change.no_op
    {
        expected.remove(&change.path);
    }
    context.require_revisions(&expected)?;
    if !checked.diagnostics.is_empty() || (!checked.no_op && checked.changes != preview.changes) {
        return Err(StoreError::Invalid(
            "strategy assignment validation changed after preview".into(),
        ));
    }
    apply_governed_change(
        root,
        GovernedOperationKind::StrategyAssignments,
        checked.changes.first(),
        &context,
        "strategy assignment verification",
    )
}

fn capture(
    root: &Path,
    request: &StrategyAssignmentRequest,
    applying: bool,
) -> Result<(MutationContext, Overlay), StoreError> {
    ensure_worktree(root)?;
    activated_investigation(root, &request.target.investigation)?;
    let path = request.target.assignments_path();
    require_regular_target(root, &path, false)?;
    let assignments = StrategyAssignments::codex(
        request.target.phase,
        request.target.strategy_id.clone(),
        request.roles.clone(),
    );
    let source = toml::to_string_pretty(&assignments)
        .map_err(|error| StoreError::Invalid(error.to_string()))?;
    parse_strategy_assignments(&path, &source).map_err(diagnostics_error)?;
    let proposal = Overlay::from([(path, Some(source.into_bytes()))]);
    let context =
        MutationContext::capture(root, &proposal, &[request.target.matrix_path()], applying)?;
    Ok((context, proposal))
}

fn prepare(
    root: &Path,
    request: StrategyAssignmentRequest,
    context: &MutationContext,
    proposal: &Overlay,
) -> Result<StrategyAssignmentPreview, StoreError> {
    let path = request.target.assignments_path();
    let matrix_path = request.target.matrix_path();
    let entry = context
        .entry(&matrix_path)
        .filter(|entry| entry.content_revision == request.expected_revisions.matrix)
        .ok_or(StoreError::StaleTargetRevision)?;
    let (adapter, matrix) = request.target.matrix(
        entry,
        context
            .facts(&matrix_path)
            .and_then(|facts| facts.strategy.as_ref()),
    )?;
    if adapter != "codex" {
        return Err(StoreError::Invalid(
            "strategy assignments require a selected Codex strategy".into(),
        ));
    }
    let mut expected = BTreeMap::new();
    if let Some(legacy) = request.target.legacy_path() {
        expected.insert(
            legacy,
            request.expected_revisions.legacy_writer_binding.clone(),
        );
    }
    let existing = context.entry(&path);
    let bytes = proposal[&path].as_ref().expect("assignments write bytes");
    if existing.is_none_or(|entry| entry.original_bytes != *bytes) {
        expected.insert(path.clone(), request.expected_revisions.assignments.clone());
    }
    context.require_revisions(&expected)?;
    if kind_for_path(&path, &context.active) != Some(Kind::StrategyAssignments) {
        return Err(StoreError::Invalid(
            "assignment target is outside the activated investigation".into(),
        ));
    }
    let selected_roles = matrix
        .workers
        .iter()
        .map(|worker| &worker.role)
        .collect::<BTreeSet<_>>();
    let requested_roles = request
        .roles
        .iter()
        .map(|role| &role.role)
        .collect::<BTreeSet<_>>();
    if requested_roles != selected_roles {
        return Err(StoreError::Invalid(
            "provide an exact assignment for every distinct selected worker role".into(),
        ));
    }
    let (changes, diagnostics) = if selected_roles.is_empty() {
        (vec![], vec![])
    } else {
        let proposed = context.overlay(proposal);
        let diagnostics =
            scoped_introduced(&context.before, &proposed, &request.target.investigation);
        (
            vec![change(root, path, existing, bytes.clone())?],
            diagnostics,
        )
    };
    Ok(StrategyAssignmentPreview {
        request,
        no_op: changes.iter().all(|change| change.no_op) && diagnostics.is_empty(),
        changes,
        expected_input_revisions: context.revisions(),
        diagnostics,
    })
}
