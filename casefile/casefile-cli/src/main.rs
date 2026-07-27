mod commands;
mod edit;
mod editor;
mod mcp;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "casefile",
    about = "Compact Casefile v1 scanner and governed writer"
)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Scan,
    Check {
        #[arg(long)]
        require_activation: bool,
        #[arg(long)]
        investigation: Option<String>,
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
    /// Apply an immutable governed strategy-transition preview.
    StrategyTransitionApply {
        #[arg(long)]
        preview: PathBuf,
    },
    /// Preview a progress-gated writer-binding request.
    WriterBindingPreview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Apply an immutable writer-binding preview.
    WriterBindingApply {
        #[arg(long)]
        preview: PathBuf,
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
    Preview {
        #[arg(long)]
        request: PathBuf,
    },
    Apply {
        #[arg(long)]
        preview: PathBuf,
    },
    /// Internal canonical progress preview; workflow callers use transition-ticket-progress.py.
    ProgressPreview {
        #[arg(long)]
        request: PathBuf,
    },
    /// Apply an immutable progress preview produced by progress-preview.
    ProgressApply {
        #[arg(long)]
        preview: PathBuf,
    },
    /// Materialize an accepted-ticket unknown bootstrap request for the workflow script.
    ProgressBootstrap {
        #[arg(long)]
        investigation: String,
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
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    commands::execute(cli.root, cli.command)
}
