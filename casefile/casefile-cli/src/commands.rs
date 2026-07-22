use crate::{Command, editor::EditorConfig, tui};
use anyhow::{Context, Result};
use casefile_core::{ChangeRequest, Diagnostic, Preview, Revision};
use casefile_store::{ActivationState, Store};
use serde::Serialize;
use std::{fs, path::PathBuf, process::ExitCode};

#[derive(Serialize)]
struct CheckResult {
    activation: ActivationState,
    valid: Option<bool>,
    revision: Revision,
    diagnostics: Vec<Diagnostic>,
}

pub(super) fn execute(root: PathBuf, command: Command) -> Result<ExitCode> {
    if let Command::Serve { port, index, write } = &command {
        casefile_server::serve(&root, *port, index.as_deref(), *write)?;
        return Ok(ExitCode::SUCCESS);
    }
    let store = Store::open(&root)?;
    match command {
        Command::Scan => {
            print_json(&store.scan()?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Check { require_activation } => {
            let scan = store.scan()?;
            let valid = match scan.activation {
                ActivationState::Unactivated => None,
                ActivationState::Active => Some(scan.diagnostics.is_empty()),
                ActivationState::Invalid => Some(false),
            };
            print_json(&CheckResult {
                activation: scan.activation,
                valid,
                revision: scan.snapshot.revision,
                diagnostics: scan.diagnostics,
            })?;
            Ok(
                if valid == Some(false) || (require_activation && valid.is_none()) {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                },
            )
        }
        Command::Preview { request } => {
            let request: ChangeRequest = read_json(&request)?;
            print_json(&store.preview(request)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Apply { preview } => {
            let preview: Preview = read_json(&preview)?;
            print_json(&store.apply(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Serve { .. } => unreachable!("serve handled before opening the store"),
        Command::Tui { editor, editor_arg } => tui::run(
            &store,
            &root,
            EditorConfig {
                program: editor,
                arguments: editor_arg,
            },
        ),
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
