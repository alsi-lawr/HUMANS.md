use crate::ui::{
    ACCENT, BAD, BORDER, GOOD, MUTED, SELECTED, WARN, classification_name, classification_style,
    kind_name, panel, safe_inline, status_style, summary_title, work_status,
};
use casefile_core::{Classification, EntrySnapshot, RecordSummary};
use casefile_store::{ActivationState, ScanResult};
use ratatui::layout::Rect;
use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    WorkQueue,
    Records,
}

impl View {
    fn title(self) -> &'static str {
        match self {
            Self::WorkQueue => "Work queue",
            Self::Records => "Records",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::WorkQueue => Self::Records,
            Self::Records => Self::WorkQueue,
        }
    }
}

pub(crate) struct Browser {
    view: View,
    selected_path: Option<String>,
    filter: String,
    entering_filter: bool,
}

impl Browser {
    pub(crate) fn new(scan: &ScanResult) -> Self {
        let mut browser = Self {
            view: View::WorkQueue,
            selected_path: None,
            filter: String::new(),
            entering_filter: false,
        };
        browser.normalise_selection(scan);
        browser
    }

    pub(crate) fn set_view(&mut self, scan: &ScanResult, view: View) {
        self.view = view;
        self.normalise_selection(scan);
    }

    pub(crate) fn cycle_view(&mut self, scan: &ScanResult) {
        self.set_view(scan, self.view.next());
    }

    pub(crate) fn is_entering_filter(&self) -> bool {
        self.entering_filter
    }

    pub(crate) fn start_filter(&mut self) {
        self.entering_filter = true;
    }

    pub(crate) fn close_filter(&mut self) {
        self.entering_filter = false;
    }

    pub(crate) fn push_filter(&mut self, scan: &ScanResult, character: char) -> bool {
        self.filter.push(character);
        self.normalise_selection(scan)
    }

    pub(crate) fn pop_filter(&mut self, scan: &ScanResult) -> bool {
        self.filter.pop();
        self.normalise_selection(scan)
    }

    pub(crate) fn clear_filter(&mut self, scan: &ScanResult) -> bool {
        self.filter.clear();
        self.normalise_selection(scan)
    }

    pub(crate) fn entries<'a>(&self, scan: &'a ScanResult) -> Vec<&'a EntrySnapshot> {
        scan.snapshot
            .entries
            .iter()
            .filter(|entry| self.matches_view(entry) && self.matches_filter(entry))
            .collect()
    }

    pub(crate) fn work_count(&self, scan: &ScanResult) -> usize {
        scan.snapshot
            .entries
            .iter()
            .filter(|entry| {
                entry.classification == Classification::Governed
                    && matches!(entry.summary, Some(RecordSummary::WorkItem { .. }))
            })
            .count()
    }

    pub(crate) fn selected<'a>(&self, scan: &'a ScanResult) -> Option<&'a EntrySnapshot> {
        let path = self.selected_path.as_deref()?;
        scan.snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
    }

    pub(crate) fn select_offset(&mut self, scan: &ScanResult, offset: isize) -> bool {
        let entries = self.entries(scan);
        if entries.is_empty() {
            let changed = self.selected_path.is_some();
            self.selected_path = None;
            return changed;
        }
        let index = entries
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
            .unwrap_or(0);
        let next = (index as isize + offset).clamp(0, entries.len() as isize - 1) as usize;
        let path = entries[next].path.clone();
        if self.selected_path.as_deref() == Some(path.as_str()) {
            false
        } else {
            self.selected_path = Some(path);
            true
        }
    }

    pub(crate) fn select_edge(&mut self, scan: &ScanResult, end: bool) -> bool {
        let entries = self.entries(scan);
        let next = entries
            .get(if end {
                entries.len().saturating_sub(1)
            } else {
                0
            })
            .map(|entry| entry.path.clone());
        if next == self.selected_path {
            false
        } else {
            self.selected_path = next;
            true
        }
    }

    pub(crate) fn render_header(&self, scan: &ScanResult, area: Rect, buffer: &mut Buffer) {
        let work_style = tab_style(self.view == View::WorkQueue);
        let records_style = tab_style(self.view == View::Records);
        let filter = if self.filter.is_empty() {
            "none".to_owned()
        } else {
            format!("\"{}\"", safe_inline(&self.filter))
        };
        let visible = self.entries(scan).len();
        let total = match self.view {
            View::WorkQueue => self.work_count(scan),
            View::Records => scan.snapshot.entries.len(),
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(" CASEFILE ", Style::default().fg(ACCENT).bold()),
                Span::styled(format!(" [1] WORK {} ", self.work_count(scan)), work_style),
                Span::raw(" "),
                Span::styled(
                    format!(" [2] RECORDS {} ", scan.snapshot.entries.len()),
                    records_style,
                ),
                Span::raw("   "),
                Span::styled(
                    activation_name(scan.activation),
                    activation_style(scan.activation).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  |  {} diagnostics", scan.diagnostics.len()),
                    Style::default().fg(MUTED),
                ),
            ]),
            Line::from(vec![
                Span::styled(" Filter ", Style::default().fg(MUTED)),
                Span::styled(filter, Style::default().fg(Color::White)),
                Span::styled(
                    format!("  |  showing {visible}/{total}"),
                    Style::default().fg(MUTED),
                ),
                if self.entering_filter {
                    Span::styled("  TYPE TO FILTER", Style::default().fg(WARN).bold())
                } else {
                    Span::raw("")
                },
            ]),
        ];
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(BORDER)),
            )
            .render(area, buffer);
    }

    pub(crate) fn render_list(
        &self,
        scan: &ScanResult,
        focused: bool,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let entries = self.entries(scan);
        let selected_index = self.selected_index(scan);
        let position = selected_index
            .map(|index| format!("{} / {}", index + 1, entries.len()))
            .unwrap_or_else(|| format!("0 / {}", entries.len()));
        let block = panel(format!(" {}  {position} ", self.view.title()), focused);
        if entries.is_empty() {
            let message = if self.filter.is_empty() {
                match self.view {
                    View::WorkQueue => "No governed tickets or epics in the work queue.",
                    View::Records => "No records in this Casefile root.",
                }
            } else {
                "No records match the active filter. Press c to clear it."
            };
            Paragraph::new(message)
                .style(Style::default().fg(MUTED))
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, buffer);
            return;
        }
        let items: Vec<_> = entries
            .iter()
            .map(|entry| ListItem::new(list_label(entry, self.view)))
            .collect();
        let mut state = ListState::default();
        state.select(selected_index);
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(SELECTED)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">");
        StatefulWidget::render(list, area, buffer, &mut state);
    }

    fn selected_index(&self, scan: &ScanResult) -> Option<usize> {
        self.entries(scan)
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
    }

    fn matches_view(&self, entry: &EntrySnapshot) -> bool {
        match self.view {
            View::Records => true,
            View::WorkQueue => {
                entry.classification == Classification::Governed
                    && matches!(entry.summary, Some(RecordSummary::WorkItem { .. }))
            }
        }
    }

    fn matches_filter(&self, entry: &EntrySnapshot) -> bool {
        let filter = self.filter.to_lowercase();
        filter.is_empty()
            || [
                entry.path.as_str(),
                classification_name(entry.classification),
                entry.kind.map(kind_name).unwrap_or_default(),
                entry.identity.as_deref().unwrap_or_default(),
                summary_title(entry.summary.as_ref()),
                work_status(entry.summary.as_ref()),
            ]
            .into_iter()
            .any(|field| field.to_lowercase().contains(&filter))
    }

    fn normalise_selection(&mut self, scan: &ScanResult) -> bool {
        let next = {
            let entries = self.entries(scan);
            if entries
                .iter()
                .any(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
            {
                self.selected_path.clone()
            } else {
                entries.first().map(|entry| entry.path.clone())
            }
        };
        if next == self.selected_path {
            false
        } else {
            self.selected_path = next;
            true
        }
    }
}

fn list_label(entry: &EntrySnapshot, view: View) -> Line<'static> {
    match (view, entry.summary.as_ref()) {
        (
            View::WorkQueue,
            Some(RecordSummary::WorkItem {
                id,
                title,
                status,
                rank,
            }),
        ) => Line::from(vec![
            Span::styled(
                format!(" {:^10} ", safe_inline(status).to_uppercase()),
                status_style(status),
            ),
            Span::styled(
                format!(" {} ", safe_inline(id)),
                Style::default().fg(ACCENT),
            ),
            Span::raw(safe_inline(title)),
            Span::styled(
                rank.map(|rank| format!("  #{rank}")).unwrap_or_default(),
                Style::default().fg(MUTED),
            ),
        ]),
        _ => Line::from(vec![
            Span::styled(
                format!(
                    " {:^10} ",
                    classification_name(entry.classification).to_uppercase()
                ),
                classification_style(entry.classification),
            ),
            Span::styled(
                format!(" {} ", entry.kind.map(kind_name).unwrap_or("unknown")),
                Style::default().fg(MUTED),
            ),
            Span::styled(
                safe_inline(entry.identity.as_deref().unwrap_or(&entry.path)),
                Style::default().fg(Color::White),
            ),
            if summary_title(entry.summary.as_ref()).is_empty() {
                Span::raw("")
            } else {
                Span::styled(
                    format!("  {}", safe_inline(summary_title(entry.summary.as_ref()))),
                    Style::default().fg(MUTED),
                )
            },
        ]),
    }
}

fn tab_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED)
    }
}

fn activation_name(activation: ActivationState) -> &'static str {
    match activation {
        ActivationState::Active => "ACTIVE",
        ActivationState::Unactivated => "UNACTIVATED",
        ActivationState::Invalid => "INVALID ACTIVATION",
    }
}

fn activation_style(activation: ActivationState) -> Style {
    Style::default().fg(match activation {
        ActivationState::Active => GOOD,
        ActivationState::Unactivated => WARN,
        ActivationState::Invalid => BAD,
    })
}
