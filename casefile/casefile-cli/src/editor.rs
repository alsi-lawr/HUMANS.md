use anyhow::{Context, Result};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct EditorConfig {
    pub(super) program: Option<PathBuf>,
    pub(super) arguments: Vec<OsString>,
}

pub(super) fn open_draft(path: &Path, editor: &EditorConfig) -> Result<()> {
    if let Some(program) = &editor.program {
        let status = Command::new(program)
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

fn default_opener() -> Command {
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
    }
    #[cfg(target_os = "windows")]
    {
        // Explorer invokes the Windows shell association without routing through cmd.exe.
        Command::new("explorer.exe")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Command::new("xdg-open")
    }
}
