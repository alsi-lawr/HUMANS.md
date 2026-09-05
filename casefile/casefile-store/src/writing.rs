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
    let requests = requests
        .into_iter()
        .map(|request| canonical_request(root, request))
        .collect::<Result<Vec<_>, _>>()?;
    ensure_worktree(root)?;
    let overlay = request_overlay(&requests);
    let context = MutationContext::capture(root, &overlay, &[], false)?;
    prepare_batch(root, requests, &context)
}

fn prepare_batch(
    root: &Path,
    requests: Vec<ChangeRequest>,
    context: &MutationContext,
) -> Result<ChangeBatchPreview, StoreError> {
    let before = &context.before;
    let active = &context.active;
    let mut paths = BTreeSet::new();
    let mut overlay = BTreeMap::new();
    let mut expected_target_revisions = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for request in &requests {
        let path = checked_path(request.path())?;
        if !paths.insert(path.clone()) {
            diagnostics.push(Diagnostic::new(
                &path,
                "duplicate_target",
                "batch requests must target distinct canonical paths",
            ));
            continue;
        }
        let existing = before
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path);
        expected_target_revisions.insert(
            path.clone(),
            existing.map(|entry| entry.content_revision.clone()),
        );
        let proposed_bytes = match request.rendered() {
            Some(Ok(bytes)) => Some(bytes),
            Some(Err(diagnostic)) => {
                diagnostics.push(diagnostic);
                continue;
            }
            None => None,
        };
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
        overlay.insert(path, proposed_bytes);
    }
    if !diagnostics.is_empty() {
        return Ok(ChangeBatchPreview {
            requests,
            expected_target_revisions,
            expected_input_revisions: context.revisions(),
            diagnostics: stable(diagnostics),
            diff: String::new(),
        });
    }
    let proposed = context.overlay(&overlay);
    let diagnostics = introduced_diagnostics(&before.diagnostics, &proposed.diagnostics);
    let mut diff = String::new();
    for request in &requests {
        let path = request.path();
        let existing = before
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path);
        diff.push_str(&git_diff(
            root,
            path,
            existing.map(|entry| entry.original_bytes.as_slice()),
            overlay.get(path).and_then(Option::as_deref),
        )?);
    }
    Ok(ChangeBatchPreview {
        requests,
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

struct BatchMutation {
    path: String,
    target: PathBuf,
    proposed: Option<Vec<u8>>,
    original: Option<Vec<u8>>,
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
    preview.requests = preview
        .requests
        .into_iter()
        .map(|request| canonical_request(root, request))
        .collect::<Result<Vec<_>, _>>()?;
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
    let overlay = request_overlay(&preview.requests);
    let context = MutationContext::capture(root, &overlay, &[], true)?;
    context.require_revisions(&preview.expected_input_revisions)?;
    let checked = prepare_batch(root, preview.requests.clone(), &context)?;
    if !checked.diagnostics.is_empty() || checked.diff != preview.diff {
        return Err(StoreError::Invalid(
            "record batch validation changed after preview".into(),
        ));
    }
    let current = &context.before;
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
        let current_entry = current
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path);
        let expected = preview
            .expected_target_revisions
            .get(&path)
            .ok_or_else(|| StoreError::Invalid("record batch target revision is missing".into()))?;
        require_target_revision(&root.join(&path), expected.as_ref())?;
        let target = root.join(&path);
        let proposed = request
            .rendered()
            .transpose()
            .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?;
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
            target,
            proposed,
            original: current_entry.map(|entry| entry.original_bytes.clone()),
        });
    }
    context.require_unchanged()?;
    let mut applied = Vec::new();
    for (index, mutation) in mutations.iter().enumerate() {
        if mutation.proposed == mutation.original {
            continue;
        }
        let result = apply_mutation(root, mutation);
        if let Err(error) = result {
            rollback_batch(&mutations, &applied).map_err(|rollback| {
                StoreError::Invalid(format!(
                    "record batch failed ({error}); rollback failed ({rollback})"
                ))
            })?;
            return Err(error);
        }
        applied.push(index);
    }
    let resulting = match context.resulting(&overlay) {
        Ok(resulting) => resulting,
        Err(error) => {
            rollback_batch(&mutations, &applied).map_err(|rollback| {
                StoreError::Invalid(format!(
                    "record batch result could not be scanned ({error}); rollback failed ({rollback})"
                ))
            })?;
            return Err(error);
        }
    };
    let resulting_target_revisions = mutations
        .iter()
        .map(|mutation| {
            (
                mutation.path.clone(),
                resulting
                    .snapshot
                    .entries
                    .iter()
                    .find(|entry| entry.path == mutation.path)
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

fn apply_mutation(root: &Path, mutation: &BatchMutation) -> Result<(), StoreError> {
    #[cfg(test)]
    crate::mutation_hooks::writing(root, &mutation.path)?;
    let _ = root;
    match &mutation.proposed {
        Some(bytes) => atomic_write(&mutation.target, bytes, mutation.original.is_none()),
        None => fs::remove_file(&mutation.target).map_err(StoreError::from),
    }
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

fn rollback_batch(mutations: &[BatchMutation], applied: &[usize]) -> Result<(), StoreError> {
    for index in applied.iter().rev() {
        let mutation = &mutations[*index];
        match &mutation.original {
            Some(bytes) => atomic_write(&mutation.target, bytes, !mutation.target.exists())?,
            None => match fs::symlink_metadata(&mutation.target) {
                Ok(metadata)
                    if metadata.file_type().is_file() && !metadata.file_type().is_symlink() =>
                {
                    fs::remove_file(&mutation.target)?;
                }
                Ok(_) => {
                    return Err(StoreError::Invalid(
                        "rollback target is not a regular file".into(),
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
        }
    }
    Ok(())
}

fn request_overlay(requests: &[ChangeRequest]) -> Overlay {
    requests
        .iter()
        .map(|request| {
            (
                request.path().to_owned(),
                request.rendered().and_then(Result::ok),
            )
        })
        .collect()
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
    if output.status.code().is_some_and(|code| code > 1) {
        return Err(StoreError::Invalid(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    Ok(canonical_diff(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        path,
        before.is_some(),
        after.is_some(),
    ))
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
    diff.lines()
        .map(|line| {
            if line.starts_with("diff --git ") {
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

fn atomic_write(target: &Path, bytes: &[u8], create: bool) -> Result<(), StoreError> {
    let parent = target
        .parent()
        .ok_or_else(|| StoreError::Invalid("target has no parent".into()))?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    if create {
        temporary
            .persist_noclobber(target)
            .map_err(|error| StoreError::Io(error.error))?;
    } else {
        temporary
            .persist(target)
            .map_err(|error| StoreError::Io(error.error))?;
    }
    Ok(())
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
}
