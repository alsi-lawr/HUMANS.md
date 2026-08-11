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

trait BootstrapSource {
    fn scan(&self) -> Result<ScanResult, StoreError>;
    fn derive_snapshot(&self, scan: &ScanResult) -> DerivedSnapshot;
}

impl BootstrapSource for Store {
    fn scan(&self) -> Result<ScanResult, StoreError> {
        Store::scan(self)
    }

    fn derive_snapshot(&self, scan: &ScanResult) -> DerivedSnapshot {
        Store::derive_snapshot(self, scan)
    }
}

fn bootstrap_snapshot(
    source: &impl BootstrapSource,
) -> Result<(ScanResult, DerivedSnapshot), StoreError> {
    let scan = source.scan()?;
    let derived = source.derive_snapshot(&scan);
    Ok((scan, derived))
}

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
            let result = bootstrap_snapshot(&store).map_err(|error| error.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct CountingSource {
        scan_calls: Cell<usize>,
        result: ScanResult,
    }

    impl BootstrapSource for CountingSource {
        fn scan(&self) -> Result<ScanResult, StoreError> {
            self.scan_calls.set(self.scan_calls.get() + 1);
            Ok(self.result.clone())
        }

        fn derive_snapshot(&self, scan: &ScanResult) -> DerivedSnapshot {
            test_support::derived(scan)
        }
    }

    #[test]
    fn bootstrap_scans_once_and_derives_from_that_exact_revision() {
        let source = CountingSource {
            scan_calls: Cell::new(0),
            result: test_support::scan(),
        };

        let (scan, derived) = bootstrap_snapshot(&source).expect("bootstrap");

        assert_eq!(source.scan_calls.get(), 1);
        assert_eq!(derived.source_revision, scan.snapshot.revision);
        assert_eq!(scan, source.result);
    }
}
