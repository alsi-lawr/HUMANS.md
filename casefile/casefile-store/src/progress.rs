use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use casefile_core::{
    Diagnostic, ProgressEntry, ProgressLog, Revision, parse_progress_log, render_progress_log,
    validate_progress_log,
};
use serde::{Deserialize, Serialize};

use crate::{
    activation::{ActivationState, activation},
    layout::checked_path,
    mutation::{MutationContext, Overlay},
    revision::require_target_revision,
    store::StoreError,
    writing::git_diff,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressChangeRequest {
    pub investigation: String,
    #[serde(default)]
    pub entries: Vec<ProgressEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<ProgressLog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_source: Option<String>,
    #[serde(default)]
    pub bootstrap: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressPreview {
    pub request: ProgressChangeRequest,
    pub path: String,
    pub expected_target_revision: Option<Revision>,
    #[serde(default)]
    pub expected_input_revisions: BTreeMap<String, Option<Revision>>,
    pub diagnostics: Vec<Diagnostic>,
    pub diff: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_bytes: Option<Vec<u8>>,
    pub no_op: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bootstrap_ticket_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressApplyResult {
    pub path: String,
    pub resulting_target_revision: Option<Revision>,
    pub diff: String,
    pub no_op: bool,
}

pub(super) fn preview(
    root: &Path,
    mut request: ProgressChangeRequest,
) -> Result<ProgressPreview, StoreError> {
    request.investigation = checked_path(&request.investigation)?;
    ensure_worktree(root)?;
    let (path, scope_prefix) = progress_path(root, &request.investigation)?;
    let (context, log, bytes) = capture(root, &request, false)?;
    prepare(root, request, &context, path, scope_prefix, &log, bytes)
}

fn prepare(
    root: &Path,
    request: ProgressChangeRequest,
    context: &MutationContext,
    path: String,
    scope_prefix: String,
    proposed_log: &ProgressLog,
    bytes: Vec<u8>,
) -> Result<ProgressPreview, StoreError> {
    let before = &context.before;
    let existing = context.entry(&path);
    let replacing = request.replacement.is_some() || request.replacement_source.is_some();
    let empty = ProgressLog {
        entries: Vec::new(),
    };
    let existing_log = if replacing {
        &empty
    } else {
        match existing {
            Some(_) => context
                .facts(&path)
                .and_then(|facts| facts.progress.as_ref())
                .ok_or_else(|| StoreError::Invalid("progress log is invalid".into()))?,
            None => &empty,
        }
    };
    if request.bootstrap {
        if !request.entries.is_empty()
            || request.replacement.is_some()
            || request.replacement_source.is_some()
        {
            return Ok(rejected(
                request,
                path,
                Diagnostic::new(
                    "progress/log.toml",
                    "invalid_progress_request",
                    "bootstrap cannot be combined with entries or replacement",
                ),
            ));
        }
        if let Some(existing) = existing {
            // Bootstrap marks a previously absent scope as adopted.  It never normalises or
            // replaces an existing log: parsing still proves that the record is valid, but the
            // original bytes and target revision remain the preview/apply result.
            let diagnostics = scoped_diagnostics(&before.diagnostics, &path);
            return Ok(ProgressPreview {
                request,
                path,
                expected_target_revision: Some(existing.content_revision.clone()),
                expected_input_revisions: context.revisions(),
                no_op: diagnostics.is_empty(),
                diagnostics,
                diff: String::new(),
                proposed_bytes: Some(existing.original_bytes.clone()),
                bootstrap_ticket_ids: Vec::new(),
            });
        }
    }
    if replacing {
        if !request.entries.is_empty()
            || (request.replacement.is_some() && request.replacement_source.is_some())
        {
            return Ok(rejected(
                request,
                path,
                Diagnostic::new(
                    "progress/log.toml",
                    "invalid_progress_request",
                    "replacement cannot be combined with entries",
                ),
            ));
        }
    } else {
        let existing_by_id = existing_log
            .entries
            .iter()
            .map(|entry| (entry.id(), entry))
            .collect::<BTreeMap<_, _>>();
        let mut requested = BTreeSet::new();
        for entry in &request.entries {
            if !requested.insert(entry.id()) {
                return Ok(rejected(
                    request,
                    path,
                    Diagnostic::new(
                        "progress/log.toml",
                        "invalid_progress_operation_id",
                        "operation IDs must be unique",
                    ),
                ));
            }
            if let Some(current) = existing_by_id.get(entry.id()) {
                if **current != *entry {
                    return Ok(rejected(
                        request,
                        path,
                        Diagnostic::new(
                            "progress/log.toml",
                            "conflicting_progress_operation_id",
                            "operation ID is already recorded with different content",
                        ),
                    ));
                }
            }
        }
    }
    if let Err(diagnostic) = validate_progress_log(&path, proposed_log) {
        return Ok(rejected(request, path, diagnostic));
    }
    let same = existing.is_some_and(|entry| entry.original_bytes == bytes);
    let mut overlay = BTreeMap::new();
    overlay.insert(path.clone(), Some(bytes.clone()));
    let proposed = context.overlay_progress(&overlay, &path, proposed_log);
    let diagnostics = scoped_diagnostics(&proposed.diagnostics, &path);
    let bootstrap_ticket_ids = if request.bootstrap {
        accepted_ticket_ids(before, &scope_prefix)
    } else {
        Vec::new()
    };
    if !diagnostics.is_empty() {
        return Ok(ProgressPreview {
            request,
            path,
            expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
            expected_input_revisions: context.revisions(),
            diagnostics,
            diff: String::new(),
            proposed_bytes: Some(bytes),
            no_op: false,
            bootstrap_ticket_ids,
        });
    }
    let diff = if same {
        String::new()
    } else {
        diff(
            root,
            &path,
            existing.map(|entry| entry.original_bytes.as_slice()),
            Some(&bytes),
        )?
    };
    Ok(ProgressPreview {
        request,
        path,
        expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
        expected_input_revisions: context.revisions(),
        diagnostics,
        diff,
        proposed_bytes: Some(bytes),
        no_op: same,
        bootstrap_ticket_ids,
    })
}

pub(super) fn apply(
    root: &Path,
    mut preview: ProgressPreview,
) -> Result<ProgressApplyResult, StoreError> {
    preview.request.investigation = checked_path(&preview.request.investigation)?;
    apply_ref(root, &preview)
}

pub(super) fn apply_ref(
    root: &Path,
    preview: &ProgressPreview,
) -> Result<ProgressApplyResult, StoreError> {
    checked_path(&preview.request.investigation)?;
    ensure_worktree(root)?;
    if !preview.diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "progress preview contains validation diagnostics".into(),
        ));
    }
    let (path, scope_prefix) = progress_path(root, &preview.request.investigation)?;
    if path != preview.path {
        return Err(StoreError::Invalid(
            "progress preview target does not match request".into(),
        ));
    }
    let (context, log, bytes) = capture(root, &preview.request, true)?;
    let current_entry = context.entry(&path);
    let stale_target =
        match require_target_revision(&root.join(&path), preview.expected_target_revision.as_ref())
        {
            Ok(()) => false,
            Err(StoreError::StaleTargetRevision) => true,
            Err(error) => return Err(error),
        };
    if stale_target {
        if completed_no_op(
            &preview.request,
            context
                .facts(&path)
                .and_then(|facts| facts.progress.as_ref()),
        )? {
            return Ok(ProgressApplyResult {
                path,
                resulting_target_revision: current_entry
                    .map(|entry| entry.content_revision.clone()),
                diff: preview.diff.clone(),
                no_op: true,
            });
        }
        return Err(StoreError::StaleTargetRevision);
    }
    context.require_revisions(&preview.expected_input_revisions)?;
    let checked = prepare(
        root,
        preview.request.clone(),
        &context,
        path.clone(),
        scope_prefix.clone(),
        &log,
        bytes,
    )?;
    if !checked.diagnostics.is_empty()
        || checked.proposed_bytes != preview.proposed_bytes
        || checked.no_op != preview.no_op
    {
        return Err(StoreError::Invalid(
            "progress validation changed after preview".into(),
        ));
    }
    context.require_unchanged()?;
    if preview.no_op {
        return Ok(ProgressApplyResult {
            path,
            resulting_target_revision: current_entry.map(|entry| entry.content_revision.clone()),
            diff: preview.diff.clone(),
            no_op: true,
        });
    }
    let bytes = checked
        .proposed_bytes
        .as_deref()
        .ok_or_else(|| StoreError::Invalid("progress preview has no proposed bytes".into()))?;
    let receipt = crate::mutation_restore::apply(
        root,
        &path,
        current_entry.map(|entry| entry.original_bytes.as_slice()),
        Some(bytes),
    )?;
    let resulting = match context.resulting(&Overlay::from([(path.clone(), Some(bytes.to_vec()))]))
    {
        Ok(resulting) => resulting,
        Err(error) => {
            return Err(crate::mutation_restore::rollback(
                root,
                "progress verification",
                error,
                &[receipt],
            ));
        }
    };
    let diagnostics = scoped_diagnostics(&resulting.diagnostics, &path);
    if !diagnostics.is_empty() {
        return Err(crate::mutation_restore::rollback(
            root,
            "progress validation",
            StoreError::Invalid("post-write progress validation failed".into()),
            &[receipt],
        ));
    }
    Ok(ProgressApplyResult {
        path: path.clone(),
        resulting_target_revision: resulting
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.content_revision.clone()),
        diff: preview.diff.clone(),
        no_op: false,
    })
}

pub(super) fn bootstrap(
    root: &Path,
    investigation: &str,
) -> Result<ProgressChangeRequest, StoreError> {
    let investigation = checked_path(investigation)?;
    progress_path(root, &investigation)?;
    Ok(ProgressChangeRequest {
        investigation,
        entries: Vec::new(),
        replacement: None,
        replacement_source: None,
        bootstrap: true,
    })
}

pub(super) fn validate_investigation(root: &Path, investigation: &str) -> Result<(), StoreError> {
    progress_path(root, investigation).map(|_| ())
}

fn completed_no_op(
    request: &ProgressChangeRequest,
    current: Option<&ProgressLog>,
) -> Result<bool, StoreError> {
    let Some(current) = current else {
        return Ok(false);
    };
    if request.bootstrap {
        return Ok(true);
    }
    if let Some(replacement) = &request.replacement {
        return Ok(current == replacement);
    }
    if let Some(source) = &request.replacement_source {
        return Ok(*current
            == parse_progress_log("progress/log.toml", source).map_err(diagnostics_error)?);
    }
    let ids = current
        .entries
        .iter()
        .map(|entry| (entry.id(), entry))
        .collect::<BTreeMap<_, _>>();
    Ok(request.entries.iter().all(|entry| {
        ids.get(entry.id())
            .is_some_and(|recorded| **recorded == *entry)
    }))
}

fn accepted_ticket_ids(scan: &crate::scanning::ScanResult, scope_prefix: &str) -> Vec<String> {
    let mut result = scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| {
            entry.path.starts_with(scope_prefix)
                && entry.kind == Some(casefile_core::Kind::Ticket)
                && entry.classification == casefile_core::Classification::Governed
        })
        .filter_map(|entry| match &entry.summary {
            Some(casefile_core::RecordSummary::WorkItem { id, status, .. })
                if status == "accepted" =>
            {
                Some(id.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn progress_path(root: &Path, investigation: &str) -> Result<(String, String), StoreError> {
    let investigation = checked_path(investigation)?;
    let (state, active, _) = activation(root)?;
    if state != ActivationState::Active {
        return Err(StoreError::Invalid(
            "progress mutations require an active Casefile activation".into(),
        ));
    }
    if !active.projects.values().any(|project| {
        project
            .investigations
            .iter()
            .any(|value| value == &investigation)
    }) {
        return Err(StoreError::Invalid("investigation is not activated".into()));
    }
    Ok((
        format!("{investigation}/progress/log.toml"),
        format!("{investigation}/"),
    ))
}

fn scoped_diagnostics(diagnostics: &[Diagnostic], prefix: &str) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path.starts_with(prefix))
        .cloned()
        .collect()
}

fn rejected(
    request: ProgressChangeRequest,
    path: String,
    diagnostic: Diagnostic,
) -> ProgressPreview {
    ProgressPreview {
        request,
        path,
        expected_target_revision: None,
        expected_input_revisions: BTreeMap::new(),
        diagnostics: vec![diagnostic],
        diff: String::new(),
        proposed_bytes: None,
        no_op: false,
        bootstrap_ticket_ids: Vec::new(),
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

fn ensure_worktree(root: &Path) -> Result<(), StoreError> {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()?;
    if status.status.success() && String::from_utf8_lossy(&status.stdout).trim() == "true" {
        Ok(())
    } else {
        Err(StoreError::Invalid(
            "progress preview and apply require a real Git worktree".into(),
        ))
    }
}

fn diff(
    root: &Path,
    path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<String, StoreError> {
    git_diff(root, path, before, after)
}

fn capture(
    root: &Path,
    request: &ProgressChangeRequest,
    applying: bool,
) -> Result<(MutationContext, ProgressLog, Vec<u8>), StoreError> {
    let (path, _) = progress_path(root, &request.investigation)?;
    let existing = super::mutation::read_entry(root, &path)?;
    let current_log = match existing.as_ref() {
        Some(entry) => std::str::from_utf8(&entry.original_bytes)
            .map_err(|_| {
                vec![Diagnostic::new(
                    &path,
                    "invalid_utf8",
                    "progress log must be UTF-8",
                )]
            })
            .and_then(|text| {
                parse_progress_log(&path, text).map(|log| Some(std::sync::Arc::new(log)))
            }),
        None => Ok(None),
    };
    let mut log = if request.replacement.is_some() || request.replacement_source.is_some() {
        ProgressLog {
            entries: Vec::new(),
        }
    } else {
        let current = current_log
            .as_ref()
            .map_err(|diagnostics| diagnostics_error(diagnostics.clone()))?;
        ProgressLog {
            entries: current
                .as_ref()
                .map(|log| log.entries.clone())
                .unwrap_or_default(),
        }
    };
    let mut ids = log
        .entries
        .iter()
        .map(|entry| entry.id().to_owned())
        .collect::<BTreeSet<_>>();
    for entry in &request.entries {
        if ids.insert(entry.id().to_owned()) {
            log.entries.push(entry.clone());
        }
    }
    let log = match (&request.replacement, &request.replacement_source) {
        (Some(replacement), _) => replacement.clone(),
        (None, Some(source)) => parse_progress_log(&path, source).map_err(diagnostics_error)?,
        (None, None) => log,
    };
    // Validation diagnostics retain the preview channel; preparation reports them after capture.
    let bytes = render_progress_log(&log).into_bytes();
    let extra = if request.bootstrap {
        super::mutation_dependencies::accepted_paths(root, &request.investigation)?
    } else {
        Vec::new()
    };
    let expected = existing
        .as_ref()
        .map(|entry| entry.content_revision.clone());
    let context = MutationContext::capture_seeded(
        root,
        &Overlay::from([(path.clone(), Some(bytes.clone()))]),
        &extra,
        applying,
        Some((
            path.clone(),
            super::mutation_dependencies::ProgressInput::new(
                existing,
                current_log.map(|log| log.map(super::mutation_dependencies::ProgressFacts::Full)),
            ),
        )),
        Some((&path, &log)),
    )?;
    if context.revisions().get(&path).and_then(Option::as_ref) != expected.as_ref() {
        return Err(StoreError::StaleTargetRevision);
    }
    Ok((context, log, bytes))
}
