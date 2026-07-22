use crate::{
    markdown,
    ui::{
        ACCENT, BAD, GOOD, MUTED, WARN, classification_name, classification_style, kind_name,
        panel, safe_inline, safe_multiline, status_style,
    },
};
use casefile_core::{Diagnostic, EntrySnapshot, RecordSummary};
use casefile_store::{
    DerivedRecord, EffectiveWriterBinding, StrategyBindingState, WriterBindingSource,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Tabs, Widget, Wrap},
};
use std::cell::Cell;

const TEXT_LIMIT: usize = 8_192;
const BINARY_LIMIT: usize = 256;
const BOARD_COLUMN_LIMIT: usize = 12;
const BOARD_COLUMN_TEXT_LIMIT: usize = 72;
const BOARD_COLUMNS_TEXT_LIMIT: usize = 360;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetailTab {
    Overview,
    Rendered,
    Source,
    Diagnostics,
}

impl DetailTab {
    const ALL: [Self; 4] = [
        Self::Overview,
        Self::Rendered,
        Self::Source,
        Self::Diagnostics,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Rendered => "Rendered",
            Self::Source => "Source",
            Self::Diagnostics => "Diagnostics",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    fn offset(self, amount: isize) -> Self {
        let index = (self.index() as isize + amount).rem_euclid(Self::ALL.len() as isize);
        Self::ALL[index as usize]
    }
}

pub(crate) struct RecordDetail {
    tab: DetailTab,
    scroll: u16,
    rows: Cell<u16>,
}

impl RecordDetail {
    pub(crate) fn new() -> Self {
        Self {
            tab: DetailTab::Overview,
            scroll: 0,
            rows: Cell::new(1),
        }
    }

    pub(crate) fn select_tab(&mut self, offset: isize) {
        self.tab = self.tab.offset(offset);
        self.scroll = 0;
    }

    pub(crate) fn reset_scroll(&mut self) {
        self.scroll = 0;
    }

    pub(crate) fn scroll(&mut self, offset: isize) {
        self.scroll = (self.scroll as isize + offset).clamp(0, self.max_scroll() as isize) as u16;
    }

    pub(crate) fn move_to_edge(&mut self, end: bool) {
        self.scroll = if end { self.max_scroll() } else { 0 };
    }

    pub(crate) fn render(
        &self,
        entry: Option<&EntrySnapshot>,
        derived: Option<&DerivedRecord>,
        diagnostics: &[Diagnostic],
        focused: bool,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let inner = panel("", focused).inner(area);
        if inner.height == 0 || inner.width == 0 {
            self.rows.set(1);
            return;
        }
        let [tabs, content] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .areas(inner);
        let titles = DetailTab::ALL.map(|tab| Line::from(format!(" {} ", tab.title())));
        let text = entry.map_or_else(
            || Text::from("Select a record to inspect it."),
            |entry| Text::from(detail_lines(entry, derived, diagnostics, self.tab)),
        );
        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });
        let line_count = paragraph
            .line_count(content.width)
            .max(1)
            .min(usize::from(u16::MAX)) as u16;
        self.rows.set(line_count);
        let scroll = self.scroll.min(line_count.saturating_sub(1));
        let position = scroll.saturating_add(1);
        let title = entry.map_or_else(
            || " Detail ".to_owned(),
            |entry| {
                format!(
                    " {}  |  line {position}/{line_count} ",
                    safe_inline(entry.identity.as_deref().unwrap_or(&entry.path))
                )
            },
        );
        panel(title, focused).render(area, buffer);
        Tabs::new(titles)
            .select(self.tab.index())
            .divider(" ")
            .style(Style::default().fg(MUTED))
            .highlight_style(Style::default().fg(ACCENT).bold())
            .render(tabs, buffer);
        paragraph.scroll((scroll, 0)).render(content, buffer);
    }

    fn max_scroll(&self) -> u16 {
        self.rows.get().saturating_sub(1)
    }

    #[cfg(test)]
    pub(crate) fn scroll_position(&self) -> u16 {
        self.scroll
    }
}

fn detail_lines(
    entry: &EntrySnapshot,
    derived: Option<&DerivedRecord>,
    diagnostics: &[Diagnostic],
    tab: DetailTab,
) -> Vec<Line<'static>> {
    match tab {
        DetailTab::Overview => overview_lines(entry, derived, diagnostics),
        DetailTab::Rendered => rendered_lines(entry),
        DetailTab::Source => source_lines(&entry.original_bytes),
        DetailTab::Diagnostics => diagnostic_lines(entry, diagnostics),
    }
}

fn rendered_lines(entry: &EntrySnapshot) -> Vec<Line<'static>> {
    match std::str::from_utf8(&entry.original_bytes) {
        Ok(text) if entry.path.ends_with(".md") => markdown::render(text),
        _ => content_lines(&entry.original_bytes),
    }
}

fn source_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            if text.is_empty() {
                return vec![Line::from("Empty text record.").style(Style::default().fg(MUTED))];
            }
            let (safe, truncated) = safe_multiline(text, usize::MAX);
            debug_assert!(!truncated);
            safe.split('\n')
                .map(|line| Line::from(line.to_owned()))
                .collect()
        }
        Err(_) => content_lines(bytes),
    }
}

fn overview_lines(
    entry: &EntrySnapshot,
    derived: Option<&DerivedRecord>,
    diagnostics: &[Diagnostic],
) -> Vec<Line<'static>> {
    let matching = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path == entry.path)
        .count();
    let mut lines = Vec::new();
    if let Some(summary) = entry.summary.as_ref() {
        match summary {
            RecordSummary::WorkItem {
                id,
                title,
                status,
                rank,
            } => {
                lines.push(Line::from(vec![
                    Span::styled(safe_inline(id), Style::default().fg(ACCENT).bold()),
                    Span::raw("  "),
                    Span::styled(
                        format!(" {} ", safe_inline(status).to_uppercase()),
                        status_style(status),
                    ),
                    Span::styled(
                        rank.map(|rank| format!("  rank #{rank}"))
                            .unwrap_or_default(),
                        Style::default().fg(MUTED),
                    ),
                ]));
                lines.push(Line::from(safe_inline(title)).style(Style::default().bold()));
            }
            RecordSummary::Board { id, title, columns } => {
                lines.push(Line::from(safe_inline(id)).style(Style::default().fg(ACCENT).bold()));
                lines.push(Line::from(safe_inline(title)).style(Style::default().bold()));
                lines.push(label_line("Columns", board_columns(columns)));
            }
            RecordSummary::Markdown { title } => {
                lines.push(Line::from(safe_inline(title)).style(Style::default().bold()));
            }
            RecordSummary::Strategy {
                strategy_id,
                phase,
                adapter,
            } => {
                lines.push(
                    Line::from(safe_inline(strategy_id)).style(Style::default().fg(ACCENT).bold()),
                );
                lines.push(label_line("Phase", safe_inline(phase)));
                lines.push(label_line("Adapter", safe_inline(adapter)));
                if let Some(strategy) = derived.and_then(|record| record.strategy.as_ref()) {
                    lines.push(label_line(
                        "Root binding",
                        safe_inline(&strategy.matrix.root_binding),
                    ));
                    lines.push(label_line(
                        "Limits",
                        format!(
                            "{} concurrent subagents, depth {}",
                            strategy.matrix.limits.max_concurrent_subagents,
                            strategy.matrix.limits.max_depth
                        ),
                    ));
                    lines.push(label_line(
                        "Capabilities",
                        strategy.matrix.requirements.capabilities.join(", "),
                    ));
                    lines.push(label_line(
                        "Workers",
                        strategy.matrix.workers.len().to_string(),
                    ));
                    for worker in &strategy.matrix.workers {
                        let runtime = worker
                            .model
                            .as_deref()
                            .zip(worker.reasoning_effort.as_deref())
                            .map(|(model, effort)| format!("  {model} / {effort}"))
                            .unwrap_or_default();
                        lines.push(Line::from(format!(
                            "  {}  {}..{}  {}{}",
                            safe_inline(&worker.role),
                            worker.minimum_count,
                            worker.maximum_count,
                            safe_inline(&worker.platform_profile),
                            safe_inline(&runtime)
                        )));
                    }
                    if let Some(binding) = strategy.binding.as_ref() {
                        lines.extend(binding_state_lines(binding));
                    }
                }
            }
            RecordSummary::StrategyBinding { binding } => {
                lines.push(
                    Line::from("Implementation writer binding")
                        .style(Style::default().fg(ACCENT).bold()),
                );
                lines.push(label_line("Adapter", safe_inline(&binding.adapter)));
                lines.push(label_line("Role", safe_inline(&binding.role)));
                lines.push(label_line("Model", safe_inline(&binding.model)));
                lines.push(label_line(
                    "Reasoning",
                    safe_inline(&binding.reasoning_effort),
                ));
                lines.push(label_line(
                    "Resolution",
                    safe_inline(&binding.resolution.mode),
                ));
                lines.push(label_line(
                    "Catalog value",
                    safe_inline(&binding.resolution.value),
                ));
                if let Some(state) = derived
                    .and_then(|record| record.strategy_binding.as_ref())
                    .map(|binding| &binding.state)
                {
                    lines.extend(binding_state_lines(state));
                }
            }
            RecordSummary::Activation { projects } | RecordSummary::ProjectMap { projects } => {
                lines.push(Line::from("Projects").style(Style::default().fg(ACCENT).bold()));
                for project in projects {
                    lines.push(Line::from(format!("  - {}", safe_inline(project))));
                }
            }
        }
        lines.push(Line::from(""));
    }
    lines.push(label_line("Path", safe_inline(&entry.path)));
    lines.push(Line::from(vec![
        Span::styled("Classification  ", Style::default().fg(MUTED)),
        Span::styled(
            classification_name(entry.classification),
            classification_style(entry.classification),
        ),
    ]));
    lines.push(label_line(
        "Kind",
        entry.kind.map(kind_name).unwrap_or("unknown"),
    ));
    if let Some(identity) = &entry.identity {
        lines.push(label_line("Identity", safe_inline(identity)));
    }
    lines.push(Line::from(vec![
        Span::styled("Diagnostics  ", Style::default().fg(MUTED)),
        Span::styled(
            matching.to_string(),
            Style::default().fg(if matching == 0 { GOOD } else { BAD }),
        ),
    ]));
    lines
}

fn binding_state_lines(state: &StrategyBindingState) -> Vec<Line<'static>> {
    match state {
        StrategyBindingState::Absent { effective } => {
            effective_binding_lines("matrix default", effective)
        }
        StrategyBindingState::Pending => vec![label_line("Binding state", "pending")],
        StrategyBindingState::Resolved { effective } => {
            effective_binding_lines("resolved", effective)
        }
        StrategyBindingState::Unresolved => vec![label_line("Binding state", "unresolved")],
        StrategyBindingState::Invalid => vec![label_line("Binding state", "invalid")],
    }
}

fn effective_binding_lines(state: &str, effective: &EffectiveWriterBinding) -> Vec<Line<'static>> {
    let source = match effective.source {
        WriterBindingSource::Matrix => "matrix",
        WriterBindingSource::Binding => "binding",
    };
    vec![
        label_line("Binding state", state),
        label_line("Effective writer", safe_inline(&effective.model)),
        label_line(
            "Effective reasoning",
            safe_inline(&effective.reasoning_effort),
        ),
        label_line("Effective source", source),
    ]
}

fn content_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            if text.is_empty() {
                return vec![Line::from("Empty text record.").style(Style::default().fg(MUTED))];
            }
            let (safe, truncated) = safe_multiline(text, TEXT_LIMIT);
            let mut lines: Vec<_> = safe
                .split('\n')
                .map(|line| Line::from(line.to_owned()))
                .collect();
            if truncated {
                lines.push(Line::from(""));
                lines.push(
                    Line::from(format!("... truncated at {TEXT_LIMIT} characters"))
                        .style(Style::default().fg(WARN)),
                );
            }
            lines
        }
        Err(_) => {
            let mut lines = vec![
                Line::from(format!("Binary content  |  {} bytes", bytes.len()))
                    .style(Style::default().fg(WARN).bold()),
                Line::from(""),
            ];
            for (row, chunk) in bytes
                .iter()
                .take(BINARY_LIMIT)
                .collect::<Vec<_>>()
                .chunks(16)
                .enumerate()
            {
                lines.push(Line::from(format!(
                    "{:04x}  {}",
                    row * 16,
                    chunk
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                )));
            }
            if bytes.len() > BINARY_LIMIT {
                lines.push(Line::from(""));
                lines.push(
                    Line::from(format!("... truncated at {BINARY_LIMIT} bytes"))
                        .style(Style::default().fg(WARN)),
                );
            }
            lines
        }
    }
}

fn diagnostic_lines(entry: &EntrySnapshot, diagnostics: &[Diagnostic]) -> Vec<Line<'static>> {
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path == entry.path)
        .collect();
    if matching.is_empty() {
        return vec![
            Line::from("No diagnostics for this record.").style(Style::default().fg(GOOD)),
            Line::from("Cross-record findings remain in the scanner channel.")
                .style(Style::default().fg(MUTED)),
        ];
    }
    let mut lines = Vec::new();
    for diagnostic in matching {
        lines.push(Line::from(vec![
            Span::styled("! ", Style::default().fg(BAD)),
            Span::styled(
                safe_inline(&diagnostic.code),
                Style::default().fg(BAD).bold(),
            ),
        ]));
        lines.push(Line::from(format!(
            "  {}",
            safe_inline(&diagnostic.message)
        )));
        if let Some(field) = diagnostic.field.as_deref() {
            lines.push(label_line("  Field", safe_inline(field)));
        }
        if let Some(section) = diagnostic.section.as_deref() {
            lines.push(label_line("  Section", safe_inline(section)));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn board_columns(columns: &[String]) -> String {
    let mut displayed: Vec<_> = columns
        .iter()
        .take(BOARD_COLUMN_LIMIT)
        .map(|column| bounded_terminal_text(column, BOARD_COLUMN_TEXT_LIMIT))
        .collect();
    while !displayed.is_empty()
        && board_columns_text(&displayed, columns.len() - displayed.len())
            .chars()
            .count()
            > BOARD_COLUMNS_TEXT_LIMIT
    {
        displayed.pop();
    }
    board_columns_text(&displayed, columns.len() - displayed.len())
}

fn board_columns_text(columns: &[String], omitted: usize) -> String {
    let mut text = columns.join(", ");
    if omitted > 0 {
        if !text.is_empty() {
            text.push_str(", ");
        }
        text.push_str(&format!("... +{omitted} columns omitted"));
    }
    text
}

fn bounded_terminal_text(text: &str, limit: usize) -> String {
    let mut characters = text.chars().peekable();
    let mut output = String::new();
    let mut length = 0;
    while let Some(character) = characters.next() {
        let escaped = if character.is_control() {
            character.escape_default().to_string()
        } else {
            character.to_string()
        };
        let marker_length = usize::from(characters.peek().is_some());
        let escaped_length = escaped.chars().count();
        if length + escaped_length + marker_length > limit {
            output.push_str("...");
            break;
        }
        length += escaped_length;
        output.push_str(&escaped);
    }
    output
}

fn label_line(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}  "), Style::default().fg(MUTED)),
        Span::raw(value.into()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::entry;
    use crate::ui::{safe_inline, safe_multiline};
    use casefile_core::{Classification, Diagnostic, Kind, RecordSummary};
    use ratatui::{Terminal, backend::TestBackend};

    fn render_detail(
        detail: &RecordDetail,
        entry: &EntrySnapshot,
        diagnostics: &[Diagnostic],
        width: u16,
        height: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                detail.render(
                    Some(entry),
                    None,
                    diagnostics,
                    true,
                    frame.area(),
                    frame.buffer_mut(),
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn content_keeps_lines_and_escapes_only_unsafe_controls() {
        let (safe, truncated) = safe_multiline("first\nsecond\t\x1b", TEXT_LIMIT);
        assert_eq!(safe, "first\nsecond    \\u{1b}");
        assert!(!truncated);
        assert_eq!(safe_inline("caf\u{e9}\x1b"), "caf\u{e9}\\u{1b}");
        assert_eq!(content_lines(b"")[0].to_string(), "Empty text record.");

        let entry = entry(
            "a-ticket.md",
            Classification::Governed,
            Some(Kind::Ticket),
            None,
            b"first line\nsecond line\n\x1b[31mnot a colour",
        );
        let mut detail = RecordDetail::new();
        detail.select_tab(1);
        let output = render_detail(&detail, &entry, &[], 120, 32);
        assert!(output.contains("first line"));
        assert!(output.contains("second line"));
        assert!(output.contains(r"\u{1b}[31mnot a colour"));
        assert!(!output.contains("first line\\nsecond line"));
        assert!(!output.contains('\x1b'));
    }

    #[test]
    fn source_keeps_text_beyond_the_display_limit() {
        let tail = "source tail remains available";
        let source = format!("{}\n{tail}", "x".repeat(TEXT_LIMIT + 1));
        let entry = entry(
            "a-ticket.md",
            Classification::Governed,
            Some(Kind::Ticket),
            None,
            source.as_bytes(),
        );

        let visible = detail_lines(&entry, None, &[], DetailTab::Source)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(visible.contains(tail));
        assert!(!visible.contains("truncated"));
    }

    #[test]
    fn metadata_and_diagnostics_cannot_inject_terminal_controls() {
        let control = "\x1b]0;metadata\x07";
        let path = format!("{control}-ticket.md");
        let entry = entry(
            &path,
            Classification::Invalid,
            Some(Kind::Ticket),
            Some(RecordSummary::WorkItem {
                id: format!("HMD-{control}"),
                title: format!("title-{control}"),
                status: format!("status-{control}"),
                rank: None,
            }),
            b"content",
        );
        let diagnostics = vec![
            Diagnostic::new(
                &path,
                &format!("code-{control}"),
                format!("message-{control}"),
            )
            .field(&format!("field-{control}"))
            .section(&format!("section-{control}")),
        ];
        let mut detail = RecordDetail::new();
        detail.select_tab(3);
        let output = render_detail(&detail, &entry, &diagnostics, 160, 32);
        assert!(output.contains(r"code-\u{1b}]0;metadata\u{7}"));
        assert!(output.contains(r"field-\u{1b}]0;metadata\u{7}"));
        assert!(output.contains(r"section-\u{1b}]0;metadata\u{7}"));
        assert!(!output.contains('\x1b'));
        assert!(!output.contains('\x07'));
    }

    #[test]
    fn metadata_and_board_columns_escape_controls_and_remain_bounded() {
        let metadata = "\x1b]0;metadata\x07";
        let columns: Vec<_> = (0..BOARD_COLUMN_LIMIT + 4)
            .map(|index| format!("column-{index}-{metadata}-{}", "x".repeat(100)))
            .collect();
        let rendered = board_columns(&columns);
        assert!(rendered.contains(r"\u{1b}]0;metadata\u{7}"));
        assert!(rendered.contains("columns omitted"));
        assert!(rendered.chars().count() <= BOARD_COLUMNS_TEXT_LIMIT);
        assert!(!rendered.chars().any(char::is_control));
    }
}
