mod commands;
mod edit;
mod editor;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "casefile",
    about = "Compact Casefile v1 scanner and one-path writer"
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
    /// Persist a validated writer binding through the Store transaction boundary.
    ReplaceStrategyBinding {
        #[arg(long)]
        investigation: String,
        #[arg(long)]
        source: PathBuf,
        #[arg(
            long,
            required = true,
            action = clap::ArgAction::Set,
            value_parser = clap::value_parser!(bool)
        )]
        implementation_active: bool,
    },
    /// Project the selected implementation writer through the canonical Store-derived state.
    ProjectWriterBinding {
        #[arg(long)]
        investigation: String,
        #[arg(long)]
        strategy_id: String,
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
