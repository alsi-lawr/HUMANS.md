use crate::{
    layout::{checked_path, kind_for_path},
    mutation::{MutationContext, Overlay},
    revision::require_target_revision,
    store::StoreError,
};
use casefile_core::{
    ApplyResult, ChangeBatchApplyResult, ChangeBatchPreview, ChangeRequest, Diagnostic, Kind,
    Preview, stable,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};
use tempfile::NamedTempFile;

pub(super) fn preview(root: &Path, request: ChangeRequest) -> Result<Preview, StoreError> {
    let batch = preview_batch(root, vec![request])?;
    let request = batch.requests.into_iter().next().expect("one request");
    Ok(Preview {
        expected_target_revision: batch
            .expected_target_revisions
            .get(request.path())
            .cloned()
            .flatten(),
        expected_input_revisions: batch.expected_input_revisions,
        request,
        diagnostics: batch.diagnostics,
        diff: batch.diff,
    })
}

pub(super) fn preview_batch(
    root: &Path,
    requests: Vec<ChangeRequest>,
) -> Result<ChangeBatchPreview, StoreError> {
    if requests.is_empty() {
        return Err(StoreError::Invalid(
            "record batch requires at least one request".into(),
        ));
    }
    let (requests, overlay, diagnostics) = preflight(requests)?;
    if !diagnostics.is_empty() {
        return Ok(ChangeBatchPreview {
            requests,
            diagnostics,
            expected_target_revisions: BTreeMap::new(),
            expected_input_revisions: BTreeMap::new(),
            diff: String::new(),
        });
    }
    let (requests, overlay) = canonical_batch(root, requests, overlay)?;
    ensure_worktree(root)?;
    let context = MutationContext::capture(root, &overlay, &[], false)?;
    let mut result = prepare_batch(root, &requests, &overlay, &context)?;
    result.requests = requests;
    Ok(result)
}

fn prepare_batch(
    root: &Path,
    requests: &[ChangeRequest],
    rendered: &Overlay,
    context: &MutationContext,
) -> Result<ChangeBatchPreview, StoreError> {
    let before = &context.before;
    let active = &context.active;
    let mut paths = BTreeSet::new();
    let mut expected_target_revisions = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for request in requests {
        let path = checked_path(request.path())?;
        if !paths.insert(path.clone()) {
            diagnostics.push(Diagnostic::new(
                &path,
                "duplicate_target",
                "batch requests must target distinct canonical paths",
            ));
            continue;
        }
        let existing = context.entry(&path);
        expected_target_revisions.insert(
            path.clone(),
            existing.map(|entry| entry.content_revision.clone()),
        );
        let writable = match request {
            ChangeRequest::Create { draft, .. } | ChangeRequest::Replace { draft, .. } => {
                Some(draft.kind())
            }
            ChangeRequest::Delete { .. } => existing.and_then(|entry| entry.kind),
        };
        if !writable.is_some_and(Kind::is_writable) || kind_for_path(&path, active) != writable {
            diagnostics.push(Diagnostic::new(
                &path,
                "read_only_or_wrong_path",
                "only complete ticket, epic, and board drafts may target their canonical path",
            ));
            continue;
        }
        let target_diagnostic = match request {
            ChangeRequest::Create { .. } if existing.is_some() => Some(Diagnostic::new(
                &path,
                "target_exists",
                "create requires an absent target",
            )),
            ChangeRequest::Replace { .. } if existing.is_none() => Some(Diagnostic::new(
                &path,
                "target_missing",
                "replace requires an existing target",
            )),
            ChangeRequest::Delete { .. } if existing.is_none() => Some(Diagnostic::new(
                &path,
                "target_missing",
                "delete requires an existing target",
            )),
            _ => None,
        };
        if let Some(diagnostic) = target_diagnostic {
            diagnostics.push(diagnostic);
            continue;
        }
    }
    if !diagnostics.is_empty() {
        return Ok(ChangeBatchPreview {
            requests: Vec::new(),
            expected_target_revisions,
            expected_input_revisions: context.revisions(),
            diagnostics: stable(diagnostics),
            diff: String::new(),
        });
    }
    let proposed = context.overlay(rendered);
    let diagnostics = introduced_diagnostics(&before.diagnostics, &proposed.diagnostics);
    let mut diff = String::new();
    for request in requests {
        let path = request.path();
        let existing = context.entry(path);
        diff.push_str(&git_diff(
            root,
            path,
            existing.map(|entry| entry.original_bytes.as_slice()),
            rendered.get(path).and_then(Option::as_deref),
        )?);
    }
    Ok(ChangeBatchPreview {
        requests: Vec::new(),
        expected_target_revisions,
        expected_input_revisions: context.revisions(),
        diagnostics: stable(diagnostics),
        diff,
    })
}

pub(super) fn introduced_diagnostics(
    baseline: &[Diagnostic],
    proposed: &[Diagnostic],
) -> Vec<Diagnostic> {
    let mut remaining_baseline = BTreeMap::new();
    for diagnostic in baseline {
        *remaining_baseline
            .entry(diagnostic_key(diagnostic))
            .or_insert(0) += 1;
    }
    proposed
        .iter()
        .filter_map(|diagnostic| {
            let count = remaining_baseline
                .entry(diagnostic_key(diagnostic))
                .or_insert(0);
            if *count == 0 {
                Some(diagnostic.clone())
            } else {
                *count -= 1;
                None
            }
        })
        .collect()
}

fn diagnostic_key(
    diagnostic: &Diagnostic,
) -> (u32, String, String, Option<String>, Option<String>, String) {
    (
        diagnostic.schema_version,
        diagnostic.path.clone(),
        diagnostic.code.clone(),
        diagnostic.field.clone(),
        diagnostic.section.clone(),
        diagnostic.message.clone(),
    )
}

pub(super) fn apply(root: &Path, mut preview: Preview) -> Result<ApplyResult, StoreError> {
    preview.request = canonical_request(root, preview.request)?;
    let path = preview.request.path().to_owned();
    let result = apply_batch(
        root,
        ChangeBatchPreview {
            requests: vec![preview.request],
            expected_target_revisions: BTreeMap::from([(
                path.clone(),
                preview.expected_target_revision,
            )]),
            expected_input_revisions: preview.expected_input_revisions,
            diagnostics: preview.diagnostics,
            diff: preview.diff,
        },
    )?;
    Ok(ApplyResult {
        resulting_target_revision: result
            .resulting_target_revisions
            .get(&path)
            .cloned()
            .flatten(),
        path,
        diff: result.diff,
    })
}

struct BatchMutation<'a> {
    path: String,
    proposed: Option<&'a [u8]>,
    original: Option<&'a [u8]>,
}

pub(super) fn apply_batch(
    root: &Path,
    mut preview: ChangeBatchPreview,
) -> Result<ChangeBatchApplyResult, StoreError> {
    if preview.requests.is_empty() {
        return Err(StoreError::Invalid(
            "record batch requires at least one request".into(),
        ));
    }
    let (requests, overlay, diagnostics) = preflight(preview.requests)?;
    if !diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "record batch request is invalid".into(),
        ));
    }
    let (requests, overlay) = canonical_batch(root, requests, overlay)?;
    preview.requests = requests;
    let mut expected_target_revisions = BTreeMap::new();
    for (path, revision) in preview.expected_target_revisions {
        let canonical = checked_path(&path)?;
        if expected_target_revisions
            .insert(canonical, revision)
            .is_some()
        {
            return Err(StoreError::Invalid(
                "record batch target revisions contain duplicate canonical paths".into(),
            ));
        }
    }
    preview.expected_target_revisions = expected_target_revisions;
    ensure_worktree(root)?;
    if !preview.diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "preview contains validation diagnostics".into(),
        ));
    }
    let context = MutationContext::capture(root, &overlay, &[], true)?;
    context.require_revisions(&preview.expected_input_revisions)?;
    let checked = prepare_batch(root, &preview.requests, &overlay, &context)?;
    if !checked.diagnostics.is_empty() || checked.diff != preview.diff {
        return Err(StoreError::Invalid(
            "record batch validation changed after preview".into(),
        ));
    }
    if preview.expected_target_revisions.len() != preview.requests.len() {
        return Err(StoreError::Invalid(
            "record batch target revisions are incomplete".into(),
        ));
    }
    let mut paths = BTreeSet::new();
    let mut mutations = Vec::with_capacity(preview.requests.len());
    for request in &preview.requests {
        let path = checked_path(request.path())?;
        if !paths.insert(path.clone()) {
            return Err(StoreError::Invalid(
                "record batch targets must be distinct".into(),
            ));
        }
        let current_entry = context.entry(&path);
        let expected = preview
            .expected_target_revisions
            .get(&path)
            .ok_or_else(|| StoreError::Invalid("record batch target revision is missing".into()))?;
        require_target_revision(&root.join(&path), expected.as_ref())?;
        let target = root.join(&path);
        let proposed = overlay.get(&path).and_then(Option::as_deref);
        match request {
            ChangeRequest::Create { .. } => match fs::symlink_metadata(&target) {
                Ok(_) => {
                    return Err(StoreError::Invalid(
                        "create target appeared after preview".into(),
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
            ChangeRequest::Replace { .. } | ChangeRequest::Delete { .. } => {
                let metadata = fs::symlink_metadata(&target)?;
                if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                    return Err(StoreError::Invalid(
                        "replace and delete require regular non-symlink targets".into(),
                    ));
                }
            }
        }
        if proposed.is_some()
            && let Some(parent) = target.parent()
        {
            fs::create_dir_all(parent)?;
        }
        mutations.push(BatchMutation {
            path,
            proposed,
            original: current_entry.map(|entry| entry.original_bytes.as_slice()),
        });
    }
    context.require_unchanged()?;
    let mut receipts = Vec::new();
    for mutation in &mutations {
        if mutation.proposed == mutation.original {
            continue;
        }
        match crate::mutation_restore::apply(
            root,
            &mutation.path,
            mutation.original,
            mutation.proposed,
        ) {
            Ok(receipt) => receipts.push(receipt),
            Err(error) => {
                return Err(crate::mutation_restore::rollback(
                    root,
                    "record batch write",
                    error,
                    &receipts,
                ));
            }
        }
    }
    let resulting = match context.resulting(&overlay) {
        Ok(resulting) => resulting,
        Err(error) => {
            return Err(crate::mutation_restore::rollback(
                root,
                "record batch verification",
                error,
                &receipts,
            ));
        }
    };
    let resulting_entries = resulting
        .snapshot
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let resulting_target_revisions = mutations
        .iter()
        .map(|mutation| {
            (
                mutation.path.clone(),
                resulting_entries
                    .get(mutation.path.as_str())
                    .map(|entry| entry.content_revision.clone()),
            )
        })
        .collect();
    Ok(ChangeBatchApplyResult {
        paths: mutations
            .into_iter()
            .map(|mutation| mutation.path)
            .collect(),
        resulting_target_revisions,
        diff: preview.diff,
    })
}

fn canonical_request(root: &Path, request: ChangeRequest) -> Result<ChangeRequest, StoreError> {
    Ok(match request {
        ChangeRequest::Create { path, draft } => ChangeRequest::Create {
            path: super::mutation_locks::canonical_target(root, &checked_path(&path)?)?,
            draft,
        },
        ChangeRequest::Replace { path, draft } => ChangeRequest::Replace {
            path: super::mutation_locks::canonical_target(root, &checked_path(&path)?)?,
            draft,
        },
        ChangeRequest::Delete { path } => ChangeRequest::Delete {
            path: super::mutation_locks::canonical_target(root, &checked_path(&path)?)?,
        },
    })
}

fn preflight(
    requests: Vec<ChangeRequest>,
) -> Result<(Vec<ChangeRequest>, Overlay, Vec<Diagnostic>), StoreError> {
    let mut paths = BTreeSet::new();
    let mut rendered = Overlay::new();
    let mut diagnostics = Vec::new();
    let mut normalized = Vec::with_capacity(requests.len());
    for request in requests {
        let path = checked_path(request.path())?;
        if !paths.insert(path.clone()) {
            diagnostics.push(Diagnostic::new(
                &path,
                "duplicate_target",
                "batch requests must target distinct canonical paths",
            ));
        }
        let request = match request {
            ChangeRequest::Create { draft, .. } => ChangeRequest::Create {
                path: path.clone(),
                draft,
            },
            ChangeRequest::Replace { draft, .. } => ChangeRequest::Replace {
                path: path.clone(),
                draft,
            },
            ChangeRequest::Delete { .. } => ChangeRequest::Delete { path: path.clone() },
        };
        match request.rendered() {
            Some(Ok(bytes)) => {
                rendered.insert(path, Some(bytes));
            }
            Some(Err(diagnostic)) => diagnostics.push(diagnostic),
            None => {
                rendered.insert(path, None);
            }
        }
        normalized.push(request);
    }
    Ok((normalized, rendered, stable(diagnostics)))
}

fn canonical_batch(
    root: &Path,
    requests: Vec<ChangeRequest>,
    mut rendered: Overlay,
) -> Result<(Vec<ChangeRequest>, Overlay), StoreError> {
    let mut normalized = Vec::with_capacity(requests.len());
    let mut overlay = Overlay::new();
    for request in requests {
        let bytes = rendered.remove(request.path()).flatten();
        let request = canonical_request(root, request)?;
        if overlay.insert(request.path().into(), bytes).is_some() {
            return Err(StoreError::Invalid(
                "mutation targets alias the same canonical file".into(),
            ));
        }
        normalized.push(request);
    }
    Ok((normalized, overlay))
}

pub(super) fn ensure_worktree(root: &Path) -> Result<(), StoreError> {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()?;
    if status.status.success() && String::from_utf8_lossy(&status.stdout).trim() == "true" {
        Ok(())
    } else {
        Err(StoreError::Invalid(
            "apply and preview require a real Git worktree".into(),
        ))
    }
}
pub(super) fn git_diff(
    root: &Path,
    path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<String, StoreError> {
    if before == after {
        return Ok(String::new());
    }
    let old = before.map(|bytes| temp(root, bytes)).transpose()?;
    let new = after.map(|bytes| temp(root, bytes)).transpose()?;
    let old_path = old
        .as_ref()
        .map(|file| contained_git_argument(root, file.path()))
        .transpose()?
        .unwrap_or_else(|| PathBuf::from("/dev/null"));
    let new_path = new
        .as_ref()
        .map(|file| contained_git_argument(root, file.path()))
        .transpose()?
        .unwrap_or_else(|| PathBuf::from("/dev/null"));
    let output = Command::new("git")
        .current_dir(root)
        .args(["diff", "--no-index", "--"])
        .arg(&old_path)
        .arg(&new_path)
        .output()?;
    require_diff_success(&output)?;
    Ok(canonical_diff(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        path,
        before.is_some(),
        after.is_some(),
    ))
}

fn require_diff_success(output: &std::process::Output) -> Result<(), StoreError> {
    if matches!(output.status.code(), Some(0 | 1)) {
        return Ok(());
    }
    Err(StoreError::Invalid(format!(
        "approval diff subprocess failed: {}",
        output.status
    )))
}

fn contained_git_argument(root: &Path, path: &Path) -> Result<PathBuf, StoreError> {
    let absolute_root = absolute_lexical(root)?;
    let absolute_path = absolute_lexical(path)?;
    let relative = absolute_path.strip_prefix(&absolute_root).map_err(|_| {
        StoreError::Invalid("temporary diff path escaped the configured Store root".into())
    })?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(StoreError::Invalid(
            "temporary diff path escaped the configured Store root".into(),
        ));
    }
    Ok(relative.to_path_buf())
}

fn absolute_lexical(path: &Path) -> Result<PathBuf, StoreError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn canonical_diff(diff: &str, path: &str, before: bool, after: bool) -> String {
    let mut in_hunk = false;
    diff.lines()
        .map(|line| {
            if line.starts_with("@@") {
                in_hunk = true;
            }
            if in_hunk {
                line.into()
            } else if line.starts_with("diff --git ") {
                format!("diff --git a/{path} b/{path}")
            } else if line.starts_with("--- ") {
                if before {
                    format!("--- a/{path}")
                } else {
                    "--- /dev/null".into()
                }
            } else if line.starts_with("+++ ") {
                if after {
                    format!("+++ b/{path}")
                } else {
                    "+++ /dev/null".into()
                }
            } else if line.starts_with("Binary files ") {
                let old = if before {
                    format!("a/{path}")
                } else {
                    "/dev/null".into()
                };
                let new = if after {
                    format!("b/{path}")
                } else {
                    "/dev/null".into()
                };
                format!("Binary files {old} and {new} differ")
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if diff.ends_with('\n') { "\n" } else { "" }
}
fn temp(root: &Path, bytes: &[u8]) -> Result<NamedTempFile, StoreError> {
    let mut file = NamedTempFile::new_in(root)?;
    file.write_all(bytes)?;
    Ok(file)
}

#[cfg(test)]
mod diff_argument_tests {
    use super::*;

    #[test]
    fn contained_temporary_git_arguments_are_relative_and_keep_canonical_headers() {
        let root = tempfile::tempdir().expect("temporary Store");
        let temporary = temp(root.path(), b"before\n").expect("contained temporary file");
        let argument =
            contained_git_argument(root.path(), temporary.path()).expect("relative Git argument");
        assert!(!argument.is_absolute());
        assert_eq!(root.path().join(&argument), temporary.path());
        assert!(
            contained_git_argument(root.path(), root.path().parent().expect("outside parent"))
                .is_err()
        );

        let path = "projects/demo/investigations/sample/progress/log.toml";
        let diff = git_diff(root.path(), path, None, Some(b"after\n")).expect("no-index diff");
        assert!(diff.contains(&format!("diff --git a/{path} b/{path}")));
        assert!(diff.contains("--- /dev/null"));
        assert!(diff.contains(&format!("+++ b/{path}")));
        assert!(!diff.contains(".tmp"));
    }
    #[test]
    fn literal_hunk_lines_are_not_rewritten_as_canonical_file_headers() {
        let root = tempfile::tempdir().unwrap();
        let diff = git_diff(
            root.path(),
            "record.md",
            Some(b"-- removed note\n"),
            Some(b"++ added note\n"),
        )
        .unwrap();
        assert!(diff.contains("--- a/record.md\n+++ b/record.md\n"));
        assert!(diff.contains("--- removed note\n+++ added note\n"));
    }

    #[cfg(unix)]
    #[test]
    fn terminated_and_failed_diff_processes_cannot_publish_partial_approval_output() {
        for script in ["printf partial; kill -TERM $$", "printf partial; exit 2"] {
            let output = Command::new("sh").args(["-c", script]).output().unwrap();
            assert_eq!(output.stdout, b"partial");
            assert!(require_diff_success(&output).is_err());
        }
    }
}
