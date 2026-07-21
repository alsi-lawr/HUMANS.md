use anyhow::{Context, Result};
use casefile_core::{
    ChangeRequest, Classification, Diagnostic, EntrySnapshot, Kind, Preview, Revision, parse_draft,
};
use casefile_store::{ActivationState, Store};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
};
use tempfile::Builder;

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

struct EditorConfig {
    program: Option<PathBuf>,
    arguments: Vec<OsString>,
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
    let root = cli.root;
    let store = Store::open(&root)?;
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
        Command::Tui { editor, editor_arg } => run_tui(
            &store,
            &root,
            EditorConfig {
                program: editor,
                arguments: editor_arg,
            },
        ),
    }
}

fn run_tui(store: &Store, root: &Path, editor: EditorConfig) -> Result<ExitCode> {
    loop {
        match casefile_tui::run(store.scan()?)? {
            casefile_tui::Interaction::Quit => return Ok(ExitCode::SUCCESS),
            casefile_tui::Interaction::Edit(intent) => edit_selected(store, root, &editor, intent)?,
        }
    }
}

fn edit_selected(
    store: &Store,
    root: &Path,
    editor: &EditorConfig,
    intent: casefile_tui::EditIntent,
) -> Result<()> {
    let scan = store.scan()?;
    let entry = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path == intent.path)
        .filter(|entry| editable(entry, intent.kind))
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
        return Ok(());
    }
    let text = match String::from_utf8(draft_bytes) {
        Ok(text) => text,
        Err(error) => return Err(retained_draft(error.into(), &draft_path)),
    };
    let parsed = match parse_draft(&intent.path, intent.kind, &text) {
        Ok(draft) => draft,
        Err(diagnostics) => {
            return Err(retained_draft(
                anyhow::anyhow!(format_diagnostics(&diagnostics)),
                &draft_path,
            ));
        }
    };
    let preview = match store.preview(ChangeRequest::Replace {
        path: intent.path.clone(),
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
    match casefile_tui::review(&preview.diff) {
        Ok(casefile_tui::ReviewDecision::Cancel) => {
            discard_draft(&draft_path)?;
            println!("Cancelled; discarded draft {}", draft_path.display());
            Ok(())
        }
        Ok(casefile_tui::ReviewDecision::Apply) => {
            if let Err(error) = store.apply(preview) {
                return Err(retained_draft(error.into(), &draft_path));
            }
            let scan = store.scan().map_err(|error| {
                anyhow::Error::new(error).context(format!(
                    "canonical change applied; post-apply rescan failed; draft retained at {}",
                    draft_path.display()
                ))
            })?;
            discard_draft(&draft_path).with_context(|| {
                format!(
                    "canonical change applied and rescanned; draft cleanup failed at {}",
                    draft_path.display()
                )
            })?;
            println!(
                "Applied {} and rescanned revision {}.",
                intent.path, scan.snapshot.revision.0
            );
            Ok(())
        }
        Err(error) => Err(retained_draft(error.into(), &draft_path)),
    }
}

fn editable(entry: &EntrySnapshot, kind: Kind) -> bool {
    entry.classification == Classification::Governed
        && entry.kind == Some(kind)
        && matches!(kind, Kind::Ticket | Kind::Epic | Kind::Board)
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

fn open_draft(path: &Path, editor: &EditorConfig) -> Result<()> {
    if let Some(program) = &editor.program {
        let status = ProcessCommand::new(program)
            .args(&editor.arguments)
            .arg(path)
            .status()
            .with_context(|| format!("start editor {}", program.display()))?;
        if status.success() {
            return Ok(());
        }
        anyhow::bail!("editor {} exited with {status}", program.display());
    }

    let mut command = default_opener();
    let status = command
        .arg(path)
        .status()
        .context("open draft with the OS file association")?;
    if !status.success() {
        anyhow::bail!("OS file opener exited with {status}");
    }
    print!(
        "Opened draft {}. Edit it, save it, then press Enter to continue: ",
        path.display()
    );
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(())
}

fn default_opener() -> ProcessCommand {
    #[cfg(target_os = "linux")]
    {
        ProcessCommand::new("xdg-open")
    }
    #[cfg(target_os = "macos")]
    {
        ProcessCommand::new("open")
    }
    #[cfg(target_os = "windows")]
    {
        // Explorer invokes the Windows shell association without routing through cmd.exe.
        ProcessCommand::new("explorer.exe")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        ProcessCommand::new("xdg-open")
    }
}

fn retained_draft(error: anyhow::Error, draft_path: &Path) -> anyhow::Error {
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

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}
fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
