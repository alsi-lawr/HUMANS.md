use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use casefile_core::{
    Diagnostic, ProgressEntry, ProgressLog, Revision, parse_progress_log, render_progress_log,
    validate_progress_log,
};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    activation::{ActivationState, activation},
    scanning::scan,
    store::StoreError,
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
    pub expected_store_revision: Revision,
    pub proposed_store_revision: Revision,
    pub diagnostics: Vec<Diagnostic>,
    pub diff: String,
    pub no_op: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bootstrap_ticket_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressApplyResult {
    pub path: String,
    pub resulting_target_revision: Option<Revision>,
    pub resulting_store_revision: Revision,
    pub diff: String,
    pub no_op: bool,
}

pub(super) fn preview(
    root: &Path,
    request: ProgressChangeRequest,
) -> Result<ProgressPreview, StoreError> {
    ensure_worktree(root)?;
    let (path, scope_prefix) = progress_path(root, &request.investigation)?;
    let before = scan(root, &BTreeMap::new())?;
    let existing = before
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path);
    let replacing = request.replacement.is_some() || request.replacement_source.is_some();
    let existing_log = if replacing {
        ProgressLog {
            entries: Vec::new(),
        }
    } else {
        match existing {
            Some(entry) => parse_progress_log(
                &path,
                std::str::from_utf8(&entry.original_bytes)
                    .map_err(|_| StoreError::Invalid("progress log must be UTF-8".into()))?,
            )
            .map_err(diagnostics_error)?,
            None => ProgressLog {
                entries: Vec::new(),
            },
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
                before.snapshot.revision,
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
            // original bytes and both revisions remain the preview/apply result.
            let _ = parse_progress_log(
                &path,
                std::str::from_utf8(&existing.original_bytes)
                    .map_err(|_| StoreError::Invalid("progress log must be UTF-8".into()))?,
            )
            .map_err(diagnostics_error)?;
            let diagnostics = scoped_diagnostics(&before.diagnostics, &scope_prefix);
            return Ok(ProgressPreview {
                request,
                path,
                expected_target_revision: Some(existing.content_revision.clone()),
                expected_store_revision: before.snapshot.revision.clone(),
                proposed_store_revision: before.snapshot.revision,
                no_op: diagnostics.is_empty(),
                diagnostics,
                diff: String::new(),
                bootstrap_ticket_ids: Vec::new(),
            });
        }
    }
    let proposed_log = if replacing {
        if !request.entries.is_empty()
            || (request.replacement.is_some() && request.replacement_source.is_some())
        {
            return Ok(rejected(
                request,
                path,
                before.snapshot.revision,
                Diagnostic::new(
                    "progress/log.toml",
                    "invalid_progress_request",
                    "replacement cannot be combined with entries",
                ),
            ));
        }
        match (&request.replacement, &request.replacement_source) {
            (Some(replacement), None) => replacement.clone(),
            (None, Some(source)) => parse_progress_log(&path, source).map_err(diagnostics_error)?,
            _ => unreachable!("exclusive replacement source"),
        }
    } else {
        let mut entries = existing_log.entries.clone();
        let existing_by_id = entries
            .iter()
            .map(|entry| (entry.id().to_owned(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut requested = BTreeSet::new();
        for entry in &request.entries {
            if !requested.insert(entry.id()) {
                return Ok(rejected(
                    request,
                    path,
                    before.snapshot.revision,
                    Diagnostic::new(
                        "progress/log.toml",
                        "invalid_progress_operation_id",
                        "operation IDs must be unique",
                    ),
                ));
            }
            if let Some(current) = existing_by_id.get(entry.id()) {
                if *current != *entry {
                    return Ok(rejected(
                        request,
                        path,
                        before.snapshot.revision,
                        Diagnostic::new(
                            "progress/log.toml",
                            "conflicting_progress_operation_id",
                            "operation ID is already recorded with different content",
                        ),
                    ));
                }
            } else {
                entries.push(entry.clone());
            }
        }
        ProgressLog { entries }
    };
    if let Err(diagnostic) = validate_progress_log(&path, &proposed_log) {
        return Ok(rejected(
            request,
            path,
            before.snapshot.revision,
            diagnostic,
        ));
    }
    let bytes = render_progress_log(&proposed_log).into_bytes();
    let same = existing.is_some_and(|entry| entry.original_bytes == bytes);
    let mut overlay = BTreeMap::new();
    overlay.insert(path.clone(), Some(bytes.clone()));
    let proposed = scan(root, &overlay)?;
    let diagnostics = scoped_diagnostics(&proposed.diagnostics, &scope_prefix);
    let bootstrap_ticket_ids = if request.bootstrap {
        accepted_ticket_ids(&before, &scope_prefix)
    } else {
        Vec::new()
    };
    if !diagnostics.is_empty() {
        return Ok(ProgressPreview {
            request,
            path,
            expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
            expected_store_revision: before.snapshot.revision.clone(),
            proposed_store_revision: proposed.snapshot.revision,
            diagnostics,
            diff: String::new(),
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
        expected_store_revision: before.snapshot.revision,
        proposed_store_revision: proposed.snapshot.revision,
        diagnostics,
        diff,
        no_op: same,
        bootstrap_ticket_ids,
    })
}

pub(super) fn apply(
    root: &Path,
    preview: ProgressPreview,
) -> Result<ProgressApplyResult, StoreError> {
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
    let current = scan(root, &BTreeMap::new())?;
    let current_entry = current
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path);
    let stale_store = current.snapshot.revision != preview.expected_store_revision;
    let stale_target = current_entry.map(|entry| &entry.content_revision)
        != preview.expected_target_revision.as_ref();
    if stale_store || stale_target {
        if completed_no_op(&preview.request, current_entry)? {
            return Ok(ProgressApplyResult {
                path,
                resulting_target_revision: current_entry
                    .map(|entry| entry.content_revision.clone()),
                resulting_store_revision: current.snapshot.revision,
                diff: preview.diff,
                no_op: true,
            });
        }
        return Err(if stale_store {
            StoreError::StaleStoreRevision
        } else {
            StoreError::StaleTargetRevision
        });
    }
    if preview.no_op {
        return Ok(ProgressApplyResult {
            path,
            resulting_target_revision: current_entry.map(|entry| entry.content_revision.clone()),
            resulting_store_revision: current.snapshot.revision,
            diff: preview.diff,
            no_op: true,
        });
    }
    let log = materialize(&preview.request, current_entry)?;
    validate_progress_log(&path, &log)
        .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?;
    let bytes = render_progress_log(&log).into_bytes();
    atomic_write(root, &path, &bytes)?;
    let resulting = scan(root, &BTreeMap::new())?;
    let diagnostics = scoped_diagnostics(&resulting.diagnostics, &scope_prefix);
    if !diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "post-write progress validation failed".into(),
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
        resulting_store_revision: resulting.snapshot.revision,
        diff: preview.diff,
        no_op: false,
    })
}

pub(super) fn bootstrap(
    root: &Path,
    investigation: &str,
) -> Result<ProgressChangeRequest, StoreError> {
    progress_path(root, investigation)?;
    Ok(ProgressChangeRequest {
        investigation: investigation.into(),
        entries: Vec::new(),
        replacement: None,
        replacement_source: None,
        bootstrap: true,
    })
}

pub(super) fn validate_investigation(root: &Path, investigation: &str) -> Result<(), StoreError> {
    progress_path(root, investigation).map(|_| ())
}

fn materialize(
    request: &ProgressChangeRequest,
    current: Option<&casefile_core::EntrySnapshot>,
) -> Result<ProgressLog, StoreError> {
    if let Some(replacement) = &request.replacement {
        return Ok(replacement.clone());
    }
    if let Some(source) = &request.replacement_source {
        return parse_progress_log("progress/log.toml", source).map_err(diagnostics_error);
    }
    let mut log = match current {
        Some(entry) => parse_progress_log(
            "progress/log.toml",
            std::str::from_utf8(&entry.original_bytes)
                .map_err(|_| StoreError::Invalid("progress log must be UTF-8".into()))?,
        )
        .map_err(diagnostics_error)?,
        None => ProgressLog {
            entries: Vec::new(),
        },
    };
    let ids = log
        .entries
        .iter()
        .map(|entry| (entry.id().to_owned(), entry.clone()))
        .collect::<BTreeMap<_, _>>();
    for entry in &request.entries {
        match ids.get(entry.id()) {
            Some(current) if *current == *entry => {}
            Some(_) => {
                return Err(StoreError::Invalid(
                    "conflicting progress operation ID".into(),
                ));
            }
            None => log.entries.push(entry.clone()),
        }
    }
    Ok(log)
}

fn completed_no_op(
    request: &ProgressChangeRequest,
    current: Option<&casefile_core::EntrySnapshot>,
) -> Result<bool, StoreError> {
    let Some(current) = current else {
        return Ok(false);
    };
    let current = parse_progress_log(
        "progress/log.toml",
        std::str::from_utf8(&current.original_bytes)
            .map_err(|_| StoreError::Invalid("progress log must be UTF-8".into()))?,
    )
    .map_err(diagnostics_error)?;
    if request.bootstrap {
        return Ok(true);
    }
    if let Some(replacement) = &request.replacement {
        return Ok(&current == replacement);
    }
    if let Some(source) = &request.replacement_source {
        return Ok(current
            == parse_progress_log("progress/log.toml", source).map_err(diagnostics_error)?);
    }
    Ok(request
        .entries
        .iter()
        .all(|entry| current.entries.iter().any(|recorded| recorded == entry)))
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
    if !crate::layout::safe_relative(investigation) {
        return Err(StoreError::Invalid(
            "investigation path must be contained".into(),
        ));
    }
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
            .any(|value| value == investigation)
    }) {
        return Err(StoreError::Invalid("investigation is not activated".into()));
    }
    Ok((
        format!("{}/progress/log.toml", investigation.trim_end_matches('/')),
        format!("{}/", investigation.trim_end_matches('/')),
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
    revision: Revision,
    diagnostic: Diagnostic,
) -> ProgressPreview {
    ProgressPreview {
        request,
        path,
        expected_target_revision: None,
        expected_store_revision: revision.clone(),
        proposed_store_revision: revision,
        diagnostics: vec![diagnostic],
        diff: String::new(),
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
    // Delegate canonical diff normalisation to the existing generic writer implementation by using its stable path shape.
    let old = before
        .map(|bytes| {
            tempfile::NamedTempFile::new_in(root).and_then(|mut file| {
                file.write_all(bytes)?;
                Ok(file)
            })
        })
        .transpose()?;
    let new = after
        .map(|bytes| {
            tempfile::NamedTempFile::new_in(root).and_then(|mut file| {
                file.write_all(bytes)?;
                Ok(file)
            })
        })
        .transpose()?;
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(["diff", "--no-index", "--"])
        .arg(
            old.as_ref()
                .map(|file| file.path())
                .unwrap_or_else(|| Path::new("/dev/null")),
        )
        .arg(
            new.as_ref()
                .map(|file| file.path())
                .unwrap_or_else(|| Path::new("/dev/null")),
        )
        .output()?;
    if output.status.code().is_some_and(|code| code > 1) {
        return Err(StoreError::Invalid(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    let before_exists = before.is_some();
    let after_exists = after.is_some();
    let source = String::from_utf8_lossy(&output.stdout);
    Ok(source
        .lines()
        .map(|line| {
            if line.starts_with("diff --git ") {
                format!("diff --git a/{path} b/{path}")
            } else if line.starts_with("--- ") {
                if before_exists {
                    format!("--- a/{path}")
                } else {
                    "--- /dev/null".into()
                }
            } else if line.starts_with("+++ ") {
                if after_exists {
                    format!("+++ b/{path}")
                } else {
                    "+++ /dev/null".into()
                }
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if source.ends_with('\n') { "\n" } else { "" })
}

fn atomic_write(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), StoreError> {
    let target = root.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| StoreError::Invalid("progress target has no parent".into()))?;
    let mut current = PathBuf::new();
    for component in parent.components() {
        current.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(StoreError::Invalid(
                "progress target path must not contain a symlink".into(),
            ));
        }
    }
    fs::create_dir_all(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(&target)
        && (!metadata.file_type().is_file() || metadata.file_type().is_symlink())
    {
        return Err(StoreError::Invalid(
            "progress target must be a regular non-symlink file".into(),
        ));
    }
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary
        .persist(&target)
        .map_err(|error| StoreError::Io(error.error))?;
    Ok(())
}
