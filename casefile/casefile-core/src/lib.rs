//! Pure v1 Casefile records, diagnostics, and whole-record renderers.

mod board;
mod change;
mod decision;
mod diagnostic;
mod markdown;
mod metadata;
mod project_map;
mod record;
mod rendering;
mod request;
mod snapshot;
mod strategy;
mod work_item;

pub use board::{BoardColumn, BoardDraft};
pub use change::{ApplyResult, ChangeRequest, Preview};
#[doc(hidden)]
pub use decision::parse as parse_decision;
pub use diagnostic::{Diagnostic, SCHEMA_VERSION, stable};
pub use markdown::{markdown_headings, validate_markdown};
#[doc(hidden)]
pub use metadata::arrays as parse_metadata_arrays;
#[doc(hidden)]
pub use project_map::parse as parse_project_map;
pub use record::{Classification, Kind, RecordDraft, RecordSummary};
pub use rendering::render_markdown_html;
#[doc(hidden)]
pub use request::parse as parse_request;
pub use snapshot::{CasefileSnapshot, EntrySnapshot, Revision};
pub use strategy::{
    BindingResolution, StrategyBinding, StrategyCoordination, StrategyLimits, StrategyPipeline,
    StrategyProjection, StrategyRequirements, StrategyWorker,
};
#[doc(hidden)]
pub use strategy::{
    parse as parse_strategy, parse_binding as parse_strategy_binding,
    parse_projection as parse_strategy_projection, validate_matrix as validate_strategy_matrix,
};
pub use work_item::WorkItemDraft;

pub fn parse_draft(path: &str, kind: Kind, text: &str) -> Result<RecordDraft, Vec<Diagnostic>> {
    if !kind.is_writable() {
        return Err(vec![Diagnostic::new(
            path,
            "read_only_kind",
            "only ticket, epic, and board records are writable",
        )]);
    }
    match kind {
        Kind::Ticket | Kind::Epic => work_item::parse(path, kind, text),
        Kind::Board => board::parse(path, text),
        _ => unreachable!("writable kinds are dispatched above"),
    }
}

#[allow(clippy::result_large_err)]
pub fn render_draft(path: &str, draft: &RecordDraft) -> Result<Vec<u8>, Diagnostic> {
    validate_draft(path, draft)?;
    let rendered = match draft {
        RecordDraft::Ticket(item) | RecordDraft::Epic(item) => work_item::render(item),
        RecordDraft::Board(board) => board::render(board),
    };
    let parsed = parse_draft(path, draft.kind(), &rendered).map_err(|errors| {
        errors.into_iter().next().unwrap_or_else(|| {
            Diagnostic::new(path, "render_invalid", "rendered record did not validate")
        })
    })?;
    if &parsed != draft {
        return Err(Diagnostic::new(
            path,
            "render_round_trip",
            "rendered record did not round-trip",
        ));
    }
    Ok(rendered.into_bytes())
}

#[allow(clippy::result_large_err)]
pub fn validate_draft(path: &str, draft: &RecordDraft) -> Result<(), Diagnostic> {
    match draft {
        RecordDraft::Ticket(item) => work_item::validate(path, Kind::Ticket, item),
        RecordDraft::Epic(item) => work_item::validate(path, Kind::Epic, item),
        RecordDraft::Board(board) => board::validate(path, board),
    }
}
