use crate::{edit, editor::EditorConfig};
use anyhow::Result;
use casefile_store::Store;
use std::{path::Path, process::ExitCode};

pub(super) fn run(store: &Store, root: &Path, editor: EditorConfig) -> Result<ExitCode> {
    loop {
        match casefile_tui::run_loading(store.clone())? {
            casefile_tui::Interaction::Quit => return Ok(ExitCode::SUCCESS),
            casefile_tui::Interaction::Edit(intent) => {
                let Some((preview, draft_path)) =
                    edit::prepare_preview(store, root, &editor, &intent.path, intent.kind)?
                else {
                    continue;
                };
                let decision = casefile_tui::review(&preview.diff)
                    .map_err(|error| edit::retained_draft(error.into(), &draft_path))?;
                match decision {
                    casefile_tui::ReviewDecision::Cancel => edit::cancel(&draft_path)?,
                    casefile_tui::ReviewDecision::Apply => {
                        edit::apply(store, preview, &intent.path, &draft_path)?;
                    }
                }
            }
        }
    }
}
