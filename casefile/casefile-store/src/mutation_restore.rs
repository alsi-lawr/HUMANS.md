use crate::{
    mutation::read_entry,
    revision::{filesystem_identity, metadata_revision, open_file_revision},
    store::{
        IncompleteRollback, RollbackCause, RollbackErrorCode, RollbackPathState, RollbackReason,
        RollbackRemainingState, StoreError, require_safe_target_parent,
    },
};
use casefile_core::Revision;
use std::{fs, io::Write, path::Path};
use tempfile::NamedTempFile;

pub(super) struct Receipt<'a> {
    path: &'a str,
    prior: Option<&'a [u8]>,
    written: Option<&'a [u8]>,
    identity: Option<String>,
    revision: Option<Revision>,
}

pub(super) fn apply<'a>(
    root: &Path,
    path: &'a str,
    prior: Option<&'a [u8]>,
    written: Option<&'a [u8]>,
) -> Result<Receipt<'a>, StoreError> {
    let parent = Path::new(path).parent().unwrap_or(Path::new(""));
    #[cfg(test)]
    crate::mutation_hooks::writing(root, path)?;
    require_safe_target_parent(root, parent, "mutation target")?;
    let target = root.join(path);
    let (identity, revision) = match written {
        Some(bytes) => {
            fs::create_dir_all(root.join(parent))?;
            let mut temporary = NamedTempFile::new_in(root.join(parent))?;
            temporary.write_all(bytes)?;
            temporary.flush()?;
            let identity = filesystem_identity(temporary.path())?;
            let file = if prior.is_none() {
                temporary.persist_noclobber(&target)
            } else {
                temporary.persist(&target)
            }
            .map_err(|error| {
                if prior.is_none() && error.error.kind() == std::io::ErrorKind::AlreadyExists {
                    StoreError::StaleTargetRevision
                } else {
                    StoreError::Io(error.error)
                }
            })?;
            // Observe our published inode through its handle, not a pathname an editor can replace.
            let revision = file
                .metadata()
                .ok()
                .and_then(|metadata| open_file_revision(&file, &metadata).ok());
            (Some(identity), revision)
        }
        None => {
            fs::remove_file(&target)?;
            (None, None)
        }
    };
    Ok(Receipt {
        path,
        prior,
        written,
        identity,
        revision,
    })
}

pub(super) fn rollback(
    root: &Path,
    operation: &str,
    cause: StoreError,
    receipts: &[Receipt<'_>],
) -> StoreError {
    let mut affected_paths = Vec::new();
    for receipt in receipts.iter().rev() {
        if let Err(reason) = restore(root, receipt) {
            affected_paths.push(RollbackPathState {
                path: receipt.path.into(),
                remaining: observe(root, receipt.path),
                reason,
            });
        }
    }
    if affected_paths.is_empty() {
        return cause;
    }
    let category = match &cause {
        StoreError::Io(_) => RollbackCause::Io,
        StoreError::Invalid(_) => RollbackCause::Invalid,
        StoreError::StaleTargetRevision => RollbackCause::Stale,
        StoreError::IncompleteRollback { details, .. } => details.cause,
    };
    StoreError::IncompleteRollback {
        details: IncompleteRollback {
            code: RollbackErrorCode::IncompleteRollback,
            operation: operation.into(),
            cause: category,
            affected_paths,
        },
        cause: Box::new(cause),
    }
}

fn restore(root: &Path, receipt: &Receipt<'_>) -> Result<(), RollbackReason> {
    let current = read_entry(root, receipt.path).map_err(|error| match error {
        StoreError::Invalid(_) => RollbackReason::ExternalChange,
        _ => RollbackReason::ObservationFailed,
    })?;
    match (&current, receipt.written) {
        (None, None) => {}
        (Some(current), Some(written)) => {
            let Some(expected_revision) = &receipt.revision else {
                return Err(RollbackReason::ObservationFailed);
            };
            let identity = filesystem_identity(&root.join(receipt.path))
                .map_err(|_| RollbackReason::ObservationFailed)?;
            if current.original_bytes != written
                || current.content_revision != *expected_revision
                || Some(&identity) != receipt.identity.as_ref()
            {
                return Err(RollbackReason::ExternalChange);
            }
        }
        _ => return Err(RollbackReason::ExternalChange),
    }
    // Cooperative writers retain their locks. A noncooperating last-syscall race is not a CAS guarantee.
    match receipt.prior {
        Some(bytes) => {
            let target = root.join(receipt.path);
            let mut temporary = NamedTempFile::new_in(target.parent().unwrap())
                .map_err(|_| RollbackReason::RestoreFailed)?;
            temporary
                .write_all(bytes)
                .map_err(|_| RollbackReason::RestoreFailed)?;
            temporary
                .flush()
                .map_err(|_| RollbackReason::RestoreFailed)?;
            let result = if current.is_none() {
                temporary.persist_noclobber(&target)
            } else {
                temporary.persist(&target)
            };
            result.map_err(|_| RollbackReason::RestoreFailed)?;
        }
        None => {
            fs::remove_file(root.join(receipt.path)).map_err(|_| RollbackReason::RestoreFailed)?
        }
    }
    Ok(())
}

fn observe(root: &Path, path: &str) -> RollbackRemainingState {
    if require_safe_target_parent(
        root,
        Path::new(path).parent().unwrap_or(Path::new("")),
        "rollback observation",
    )
    .is_err()
    {
        return RollbackRemainingState::Unknown;
    }
    let target = root.join(path);
    match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            RollbackRemainingState::Absent
        }
        Err(_) => RollbackRemainingState::Unknown,
        Ok(metadata) if metadata.file_type().is_symlink() => RollbackRemainingState::Symlink,
        Ok(metadata) if metadata.is_dir() => RollbackRemainingState::Directory,
        Ok(metadata) if metadata.is_file() => metadata_revision(&target, &metadata)
            .map(|revision| RollbackRemainingState::Regular { revision })
            .unwrap_or(RollbackRemainingState::Unknown),
        Ok(_) => RollbackRemainingState::Other,
    }
}
