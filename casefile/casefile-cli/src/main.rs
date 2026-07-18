use anyhow::{Context, Result};
use casefile_core::{ChangeRequest, Diagnostic, Preview, Revision};
use casefile_store::{ActivationState, Store};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::{fs, path::PathBuf, process::ExitCode};

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
    },
    Preview {
        #[arg(long)]
        request: PathBuf,
    },
    Apply {
        #[arg(long)]
        preview: PathBuf,
    },
    Tui,
}

#[derive(Serialize)]
struct CheckResult {
    activation: ActivationState,
    valid: Option<bool>,
    revision: Revision,
    diagnostics: Vec<Diagnostic>,
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
    let store = Store::open(cli.root)?;
    match cli.command {
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
        Command::Tui => {
            casefile_tui::run(store.scan()?)?;
            Ok(ExitCode::SUCCESS)
        }
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
