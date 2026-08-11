//! Terminal interaction and rendering for a Casefile snapshot.

mod browsing;
mod interaction;
mod markdown;
mod progressive;
mod record_detail;
mod review;
#[cfg(test)]
mod test_support;
mod ui;
mod watching;
mod workbench;

pub use interaction::{EditIntent, Interaction};
pub use progressive::{ObservationHandoff, RefreshMinimumScope, RefreshObservation, RefreshReport};
pub use review::ReviewDecision;
pub use workbench::WorkbenchResume;

use casefile_store::{DerivedSnapshot, ScanResult, Store};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

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
    Ok(run_loading_resuming(store, None)?.interaction)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchOutcome {
    pub interaction: Interaction,
    pub resume: WorkbenchResume,
}

/// Opens the progressive workbench and restores session-local interaction state when supplied.
pub fn run_loading_resuming(
    store: Store,
    resume: Option<WorkbenchResume>,
) -> io::Result<WorkbenchOutcome> {
    let (mut watcher, handoff, initial_observation) =
        watching::WatchCoordinator::start(store.observation_root());
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut coordinator = progressive::Coordinator::start_at(
        store.presentation_session(),
        Some(handoff),
        initial_observation,
    )
    .map_err(|error| io::Error::other(error.to_string()))?;
    let mut app = workbench::App::from_projection(coordinator.projection(), resume);
    app.set_status(coordinator.status());
    let result = app
        .run_progressive_watched(&mut terminal, &mut coordinator, &mut watcher)
        .map(|(interaction, resume)| WorkbenchOutcome {
            interaction,
            resume,
        });
    terminal.show_cursor()?;
    result
}

/// Opens the progressive workbench with the typed observation/report handoff used by watchers.
pub fn run_loading_with_observations(
    store: Store,
    resume: Option<WorkbenchResume>,
    handoff: Option<ObservationHandoff>,
) -> io::Result<WorkbenchOutcome> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut coordinator = progressive::Coordinator::start(store.presentation_session(), handoff)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut app = workbench::App::from_projection(coordinator.projection(), resume);
    app.set_status(coordinator.status());
    let result =
        app.run_progressive(&mut terminal, &mut coordinator)
            .map(|(interaction, resume)| WorkbenchOutcome {
                interaction,
                resume,
            });
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
