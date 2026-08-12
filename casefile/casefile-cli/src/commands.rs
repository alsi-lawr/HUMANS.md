use crate::{Command, editor::EditorConfig, mcp, tui};
use anyhow::{Context, Result};
use casefile_core::{
    ChangeRequest, Classification, Diagnostic, Kind, RecordSummary, Revision, parse_strategy,
};
use casefile_store::{
    ActivationState, ProgressChangeRequest, ProgressOperation, ProgressPreview, Provider, Store,
    StrategyBindingState, StrategyTransitionRequest, WriterBindingRequest,
    normalize_planning_relative,
};
use serde::Serialize;
use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

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
    if matches!(command, Command::McpCompatibility) {
        mcp::print_compatibility()?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Command::McpPackage { planning_root } = &command {
        mcp::serve_package(planning_root)?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Command::McpStdio {
        planning_root,
        expected_root,
        expected_provider_protocol,
        required_provider_operations,
    } = &command
    {
        mcp::serve(
            planning_root,
            expected_root,
            *expected_provider_protocol,
            required_provider_operations,
        )?;
        return Ok(ExitCode::SUCCESS);
    }
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
    if let Command::ScratchStrategy { matrix, target } = &command {
        scratch_strategy(&root, matrix, target)?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Command::Serve { port, index, write } = &command {
        casefile_server::serve(&root, *port, index.as_deref(), *write)?;
        return Ok(ExitCode::SUCCESS);
    }
    match &command {
        Command::Preview { request }
        | Command::RecordSession { request }
        | Command::ProgressPreview { request }
        | Command::ProgressSession { request }
        | Command::ProgressRepairPreview { request }
        | Command::StrategyTransitionPreview { request }
        | Command::StrategyTransitionSession { request }
        | Command::WriterBindingPreview { request }
        | Command::WriterBindingSession { request } => require_external_payload(&root, request)?,
        Command::ProgressRepairApply { preview } => require_external_payload(&root, preview)?,
        _ => {}
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
            let investigation = investigation
                .map(|value| canonical_investigation(&value))
                .transpose()?;
            if let Some(investigation) = &investigation {
                store.validate_investigation(investigation)?;
            }
            let scan = store.scan()?;
            let diagnostics = investigation.as_ref().map_or_else(
                || scan.diagnostics.clone(),
                |investigation| {
                    let prefix = format!("{investigation}/");
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
            print_json(&Provider::without_cache(store).preview_record(request)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::RecordSession { request } => {
            let request: ChangeRequest = read_json(&request)?;
            let provider = Provider::without_cache(store);
            let preview = provider.preview_record(request)?;
            review_preview(&preview.preview_id, preview.approval_required, &preview)?;
            print_json(&provider.apply_record(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressPreview { request } => {
            let operation: ProgressOperation = read_json(&request)?;
            print_json(&Provider::without_cache(store).preview_progress(operation)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressSession { request } => {
            let operation: ProgressOperation = read_json(&request)?;
            let provider = Provider::without_cache(store);
            let preview = provider.preview_progress(operation)?;
            review_preview(&preview.preview_id, preview.approval_required, &preview)?;
            print_json(&provider.apply_progress(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressBootstrap { investigation } => {
            print_json(&Provider::without_cache(store).bootstrap_progress(investigation)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressRepairPreview { request } => {
            let request: ProgressChangeRequest = read_json(&request)?;
            if request.replacement_source.is_none()
                || !request.entries.is_empty()
                || request.bootstrap
            {
                anyhow::bail!("progress recovery accepts only an explicit replacement source");
            }
            print_json(&store.preview_progress(request)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ProgressRepairApply { preview } => {
            let preview: ProgressPreview = read_json(&preview)?;
            if preview.request.replacement_source.is_none()
                || !preview.request.entries.is_empty()
                || preview.request.bootstrap
            {
                anyhow::bail!("progress recovery preview is not an explicit replacement");
            }
            print_json(&store.apply_progress(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::StrategyTransitionPreview { request } => {
            let request: StrategyTransitionRequest = read_json(&request)?;
            print_json(&Provider::without_cache(store).preview_strategy_transition(request)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::StrategyTransitionSession { request } => {
            let request: StrategyTransitionRequest = read_json(&request)?;
            let provider = Provider::without_cache(store);
            let preview = provider.preview_strategy_transition(request)?;
            review_preview(&preview.preview_id, preview.approval_required, &preview)?;
            print_json(&provider.apply_strategy_transition(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::WriterBindingPreview { request } => {
            let request: WriterBindingRequest = read_json(&request)?;
            print_json(&Provider::without_cache(store).preview_writer_binding(request)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::WriterBindingSession { request } => {
            let request: WriterBindingRequest = read_json(&request)?;
            let provider = Provider::without_cache(store);
            let preview = provider.preview_writer_binding(request)?;
            review_preview(&preview.preview_id, preview.approval_required, &preview)?;
            print_json(&provider.apply_writer_binding(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::DefaultDeliveryBoardPreview { investigation } => {
            print_json(
                &Provider::without_cache(store).preview_default_delivery_board(investigation)?,
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::DefaultDeliveryBoardSession { investigation } => {
            let provider = Provider::without_cache(store);
            let preview = provider.preview_default_delivery_board(investigation)?;
            review_preview(&preview.preview_id, preview.approval_required, &preview)?;
            print_json(&provider.apply_default_delivery_board(preview)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::RequireWriterProgress {
            investigation,
            ticket_id,
        } => {
            let investigation = canonical_investigation(&investigation)?;
            store.require_writer_progress(&investigation, &ticket_id)?;
            print_json(&serde_json::json!({
                "investigation": investigation,
                "ticket_id": ticket_id,
                "status": "in_progress",
                "writer_spawn_permitted": true,
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
        Command::ScratchStrategy { .. } => {
            unreachable!("scratch strategy handled before opening the store")
        }
        Command::McpCompatibility | Command::McpPackage { .. } | Command::McpStdio { .. } => {
            unreachable!("MCP commands handled before opening the store")
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

fn scratch_strategy(planning_root: &Path, matrix: &Path, target: &Path) -> Result<()> {
    if !target.is_absolute() {
        anyhow::bail!("scratch strategy target must be an explicit absolute path");
    }
    if target
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        anyhow::bail!("scratch strategy target must be lexically normalized");
    }
    let planning_root = fs::canonicalize(planning_root).context("canonicalize planning root")?;
    let matrix = fs::canonicalize(matrix).context("canonicalize scratch matrix")?;
    if !matrix.is_file() {
        anyhow::bail!("scratch strategy matrix must be a regular file");
    }
    let requested_parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("scratch strategy target must have a parent"))?;
    let target_parent = canonical_future_directory(requested_parent)?;
    let target = target_parent.join(
        target
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("scratch strategy target must name a file"))?,
    );
    if matrix.starts_with(&planning_root)
        || planning_root.starts_with(&matrix)
        || target.starts_with(&planning_root)
        || planning_root.starts_with(&target)
    {
        anyhow::bail!(
            "scratch strategy inputs and target must not overlap the configured planning Store"
        );
    }
    fs::create_dir_all(&target_parent).context("create scratch target parent")?;
    if target
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        anyhow::bail!("scratch strategy target must not be a symlink");
    }
    let source = fs::read_to_string(&matrix).context("read scratch matrix")?;
    casefile_core::validate_strategy_matrix(&source).map_err(|diagnostics| {
        anyhow::anyhow!(
            diagnostics
                .into_iter()
                .map(|item| item.message)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    let temporary =
        tempfile::NamedTempFile::new_in(&target_parent).context("create scratch temporary")?;
    fs::write(temporary.path(), source.as_bytes()).context("write scratch temporary")?;
    temporary
        .persist(&target)
        .map_err(|error| anyhow::anyhow!(error.error))?;
    print_json(&serde_json::json!({
        "operation": "local_scratch_strategy",
        "target": target,
        "matrix": matrix,
        "governed": false,
        "provider_visible": false,
    }))?;
    Ok(())
}

fn canonical_future_directory(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(
            existing
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("scratch target has no existing ancestor"))?
                .to_owned(),
        );
        existing = existing
            .parent()
            .ok_or_else(|| anyhow::anyhow!("scratch target has no existing ancestor"))?;
    }
    let mut resolved =
        fs::canonicalize(existing).context("canonicalize scratch target ancestor")?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn require_external_payload(root: &Path, payload: &Path) -> Result<()> {
    let root = fs::canonicalize(root).context("canonicalize planning root")?;
    let payload = fs::canonicalize(payload).context("canonicalize provider payload")?;
    if payload.starts_with(root) {
        anyhow::bail!("provider request and preview files must remain outside the planning Store");
    }
    Ok(())
}

fn review_preview(id: &str, approval_required: bool, preview: &impl Serialize) -> Result<()> {
    print_json(preview)?;
    if !approval_required {
        return Ok(());
    }
    eprintln!("Type the exact provider preview ID to approve this immutable preview:");
    io::stderr().flush()?;
    let mut approval = String::new();
    io::stdin().read_line(&mut approval)?;
    if approval.trim_end() != id {
        anyhow::bail!(
            "provider preview was not explicitly approved; no planning mutation occurred"
        );
    }
    Ok(())
}

fn strategy_path(investigation: &str) -> Result<String> {
    let investigation = canonical_investigation(investigation)?;
    Ok(format!("{investigation}/strategy/implementation.toml"))
}

fn canonical_investigation(investigation: &str) -> Result<String> {
    normalize_planning_relative(investigation)
        .map_err(|message| anyhow::anyhow!("investigation path {message}"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
