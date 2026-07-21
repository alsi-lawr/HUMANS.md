use crate::editor::{EditorConfig, open_draft};
use anyhow::{Context, Result};
use casefile_core::{
    ChangeRequest, Classification, Diagnostic, EntrySnapshot, Kind, Preview, parse_draft,
};
use casefile_store::Store;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::Builder;

pub(super) fn prepare_preview(
    store: &Store,
    root: &Path,
    editor: &EditorConfig,
    path: &str,
    kind: Kind,
) -> Result<Option<(Preview, PathBuf)>> {
    let scan = store.scan()?;
    let entry = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .filter(|entry| editable(entry, kind))
        .context("selected record is no longer an editable governed ticket, epic, or board")?;
    let draft_path = create_draft(root, entry)?;

    if let Err(error) = open_draft(&draft_path, editor) {
        return Err(retained_draft(error, &draft_path));
    }

    let draft_bytes = match fs::read(&draft_path) {
        Ok(bytes) => bytes,
        Err(error) => return Err(retained_draft(error.into(), &draft_path)),
    };
    if draft_bytes == entry.original_bytes {
        discard_draft(&draft_path)?;
        println!("No changes; discarded draft {}", draft_path.display());
        return Ok(None);
    }
    let text = match String::from_utf8(draft_bytes) {
        Ok(text) => text,
        Err(error) => return Err(retained_draft(error.into(), &draft_path)),
    };
    let parsed = match parse_draft(path, kind, &text) {
        Ok(draft) => draft,
        Err(diagnostics) => {
            return Err(retained_draft(
                anyhow::anyhow!(format_diagnostics(&diagnostics)),
                &draft_path,
            ));
        }
    };
    let preview = match store.preview(ChangeRequest::Replace {
        path: path.to_owned(),
        draft: parsed,
    }) {
        Ok(preview) if preview.diagnostics.is_empty() => preview,
        Ok(preview) => {
            return Err(retained_draft(
                anyhow::anyhow!(format_diagnostics(&preview.diagnostics)),
                &draft_path,
            ));
        }
        Err(error) => return Err(retained_draft(error.into(), &draft_path)),
    };
    Ok(Some((preview, draft_path)))
}

pub(super) fn cancel(draft_path: &Path) -> Result<()> {
    discard_draft(draft_path)?;
    println!("Cancelled; discarded draft {}", draft_path.display());
    Ok(())
}

pub(super) fn apply(store: &Store, preview: Preview, path: &str, draft_path: &Path) -> Result<()> {
    if let Err(error) = store.apply(preview) {
        return Err(retained_draft(error.into(), draft_path));
    }
    let scan = store.scan().map_err(|error| {
        anyhow::Error::new(error).context(format!(
            "canonical change applied; post-apply rescan failed; draft retained at {}",
            draft_path.display()
        ))
    })?;
    discard_draft(draft_path).with_context(|| {
        format!(
            "canonical change applied and rescanned; draft cleanup failed at {}",
            draft_path.display()
        )
    })?;
    println!(
        "Applied {} and rescanned revision {}.",
        path, scan.snapshot.revision.0
    );
    Ok(())
}

fn editable(entry: &EntrySnapshot, kind: Kind) -> bool {
    entry.classification == Classification::Governed
        && entry.kind == Some(kind)
        && kind.is_writable()
}

fn create_draft(root: &Path, entry: &EntrySnapshot) -> Result<PathBuf> {
    let target = root.join(&entry.path);
    let parent = target
        .parent()
        .context("selected record has no parent directory")?;
    let extension = target
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .context("selected record has no usable extension")?;
    let directory = Builder::new()
        .prefix(".casefile-draft-")
        .tempdir_in(parent)
        .with_context(|| format!("create secure draft directory beside {}", entry.path))?;
    let mut draft = Builder::new()
        .prefix("draft-")
        .suffix(&extension)
        .tempfile_in(directory.path())
        .with_context(|| format!("create secure draft beside {}", entry.path))?;
    draft
        .write_all(&entry.original_bytes)
        .with_context(|| format!("write draft for {}", entry.path))?;
    draft.flush()?;
    let (_, path) = draft.keep().context("retain draft")?;
    let _directory = directory.keep();
    Ok(path)
}

pub(super) fn retained_draft(error: anyhow::Error, draft_path: &Path) -> anyhow::Error {
    error.context(format!(
        "canonical files unchanged; draft retained at {}",
        draft_path.display()
    ))
}

fn discard_draft(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("discard draft {}", path.display()))?;
    let directory = path.parent().context("draft has no parent directory")?;
    fs::remove_dir(directory)
        .with_context(|| format!("discard draft directory {}", directory.display()))
}

fn format_diagnostics(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("; ")
}
