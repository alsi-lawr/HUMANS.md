use super::*;
use crate::record_detail::layout::Layout;

#[derive(Default)]
pub(super) struct Cache {
    key: Option<(u64, Option<(String, String)>)>,
    lines: Vec<Line<'static>>,
    layout: Option<Layout>,
    selected_lines: BTreeMap<String, Vec<usize>>,
    pub(super) paths: Vec<String>,
    pub(super) count: usize,
}

impl Cache {
    pub(super) fn prepare(
        &mut self,
        generation: u64,
        scope: Option<(&str, &str)>,
        scan: &ScanResult,
        revision: &casefile_core::Revision,
        boards: &BTreeMap<casefile_store::ScopedIdentity, casefile_store::DerivedBoard>,
        facts: &facts::Facts,
    ) {
        if self.key.as_ref().is_some_and(|(old, selected)| {
            *old == generation && selected.as_ref().map(|(p, i)| (p.as_str(), i.as_str())) == scope
        }) {
            return;
        }
        self.key = Some((generation, scope.map(|(p, i)| (p.into(), i.into()))));
        self.lines.clear();
        self.paths.clear();
        self.selected_lines.clear();
        self.layout = None;
        self.count = 0;
        if *revision != scan.snapshot.revision {
            self.lines.push(
                Line::from("Boards are out of date. Press r to refresh.")
                    .style(Style::default().fg(WARN)),
            );
            return;
        }
        let Some((project, investigation)) = scope else {
            self.lines.push(
                Line::from("Select an investigation to inspect its boards.")
                    .style(Style::default().fg(MUTED)),
            );
            return;
        };
        self.count = boards
            .values()
            .filter(|board| {
                board.identity.scope.project == project
                    && board.identity.scope.investigation.as_deref() == Some(investigation)
            })
            .count();
        let diagnostics = facts.board_diagnostics(project, investigation);
        if !diagnostics.is_empty() {
            // Diagnostics affect display, not navigation to already-resolved cards.
            self.paths.extend(
                boards
                    .values()
                    .filter(|board| {
                        board.identity.scope.project == project
                            && board.identity.scope.investigation.as_deref() == Some(investigation)
                    })
                    .flat_map(|board| &board.columns)
                    .flat_map(|column| &column.cards)
                    .filter_map(|card| match facts.card_path(card) {
                        CardPathResolution::Resolved(path) => Some(path),
                        CardPathResolution::Missing | CardPathResolution::Ambiguous => None,
                    }),
            );
            self.lines.push(
                Line::from("Board definitions or the progress log are invalid.")
                    .style(Style::default().fg(WARN).bold()),
            );
            self.lines.extend(diagnostics.iter().map(|d| {
                Line::from(format!(
                    "{}: {}",
                    safe_inline(&d.code),
                    safe_inline(&d.message)
                ))
                .style(Style::default().fg(WARN))
            }));
            return;
        }
        for board in boards.values().filter(|board| {
            board.identity.scope.project == project
                && board.identity.scope.investigation.as_deref() == Some(investigation)
        }) {
            if !self.lines.is_empty() {
                self.lines.push(Line::from(""));
            }
            self.lines.push(
                Line::from(format!(
                    "{}  [{:?}]",
                    safe_inline(&board.title),
                    board.status_source
                ))
                .style(Style::default().fg(ACCENT).bold()),
            );
            for column in &board.columns {
                self.lines.push(
                    Line::from(format!(
                        "  {} ({})",
                        safe_inline(&column.name),
                        column.cards.len()
                    ))
                    .style(Style::default().fg(ACCENT)),
                );
                if column.cards.is_empty() {
                    self.lines
                        .push(Line::from("    No cards.").style(Style::default().fg(MUTED)));
                }
                for card in &column.cards {
                    let resolution = facts.card_path(card);
                    let (marker, suffix) = match &resolution {
                        CardPathResolution::Resolved(path) => {
                            self.paths.push(path.clone());
                            self.selected_lines
                                .entry(path.clone())
                                .or_default()
                                .push(self.lines.len());
                            (" ", "")
                        }
                        CardPathResolution::Missing => {
                            ("!", "  [detail unavailable: missing identity]")
                        }
                        CardPathResolution::Ambiguous => {
                            ("!", "  [detail unavailable: ambiguous identity]")
                        }
                    };
                    self.lines.push(
                        Line::from(format!(
                            "  {marker} {}  {}  {}{}",
                            safe_inline(&card.identity.identity),
                            safe_inline(&card.status),
                            safe_inline(&card.title),
                            suffix
                        ))
                        .style(
                            if matches!(resolution, CardPathResolution::Resolved(_)) {
                                Style::default()
                            } else {
                                Style::default().fg(WARN)
                            },
                        ),
                    );
                }
            }
        }
        if self.count == 0 {
            self.lines.push(
                Line::from("This investigation has no board definitions.")
                    .style(Style::default().fg(MUTED)),
            );
        }
    }

    pub(super) fn render(
        &mut self,
        selected: Option<&str>,
        focused: bool,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let block = crate::ui::panel(" Boards ", focused);
        let inner = block.inner(area);
        block.render(area, buffer);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        if self
            .layout
            .as_ref()
            .is_none_or(|layout| layout.width() != inner.width)
        {
            self.layout = Some(Layout::new(&self.lines, inner.width));
        }
        let layout = self.layout.as_ref().expect("board layout");
        let selected = selected.and_then(|path| self.selected_lines.get(path));
        let last = selected
            .and_then(|lines| lines.last())
            .map(|index| layout.line_range(*index));
        let scroll = last
            .as_ref()
            .map_or(0, |range| {
                if range.len() >= usize::from(inner.height) {
                    range.start
                } else {
                    range.end.saturating_sub(usize::from(inner.height))
                }
            })
            .min(usize::from(u16::MAX)) as u16;
        layout.render(scroll, inner, buffer);
        for index in selected.into_iter().flatten() {
            let range = layout.line_range(*index);
            for y in range.start.max(usize::from(scroll))
                ..range
                    .end
                    .min(usize::from(scroll) + usize::from(inner.height))
            {
                for x in 0..inner.width {
                    buffer[(inner.x + x, inner.y + (y - usize::from(scroll)) as u16)]
                        .set_style(Style::default().fg(ACCENT).bold());
                }
            }
            if range.start >= usize::from(scroll)
                && range.start < usize::from(scroll) + usize::from(inner.height)
                && inner.width > 2
            {
                buffer[(
                    inner.x + 2,
                    inner.y + (range.start - usize::from(scroll)) as u16,
                )]
                    .set_symbol(">");
            }
        }
    }
}
