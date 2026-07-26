use crate::{Command, editor::EditorConfig, tui};
use anyhow::{Context, Result};
use casefile_core::{
    ChangeRequest, Classification, Diagnostic, Kind, Preview, RecordSummary, Revision,
    parse_strategy,
};
use casefile_store::{
    ActivationState, ProgressChangeRequest, ProgressPreview, Store, StrategyBindingState,
};
use serde::Serialize;
use std::{fs, path::PathBuf, process::ExitCode};

#[derive(Serialize)]
struct CheckResult {
    activation: ActivationState,
    valid: Option<bool>,
    revision: Revision,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize)]
struct WriterBindingProjection {
    strategy_id: String,
    adapter: String,
    binding: StrategyBindingState,
}

pub(super) fn execute(root: PathBuf, command: Command) -> Result<ExitCode> {
    if let Command::ValidateMatrix { matrix } = &command {
        let source = fs::read_to_string(matrix)?;
        casefile_core::validate_strategy_matrix(&source).map_err(|diagnostics| {
            anyhow::anyhow!(
                diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })?;
        return Ok(ExitCode::SUCCESS);
    }
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
        Command::Check {
            require_activation,
            investigation,
        } => {
            if let Some(investigation) = &investigation {
                store.validate_investigation(investigation)?;
            }
            let scan = store.scan()?;
            let diagnostics = investigation.as_ref().map_or_else(
                || scan.diagnostics.clone(),
                |investigation| {
                    let prefix = format!("{}/", investigation.trim_end_matches('/'));
                    scan.diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.path.starts_with(&prefix))
                        .cloned()
                        .collect()
                },
            );
            let valid = match scan.activation {
                ActivationState::Unactivated => None,
                ActivationState::Active => Some(diagnostics.is_empty()),
                ActivationState::Invalid => Some(false),
            };
            print_json(&CheckResult {
                activation: scan.activation,
                valid,
                revision: scan.snapshot.revision,
                diagnostics,
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
        Command::ProgressPreview { request } => {
            let request: ProgressChangeRequest = read_json(&request)?;
            print_json(&store.preview_progress(request)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressApply { preview } => {
            let preview: ProgressPreview = read_json(&preview)?;
            print_json(&store.apply_progress(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressBootstrap { investigation } => {
            print_json(&store.bootstrap_progress(&investigation)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ReplaceStrategyBinding {
            investigation,
            source,
            implementation_active,
        } => {
            let source = fs::read_to_string(&source)
                .with_context(|| format!("read {}", source.display()))?;
            store.replace_strategy_binding(&investigation, &source, implementation_active)?;
            print_json(&serde_json::json!({
                "path": format!("{}/strategy/bindings.toml", investigation.trim_end_matches('/')),
                "replaced": true,
            }))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProjectWriterBinding {
            investigation,
            strategy_id,
        } => {
            let implementation_path = strategy_path(&investigation)?;
            let derived = store.derived_snapshot()?;
            let record = derived
                .records
                .iter()
                .find(|record| record.path == implementation_path)
                .ok_or_else(|| anyhow::anyhow!("selected implementation strategy is missing"))?;
            if record.classification != Classification::Governed
                || record.kind != Some(Kind::Strategy)
            {
                anyhow::bail!("selected implementation strategy is invalid or ungraphable");
            }
            let content = record.content.as_deref().ok_or_else(|| {
                anyhow::anyhow!("selected implementation strategy is invalid or ungraphable")
            })?;
            let summary = parse_strategy(&implementation_path, content).map_err(|_| {
                anyhow::anyhow!("selected implementation strategy is invalid or ungraphable")
            })?;
            let RecordSummary::Strategy {
                strategy_id: selected_id,
                phase,
                adapter,
            } = summary
            else {
                anyhow::bail!("selected implementation strategy is invalid or ungraphable");
            };
            if phase != "implementation" || selected_id != strategy_id || adapter != "codex" {
                anyhow::bail!("requested Codex implementation strategy is not selected");
            }
            let strategy = record.strategy.as_ref().ok_or_else(|| {
                anyhow::anyhow!("selected implementation strategy is invalid or ungraphable")
            })?;
            let binding = strategy.binding.clone().ok_or_else(|| {
                anyhow::anyhow!("selected implementation strategy has no writer binding state")
            })?;
            print_json(&WriterBindingProjection {
                strategy_id,
                adapter,
                binding,
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Serve { .. } => unreachable!("serve handled before opening the store"),
        Command::ValidateMatrix { .. } => {
            unreachable!("validation handled before opening the store")
        }
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

fn strategy_path(investigation: &str) -> Result<String> {
    let investigation = investigation.trim_end_matches('/');
    if investigation.is_empty()
        || investigation.starts_with('/')
        || investigation.contains('\\')
        || investigation
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        anyhow::bail!("investigation path must be a contained relative path");
    }
    Ok(format!("{investigation}/strategy/implementation.toml"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
