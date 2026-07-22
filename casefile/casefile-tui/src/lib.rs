//! Terminal interaction and rendering for a Casefile snapshot.

mod browsing;
mod interaction;
mod loading;
mod markdown;
mod record_detail;
mod review;
#[cfg(test)]
mod test_support;
mod ui;
mod workbench;

pub use interaction::{EditIntent, Interaction};
pub use review::ReviewDecision;

use casefile_store::{DerivedSnapshot, ScanResult, Store, StoreError};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, sync::mpsc, thread};

const PAGE_SIZE: isize = 10;

/// Starts the workbench for an already scanned snapshot.
pub fn run(scan: ScanResult, derived: DerivedSnapshot) -> io::Result<Interaction> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = workbench::App::new(scan, derived);
    let result = app.run(&mut terminal);
    terminal.show_cursor()?;
    result
}

/// Opens the terminal immediately and loads the Casefile snapshot on a background thread.
pub fn run_loading(store: Store) -> io::Result<Interaction> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("casefile-tui-loader".into())
        .spawn(move || {
            let result = (|| Ok::<_, StoreError>((store.scan()?, store.derived_snapshot()?)))()
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        })?;

    let result = match loading::run(&mut terminal, receiver) {
        Ok(loading::Outcome::Ready(scan, derived)) => {
            workbench::App::new(scan, derived).run(&mut terminal)
        }
        Ok(loading::Outcome::Quit) => Ok(Interaction::Quit),
        Ok(loading::Outcome::Failed(message)) => Err(io::Error::other(message)),
        Err(error) => Err(error),
    };
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
