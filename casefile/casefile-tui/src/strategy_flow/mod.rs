mod contracts;
mod layout;
#[cfg(test)]
mod tests;

use crate::ui::{ACCENT, MUTED, safe_inline};
use casefile_core::{Classification, EntrySnapshot, Kind, RecordSummary, Revision};
use casefile_store::{DerivedRecord, DerivedStrategy};
use ratatui::{style::Style, text::Line};

#[derive(Default)]
pub(crate) struct Cache {
    selected: Option<Selected>,
}

struct Selected {
    path: String,
    revision: Revision,
    strategy: Option<DerivedStrategy>,
    width: u16,
    lines: Vec<Line<'static>>,
}

impl Cache {
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
            && selected.strategy.as_ref() == strategy
            && selected.width == width
        {
            return selected.lines.clone();
        }
        let lines = render(entry, strategy, width);
        self.selected = Some(Selected {
            path: entry.path.clone(),
            revision: entry.content_revision.clone(),
            strategy: strategy.cloned(),
            width,
            lines: lines.clone(),
        });
        lines
    }
}

fn render(
    entry: &EntrySnapshot,
    strategy: Option<&DerivedStrategy>,
    width: u16,
) -> Vec<Line<'static>> {
    let unavailable = || vec![Line::from("Flow unavailable").style(Style::default().fg(MUTED))];
    let Some(RecordSummary::Strategy {
        strategy_id,
        phase,
        adapter,
    }) = &entry.summary
    else {
        return unavailable();
    };
    if entry.classification == Classification::Invalid || entry.kind != Some(Kind::Strategy) {
        return unavailable();
    }
    let Some(strategy) = strategy else {
        return unavailable();
    };
    let Some(flow) = contracts::build(strategy_id, phase, strategy) else {
        return unavailable();
    };
    let mut lines = layout::wrapped(
        &format!(
            "{} · {} · {}",
            safe_inline(strategy_id),
            safe_inline(phase),
            safe_inline(adapter)
        ),
        width.max(1),
    )
    .into_iter()
    .map(|line| line.style(Style::default().fg(ACCENT).bold()))
    .collect::<Vec<_>>();
    lines.push(Line::default());
    lines.extend(layout::render(&flow, width));
    lines
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    Assign,
    Root,
    Writer,
    LookAhead,
    AdvisoryReceipt,
    Primary,
    Verifier,
    Inspector,
    Detective,
    Chair,
    Challenger,
    Reconcile,
    Correct,
    Accepted,
    Human,
    Independence,
    NextWriter,
}

struct Node {
    stage: Stage,
    label: String,
}

#[derive(Default)]
struct Flow {
    rows: Vec<Vec<Node>>,
    edges: Vec<(Stage, Stage)>,
}

impl Flow {
    fn row(&mut self, nodes: Vec<Node>) {
        self.rows.push(nodes);
    }
    fn edge(&mut self, from: Stage, to: Stage) {
        self.edges.push((from, to));
    }
    fn chain(&mut self, nodes: Vec<Node>) {
        let mut previous = None;
        for node in nodes {
            if let Some(from) = previous {
                self.edge(from, node.stage);
            }
            previous = Some(node.stage);
            self.row(vec![node]);
        }
    }
}

fn node(stage: Stage, label: impl Into<String>) -> Node {
    Node {
        stage,
        label: label.into(),
    }
}
