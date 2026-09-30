#![allow(dead_code, unused_imports)]

mod browsing;
mod interaction;
mod markdown;
mod progressive;
mod record_detail;
mod strategy_flow;
#[cfg(test)]
mod test_support;
mod ui;
mod watching;
mod workbench;

pub use interaction::{EditIntent, Interaction};
pub use progressive::{ObservationHandoff, RefreshMinimumScope, RefreshObservation, RefreshReport};

// Paging is unmeasured; this compile shim is not a shared production configuration.
const PAGE_SIZE: isize = 10;

#[path = "../benches/interactions.rs"]
mod benchmarks;

fn main() {
    benchmarks::run();
}
