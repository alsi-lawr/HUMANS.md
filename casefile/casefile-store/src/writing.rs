use crate::{
    activation::activation,
    layout::{checked_path, kind_for_path},
    scanning::scan,
    store::StoreError,
};
use casefile_core::{ApplyResult, ChangeRequest, Diagnostic, Kind, Preview, Revision, stable};
use std::{collections::BTreeMap, ffi::OsStr, fs, io::Write, path::Path, process::Command};
use tempfile::NamedTempFile;

pub(super) fn preview(root: &Path, request: ChangeRequest) -> Result<Preview, StoreError> {
    ensure_worktree(root)?;
    let before = scan(root, &BTreeMap::new())?;
    let path = checked_path(request.path())?;
    let existing = before
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path);
    let proposed_bytes = match request.rendered() {
        Some(Ok(bytes)) => bytes,
        Some(Err(diagnostic)) => {
            return Ok(rejected(request, before.snapshot.revision, diagnostic));
        }
        None => Vec::new(),
    };
    let writable = match &request {
        ChangeRequest::Create { draft, .. } | ChangeRequest::Replace { draft, .. } => {
            Some(draft.kind())
        }
        ChangeRequest::Delete { .. } => existing.and_then(|entry| entry.kind),
    };
    let path_kind = kind_for_path(&path, &activation(root)?.1);
    if !writable.is_some_and(Kind::is_writable) || path_kind != writable {
        return Ok(rejected(
            request,
            before.snapshot.revision,
            Diagnostic::new(
                &path,
                "read_only_or_wrong_path",
                "only complete ticket, epic, and board drafts may target their canonical path",
            ),
        ));
    }
    match &request {
        ChangeRequest::Create { .. } if existing.is_some() => {
            return Ok(rejected(
                request,
                before.snapshot.revision,
                Diagnostic::new(&path, "target_exists", "create requires an absent target"),
            ));
        }
        ChangeRequest::Replace { .. } if existing.is_none() => {
            return Ok(rejected(
                request,
                before.snapshot.revision,
                Diagnostic::new(
                    &path,
                    "target_missing",
                    "replace requires an existing target",
                ),
            ));
        }
        ChangeRequest::Delete { .. } if existing.is_none() => {
            return Ok(rejected(
                request,
                before.snapshot.revision,
                Diagnostic::new(
                    &path,
                    "target_missing",
                    "delete requires an existing target",
                ),
            ));
        }
        _ => {}
    }
    let mut overlay = BTreeMap::new();
    overlay.insert(
        path.clone(),
        if matches!(request, ChangeRequest::Delete { .. }) {
            None
        } else {
            Some(proposed_bytes.clone())
        },
    );
    let proposed = scan(root, &overlay)?;
    let mut diagnostics = proposed.diagnostics;
    if diagnostics.is_empty() {
        diagnostics = Vec::new();
    }
    let diff = git_diff(
        root,
        &path,
        existing.map(|entry| entry.original_bytes.as_slice()),
        if matches!(request, ChangeRequest::Delete { .. }) {
            None
        } else {
            Some(proposed_bytes.as_slice())
        },
    )?;
    Ok(Preview {
        request,
        expected_target_revision: existing.map(|entry| entry.content_revision.clone()),
        expected_store_revision: before.snapshot.revision,
        proposed_store_revision: proposed.snapshot.revision,
        diagnostics: stable(diagnostics),
        diff,
    })
}

pub(super) fn apply(root: &Path, preview: Preview) -> Result<ApplyResult, StoreError> {
    ensure_worktree(root)?;
    if !preview.diagnostics.is_empty() {
        return Err(StoreError::Invalid(
            "preview contains validation diagnostics".into(),
        ));
    }
    let current = scan(root, &BTreeMap::new())?;
    if current.snapshot.revision != preview.expected_store_revision {
        return Err(StoreError::StaleStoreRevision);
    }
    let path = checked_path(preview.request.path())?;
    let current_entry = current
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path);
    if current_entry.map(|entry| &entry.content_revision)
        != preview.expected_target_revision.as_ref()
    {
        return Err(StoreError::StaleTargetRevision);
    }
    let target = root.join(&path);
    match &preview.request {
        ChangeRequest::Create { draft, .. } | ChangeRequest::Replace { draft, .. } => {
            let bytes = casefile_core::render_draft(&path, draft)
                .map_err(|diagnostic| StoreError::Invalid(diagnostic.message))?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if target.exists() && fs::symlink_metadata(&target)?.file_type().is_symlink() {
                return Err(StoreError::Invalid("target must not be a symlink".into()));
            }
            atomic_write(
                &target,
                &bytes,
                matches!(preview.request, ChangeRequest::Create { .. }),
            )?;
        }
        ChangeRequest::Delete { .. } => {
            let metadata = fs::symlink_metadata(&target)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(StoreError::Invalid(
                    "delete requires a regular non-symlink target".into(),
                ));
            }
            fs::remove_file(&target)?;
        }
    }
    let resulting = scan(root, &BTreeMap::new())?;
    let target_revision = resulting
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.content_revision.clone());
    Ok(ApplyResult {
        path,
        resulting_target_revision: target_revision,
        resulting_store_revision: resulting.snapshot.revision,
        diff: preview.diff,
    })
}
fn rejected(request: ChangeRequest, revision: Revision, diagnostic: Diagnostic) -> Preview {
    Preview {
        request,
        expected_target_revision: None,
        expected_store_revision: revision.clone(),
        proposed_store_revision: revision,
        diagnostics: vec![diagnostic],
        diff: String::new(),
    }
}

fn ensure_worktree(root: &Path) -> Result<(), StoreError> {
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
fn git_diff(
    root: &Path,
    path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<String, StoreError> {
    let old = before.map(|bytes| temp(root, bytes)).transpose()?;
    let new = after.map(|bytes| temp(root, bytes)).transpose()?;
    let old_path = old
        .as_ref()
        .map(|file| file.path().as_os_str())
        .unwrap_or_else(|| OsStr::new("/dev/null"));
    let new_path = new
        .as_ref()
        .map(|file| file.path().as_os_str())
        .unwrap_or_else(|| OsStr::new("/dev/null"));
    let output = Command::new("git")
        .current_dir(root)
        .args(["diff", "--no-index", "--"])
        .arg(old_path)
        .arg(new_path)
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
