mod contracts;
mod layout;
#[cfg(test)]
mod tests;

use crate::ui::MUTED;
use casefile_core::{
    Classification, EntrySnapshot, Kind, RecordSummary, Revision, StrategyProjection,
};
use casefile_store::{DerivedRecord, DerivedStrategy};
use ratatui::{style::Style, text::Line};

#[derive(Default)]
pub(crate) struct Cache {
    selected: Option<Selected>,
}

struct Selected {
    path: String,
    revision: Revision,
    matrix: Option<StrategyProjection>,
    width: u16,
    lines: Vec<Line<'static>>,
    title: Option<&'static str>,
}

impl Cache {
    pub(crate) fn title(&self) -> Option<&'static str> {
        self.selected.as_ref().and_then(|selected| selected.title)
    }

    pub(crate) fn lines(
        &mut self,
        entry: &EntrySnapshot,
        derived: Option<&DerivedRecord>,
        width: u16,
    ) -> Vec<Line<'static>> {
        let strategy = derived.and_then(|record| record.strategy.as_ref());
        if let Some(selected) = &self.selected
            && selected.path == entry.path
            && selected.revision == entry.content_revision
            && selected.matrix.as_ref() == strategy.map(|value| &value.matrix)
            && selected.width == width
        {
            return selected.lines.clone();
        }
        let (title, lines) = render(entry, strategy, width);
        self.selected = Some(Selected {
            path: entry.path.clone(),
            revision: entry.content_revision.clone(),
            matrix: strategy.map(|value| value.matrix.clone()),
            width,
            lines: lines.clone(),
            title,
        });
        lines
    }
}

fn render(
    entry: &EntrySnapshot,
    strategy: Option<&DerivedStrategy>,
    width: u16,
) -> (Option<&'static str>, Vec<Line<'static>>) {
    selected_flow(entry, strategy).map_or_else(
        || {
            (
                None,
                vec![Line::from("Flow unavailable").style(Style::default().fg(MUTED))],
            )
        },
        |flow| (Some(flow.title), layout::render(&flow, width)),
    )
}

fn selected_flow(entry: &EntrySnapshot, strategy: Option<&DerivedStrategy>) -> Option<Flow> {
    let Some(RecordSummary::Strategy {
        strategy_id, phase, ..
    }) = &entry.summary
    else {
        return None;
    };
    if entry.classification == Classification::Invalid || entry.kind != Some(Kind::Strategy) {
        return None;
    }
    contracts::build(strategy_id, phase, strategy?)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    Investigate,
    Detectives,
    Inspectors,
    Assess,
    Tickets,
    Implement,
    Review,
    Verify,
    Chair,
    Reconcile,
    Done,
}

impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::Investigate => "Investigate",
            Self::Detectives => "Detectives",
            Self::Inspectors => "Inspectors",
            Self::Assess => "Assess",
            Self::Tickets => "Tickets",
            Self::Implement => "Implement",
            Self::Review => "Review",
            Self::Verify => "Verify",
            Self::Chair => "Chair",
            Self::Reconcile => "Reconcile",
            Self::Done => "Done",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Treatment {
    Linear,
    Review,
    Correction,
    Pipeline,
    Dialogue,
}

#[derive(Debug, Eq, PartialEq)]
struct Flow {
    title: &'static str,
    main: Vec<Stage>,
    treatment: Treatment,
    preflight: bool,
}
