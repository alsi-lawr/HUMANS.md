mod commands;
mod edit;
mod editor;
mod json_output;
mod mcp;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "casefile",
    version,
    about = "Casefile scoped diagnostics and governed writer. JSON responses are limited to 8 MiB; oversized responses fail before stdout."
)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Metadata counts only; never reads or emits file bodies. JSON limit: 8 MiB, no partial stdout. Do not use scan or whole-document parsing to diagnose live Stores: use snapshot, scoped record_index, exact record_detail and diagnostics.
    Scan,
    /// Validate one exact investigation plus project support summaries, or all scopes incrementally. Opaque bodies are not opened. JSON limit: 8 MiB, no partial stdout. Never use raw/full scans or whole-document parsing on live Stores; recover through snapshot, scoped record_index, exact record_detail and diagnostics.
    Check {
        #[arg(long)]
        require_activation: bool,
        #[arg(long)]
        investigation: Option<String>,
    },
    /// Diagnostics for an exact governed project/investigation identity (not a path). Returns at most 128 diagnostics, 1024-byte messages and 256 KiB of fields; compare total_count. JSON limit: 8 MiB.
    Diagnostics {
        #[arg(long)]
        project: String,
        #[arg(long)]
        investigation: String,
    },
    /// Validate a complete candidate strategy matrix through the canonical Rust parser.
    ValidateMatrix {
        #[arg(long)]
        matrix: PathBuf,
    },
    /// Preview a governed strategy transition request.
    StrategyTransitionPreview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview, display, explicitly confirm, and apply a governed strategy in one provider session.
    StrategyTransitionSession {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview a progress-gated writer-binding request.
    WriterBindingPreview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview, display, explicitly confirm, and apply a writer binding in one provider session.
    WriterBindingSession {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview the canonical default delivery board through the provider.
    DefaultDeliveryBoardPreview {
        #[arg(long)]
        investigation: String,
    },
    /// Preview, display, explicitly confirm, and apply the default board in one provider session.
    DefaultDeliveryBoardSession {
        #[arg(long)]
        investigation: String,
    },
    /// Require explicit canonical in_progress state immediately before writer spawn.
    RequireWriterProgress {
        #[arg(long)]
        investigation: String,
        #[arg(long)]
        ticket_id: String,
    },
    /// Project the selected implementation writer through the canonical Store-derived state.
    ProjectWriterBinding {
        #[arg(long)]
        investigation: String,
        #[arg(long)]
        strategy_id: String,
    },
    /// Print the adapter/provider compatibility contract for explicit launcher verification.
    McpCompatibility,
    /// Serve the packaged MCP contract with one explicit planning Store root.
    McpPackage {
        /// One explicit, absolute planning Store root. No default or environment fallback exists.
        #[arg(long)]
        planning_root: PathBuf,
    },
    /// Serve the canonical provider as a fixed-root local stdio MCP server.
    McpStdio {
        /// One explicit, absolute planning Store root. No default or environment fallback exists.
        #[arg(long)]
        planning_root: PathBuf,
        /// Canonical root identity supplied by the launcher for conflict detection.
        #[arg(long)]
        expected_root: PathBuf,
        /// Provider protocol version required by the launcher.
        #[arg(long)]
        expected_provider_protocol: u32,
        /// Comma-separated provider operations required by the launcher.
        #[arg(long)]
        required_provider_operations: String,
    },
    /// Preview a typed record change through the canonical provider.
    Preview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview, display, explicitly confirm, and apply a record in one provider session.
    RecordSession {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview a canonical progress operation through the provider.
    ProgressPreview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview, display, explicitly confirm, and apply progress in one provider session.
    ProgressSession {
        #[arg(long)]
        request: PathBuf,
    },
    /// Preview an accepted-ticket unknown bootstrap through the provider.
    ProgressBootstrap {
        #[arg(long)]
        investigation: String,
    },
    /// Preview an explicit malformed-progress replacement through the canonical recovery adapter.
    ProgressRepairPreview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Apply an exact canonical malformed-progress recovery preview.
    ProgressRepairApply {
        #[arg(long)]
        preview: PathBuf,
    },
    /// Copy one canonically validated strategy matrix to an explicit local scratch target.
    ScratchStrategy {
        #[arg(long)]
        matrix: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    /// Serve the fixed planning root on an IPv4 loopback socket.
    Serve {
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        write: bool,
    },
    /// Open the interactive workbench.
    Tui {
        /// Run this editor program and wait for it to exit instead of using the OS file opener.
        #[arg(long, value_name = "PROGRAM")]
        editor: Option<PathBuf>,
        /// Add one argument to --editor; repeat this option to preserve argument boundaries.
        #[arg(long, value_name = "ARG", requires = "editor")]
        editor_arg: Vec<OsString>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(status) => status,
        Err(error) => report_error(&error, &mut std::io::stderr().lock()),
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    commands::execute(cli.root, cli.command)
}

fn rollback_details(error: &anyhow::Error) -> Option<&casefile_store::IncompleteRollback> {
    for error in error.chain() {
        let store = error
            .downcast_ref::<casefile_store::StoreError>()
            .or_else(
                || match error.downcast_ref::<casefile_store::ProviderError>() {
                    Some(casefile_store::ProviderError::Store(store)) => Some(store),
                    _ => None,
                },
            );
        if let Some(casefile_store::StoreError::IncompleteRollback { details, .. }) = store {
            return Some(details);
        }
    }
    None
}

fn report_error(error: &anyhow::Error, output: &mut impl std::io::Write) -> ExitCode {
    if let Some(details) = rollback_details(error) {
        writeln!(
            output,
            "{}",
            serde_json::to_string(details).expect("rollback details serialize")
        )
        .expect("write failure channel");
    } else {
        writeln!(output, "{error:#}").expect("write failure channel");
    }
    ExitCode::FAILURE
}

#[cfg(test)]
mod error_tests {
    #[test]
    fn rollback_failure_channel_retains_details_and_excludes_private_source_text() {
        use casefile_store::*;
        let details = IncompleteRollback {
            code: RollbackErrorCode::IncompleteRollback,
            operation: "progress verification".into(),
            cause: RollbackCause::Io,
            affected_paths: vec![RollbackPathState {
                path: "progress/log.toml".into(),
                remaining: RollbackRemainingState::Unknown,
                reason: RollbackReason::ObservationFailed,
            }],
        };
        let error = anyhow::Error::from(ProviderError::Store(StoreError::IncompleteRollback {
            details: details.clone(),
            cause: Box::new(StoreError::Invalid("PRIVATE FILE EXCERPT".into())),
        }));
        let mut output = Vec::new();
        assert_eq!(
            super::report_error(&error, &mut output),
            std::process::ExitCode::FAILURE
        );
        assert_eq!(
            serde_json::from_slice::<IncompleteRollback>(&output).unwrap(),
            details
        );
        assert!(
            !String::from_utf8(output)
                .unwrap()
                .contains("PRIVATE FILE EXCERPT")
        );
    }
}
