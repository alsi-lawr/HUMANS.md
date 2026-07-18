use anyhow::{Context, Result};
use casefile_core::{ChangeRequest, Preview};
use casefile_store::Store;
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

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
    Preview {
        #[arg(long)]
        request: PathBuf,
    },
    Apply {
        #[arg(long)]
        preview: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::open(cli.root)?;
    match cli.command {
        Command::Scan => print_json(&store.scan()?),
        Command::Preview { request } => {
            let request: ChangeRequest = read_json(&request)?;
            print_json(&store.preview(request)?)
        }
        Command::Apply { preview } => {
            let preview: Preview = read_json(&preview)?;
            print_json(&store.apply(preview)?)
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
