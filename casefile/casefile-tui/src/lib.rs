//! Terminal interaction and rendering for an already scanned Casefile snapshot.

mod browsing;
mod interaction;
mod markdown;
mod record_detail;
mod review;
#[cfg(test)]
mod test_support;
mod ui;
mod workbench;

pub use interaction::{EditIntent, Interaction};
pub use review::ReviewDecision;

use casefile_store::ScanResult;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

const PAGE_SIZE: isize = 10;

/// Starts the workbench for an already scanned snapshot.
pub fn run(scan: ScanResult) -> io::Result<Interaction> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = workbench::App::new(scan);
    let result = app.run(&mut terminal);
    terminal.show_cursor()?;
    result
}

/// Shows the Store-provided diff and returns an explicit apply or cancel decision.
pub fn review(diff: &str) -> io::Result<ReviewDecision> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = review::ReviewApp::new(diff);
    let result = app.run(&mut terminal);
    terminal.show_cursor()?;
    result
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
