//! Read-only terminal workbench for an already scanned Casefile snapshot.

use casefile_core::{Classification, Diagnostic, EntrySnapshot, Kind, RecordSummary};
use casefile_store::{ActivationState, ScanResult};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget,
        Tabs, Widget, Wrap,
    },
};
use std::{
    cell::Cell,
    io::{self, Stdout},
};

const TEXT_LIMIT: usize = 8_192;
const BINARY_LIMIT: usize = 256;
const BOARD_COLUMN_LIMIT: usize = 12;
const BOARD_COLUMN_TEXT_LIMIT: usize = 72;
const BOARD_COLUMNS_TEXT_LIMIT: usize = 360;
const PAGE_SIZE: isize = 10;
const WIDE_MINIMUM: u16 = 96;

const ACCENT: Color = Color::Rgb(91, 192, 235);
const MUTED: Color = Color::Rgb(118, 126, 138);
const BORDER: Color = Color::Rgb(68, 75, 86);
const SELECTED: Color = Color::Rgb(37, 52, 67);
const GOOD: Color = Color::Rgb(117, 190, 96);
const WARN: Color = Color::Rgb(229, 192, 123);
const BAD: Color = Color::Rgb(224, 108, 117);
const RAW: Color = Color::Rgb(198, 120, 221);

/// Starts the read-only workbench for an already scanned snapshot.
pub fn run(scan: ScanResult) -> io::Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(scan);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum View {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetailTab {
    Overview,
    Content,
    Diagnostics,
}

impl DetailTab {
    const ALL: [Self; 3] = [Self::Overview, Self::Content, Self::Diagnostics];

    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Content => "Content",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    List,
    Detail,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::List => Self::Detail,
            Self::Detail => Self::List,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayoutMode {
    Wide,
    Narrow,
}

struct App {
    scan: ScanResult,
    view: View,
    detail_tab: DetailTab,
    focus: Focus,
    selected_path: Option<String>,
    filter: String,
    entering_filter: bool,
    show_help: bool,
    detail_scroll: u16,
    detail_rows: Cell<u16>,
    quit: bool,
}

impl App {
    fn new(scan: ScanResult) -> Self {
        let mut app = Self {
            scan,
            view: View::WorkQueue,
            detail_tab: DetailTab::Overview,
            focus: Focus::List,
            selected_path: None,
            filter: String::new(),
            entering_filter: false,
            show_help: false,
            detail_scroll: 0,
            detail_rows: Cell::new(1),
            quit: false,
        };
        app.normalise_selection();
        app
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        while !self.quit {
            terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle(key.code);
            }
        }
        Ok(())
    }

    fn handle(&mut self, key: KeyCode) {
        if self.show_help {
            match key {
                KeyCode::Char('q') => self.quit = true,
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => self.show_help = false,
                _ => {}
            }
            return;
        }
        if self.entering_filter {
            match key {
                KeyCode::Esc | KeyCode::Enter => self.entering_filter = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.normalise_selection();
                }
                KeyCode::Char(character) => {
                    self.filter.push(character);
                    self.normalise_selection();
                }
                _ => {}
            }
            return;
        }
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('1') => self.set_view(View::WorkQueue),
            KeyCode::Char('2') => self.set_view(View::Records),
            KeyCode::Char('t') => self.set_view(self.view.next()),
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Char('/') => self.entering_filter = true,
            KeyCode::Char('c') => {
                self.filter.clear();
                self.normalise_selection();
            }
            KeyCode::Left | KeyCode::Char('h') => self.set_detail_tab(-1),
            KeyCode::Right | KeyCode::Char('l') => self.set_detail_tab(1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focus(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focus(-1),
            KeyCode::PageDown => self.move_focus(PAGE_SIZE),
            KeyCode::PageUp => self.move_focus(-PAGE_SIZE),
            KeyCode::Home => self.move_to_edge(false),
            KeyCode::End => self.move_to_edge(true),
            _ => {}
        }
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
        self.detail_scroll = 0;
        self.normalise_selection();
    }

    fn set_detail_tab(&mut self, offset: isize) {
        self.detail_tab = self.detail_tab.offset(offset);
        self.detail_scroll = 0;
    }

    fn move_focus(&mut self, offset: isize) {
        match self.focus {
            Focus::List => self.select_offset(offset),
            Focus::Detail => self.scroll_detail(offset),
        }
    }

    fn move_to_edge(&mut self, end: bool) {
        match self.focus {
            Focus::List => {
                let next = self
                    .entries()
                    .get(if end {
                        self.entries().len().saturating_sub(1)
                    } else {
                        0
                    })
                    .map(|entry| entry.path.clone());
                if next != self.selected_path {
                    self.selected_path = next;
                    self.detail_scroll = 0;
                }
            }
            Focus::Detail => {
                self.detail_scroll = if end { self.max_detail_scroll() } else { 0 };
            }
        }
    }

    fn entries(&self) -> Vec<&EntrySnapshot> {
        self.scan
            .snapshot
            .entries
            .iter()
            .filter(|entry| self.matches_view(entry) && self.matches_filter(entry))
            .collect()
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

    fn work_count(&self) -> usize {
        self.scan
            .snapshot
            .entries
            .iter()
            .filter(|entry| {
                entry.classification == Classification::Governed
                    && matches!(entry.summary, Some(RecordSummary::WorkItem { .. }))
            })
            .count()
    }

    fn selected(&self) -> Option<&EntrySnapshot> {
        let path = self.selected_path.as_deref()?;
        self.scan
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
    }

    fn selected_index(&self) -> Option<usize> {
        self.entries()
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
    }

    fn normalise_selection(&mut self) {
        let next = {
            let entries = self.entries();
            if entries
                .iter()
                .any(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
            {
                self.selected_path.clone()
            } else {
                entries.first().map(|entry| entry.path.clone())
            }
        };
        if next != self.selected_path {
            self.selected_path = next;
            self.detail_scroll = 0;
        }
    }

    fn select_offset(&mut self, offset: isize) {
        let entries = self.entries();
        if entries.is_empty() {
            self.selected_path = None;
            self.detail_scroll = 0;
            return;
        }
        let index = entries
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
            .unwrap_or(0);
        let next = (index as isize + offset).clamp(0, entries.len() as isize - 1) as usize;
        let path = entries[next].path.clone();
        if self.selected_path.as_deref() != Some(path.as_str()) {
            self.selected_path = Some(path);
            self.detail_scroll = 0;
        }
    }

    fn scroll_detail(&mut self, offset: isize) {
        self.detail_scroll = (self.detail_scroll as isize + offset)
            .clamp(0, self.max_detail_scroll() as isize) as u16;
    }

    fn max_detail_scroll(&self) -> u16 {
        self.detail_rows.get().saturating_sub(1)
    }

    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let [header, body, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .areas(area);
        self.render_header(header, buffer);

        match layout_mode(body) {
            LayoutMode::Wide => {
                let [list, detail] = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(64), Constraint::Min(32)])
                    .areas(body);
                self.render_list(list, buffer);
                self.render_detail(detail, buffer);
            }
            LayoutMode::Narrow => {
                let [list, detail] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                    .areas(body);
                self.render_list(list, buffer);
                self.render_detail(detail, buffer);
            }
        }

        let footer_text = if self.entering_filter {
            " Type to filter  Enter accept  Esc close "
        } else {
            " 1/2 view  Tab focus  j/k move  PgUp/PgDn page  h/l detail  / filter  ? help  q quit "
        };
        Paragraph::new(footer_text)
            .style(Style::default().fg(MUTED))
            .render(footer, buffer);

        if self.show_help {
            self.render_help(area, buffer);
        }
    }

    fn render_header(&self, area: Rect, buffer: &mut Buffer) {
        let work_style = tab_style(self.view == View::WorkQueue);
        let records_style = tab_style(self.view == View::Records);
        let filter = if self.filter.is_empty() {
            "none".to_owned()
        } else {
            format!("\"{}\"", safe_inline(&self.filter))
        };
        let visible = self.entries().len();
        let total = match self.view {
            View::WorkQueue => self.work_count(),
            View::Records => self.scan.snapshot.entries.len(),
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(" CASEFILE ", Style::default().fg(ACCENT).bold()),
                Span::styled(format!(" [1] WORK {} ", self.work_count()), work_style),
                Span::raw(" "),
                Span::styled(
                    format!(" [2] RECORDS {} ", self.scan.snapshot.entries.len()),
                    records_style,
                ),
                Span::raw("   "),
                Span::styled(
                    activation_name(self.scan.activation),
                    activation_style(self.scan.activation).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  |  {} diagnostics", self.scan.diagnostics.len()),
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

    fn render_list(&self, area: Rect, buffer: &mut Buffer) {
        let entries = self.entries();
        let position = self
            .selected_index()
            .map(|index| format!("{} / {}", index + 1, entries.len()))
            .unwrap_or_else(|| format!("0 / {}", entries.len()));
        let block = panel(
            format!(" {}  {position} ", self.view.title()),
            self.focus == Focus::List,
        );
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
        state.select(self.selected_index());
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

    fn render_detail(&self, area: Rect, buffer: &mut Buffer) {
        let inner = panel("", self.focus == Focus::Detail).inner(area);
        if inner.height == 0 || inner.width == 0 {
            self.detail_rows.set(1);
            return;
        }
        let [tabs, content] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .areas(inner);
        let titles = DetailTab::ALL.map(|tab| Line::from(format!(" {} ", tab.title())));
        let text = self.selected().map_or_else(
            || Text::from("Select a record to inspect it."),
            |entry| Text::from(detail_lines(entry, &self.scan.diagnostics, self.detail_tab)),
        );
        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });
        let line_count = paragraph
            .line_count(content.width)
            .max(1)
            .min(usize::from(u16::MAX)) as u16;
        self.detail_rows.set(line_count);
        let scroll = self.detail_scroll.min(line_count.saturating_sub(1));
        let position = scroll.saturating_add(1);
        let title = self.selected().map_or_else(
            || " Detail ".to_owned(),
            |entry| {
                format!(
                    " {}  |  line {position}/{line_count} ",
                    safe_inline(entry.identity.as_deref().unwrap_or(&entry.path))
                )
            },
        );
        panel(title, self.focus == Focus::Detail).render(area, buffer);
        Tabs::new(titles)
            .select(self.detail_tab.index())
            .divider(" ")
            .style(Style::default().fg(MUTED))
            .highlight_style(Style::default().fg(ACCENT).bold())
            .render(tabs, buffer);
        paragraph.scroll((scroll, 0)).render(content, buffer);
    }

    fn render_help(&self, area: Rect, buffer: &mut Buffer) {
        let popup = centred(area, 68, 20);
        Clear.render(popup, buffer);
        let lines = vec![
            Line::from("MOVE").style(Style::default().fg(ACCENT).bold()),
            help_line("j / k, Up / Down", "Move selection or scroll focused pane"),
            help_line("PgUp / PgDn", "Page through the focused pane"),
            help_line("Home / End", "Jump to the first or last position"),
            help_line("Tab", "Switch focus between list and detail"),
            Line::from(""),
            Line::from("VIEW").style(Style::default().fg(ACCENT).bold()),
            help_line("1 / 2", "Open Work queue or Records"),
            help_line(
                "h / l, Left / Right",
                "Switch Overview, Content, Diagnostics",
            ),
            help_line("/", "Enter filter mode"),
            help_line("c", "Clear the active filter"),
            Line::from(""),
            help_line("? / Esc / Enter", "Close this help"),
            help_line("q", "Quit Casefile"),
        ];
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Keyboard help ")
                    .title_style(Style::default().fg(ACCENT).bold())
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ACCENT)),
            )
            .wrap(Wrap { trim: false })
            .render(popup, buffer);
    }
}

fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { ACCENT } else { BORDER }))
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

fn layout_mode(area: Rect) -> LayoutMode {
    if area.width >= WIDE_MINIMUM {
        LayoutMode::Wide
    } else {
        LayoutMode::Narrow
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

fn detail_lines(
    entry: &EntrySnapshot,
    diagnostics: &[Diagnostic],
    tab: DetailTab,
) -> Vec<Line<'static>> {
    match tab {
        DetailTab::Overview => overview_lines(entry, diagnostics),
        DetailTab::Content => content_lines(&entry.original_bytes),
        DetailTab::Diagnostics => diagnostic_lines(entry, diagnostics),
    }
}

fn overview_lines(entry: &EntrySnapshot, diagnostics: &[Diagnostic]) -> Vec<Line<'static>> {
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

fn safe_multiline(text: &str, limit: usize) -> (String, bool) {
    let mut output = String::new();
    let mut characters = text.chars().peekable();
    let mut count = 0;
    while let Some(character) = characters.next() {
        if count == limit {
            return (output, true);
        }
        count += 1;
        match character {
            '\r' if characters.peek() == Some(&'\n') => {}
            '\n' => output.push('\n'),
            '\t' => output.push_str("    "),
            character if character.is_control() => output.extend(character.escape_default()),
            character => output.push(character),
        }
    }
    (output, false)
}

fn safe_inline(text: &str) -> String {
    let mut output = String::new();
    for character in text.chars() {
        if character.is_control() {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output
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

fn classification_name(classification: Classification) -> &'static str {
    match classification {
        Classification::Governed => "governed",
        Classification::Ungoverned => "ungoverned",
        Classification::Invalid => "invalid",
        Classification::Raw => "raw",
    }
}

fn classification_style(classification: Classification) -> Style {
    Style::default().fg(match classification {
        Classification::Governed => GOOD,
        Classification::Ungoverned => WARN,
        Classification::Invalid => BAD,
        Classification::Raw => RAW,
    })
}

fn status_style(status: &str) -> Style {
    let color = match status.to_ascii_lowercase().as_str() {
        "accepted" | "complete" | "completed" | "done" => GOOD,
        "rejected" | "blocked" | "failed" => BAD,
        "pending" | "proposed" | "review" => WARN,
        _ => ACCENT,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
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

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Activation => "activation",
        Kind::ProjectMap => "project_map",
        Kind::Request => "request",
        Kind::Decision => "decision",
        Kind::Evidence => "evidence",
        Kind::Review => "review",
        Kind::Plan => "plan",
        Kind::Closeout => "closeout",
        Kind::Strategy => "strategy",
        Kind::Ticket => "ticket",
        Kind::Epic => "epic",
        Kind::Board => "board",
    }
}

fn summary_title(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::Markdown { title })
        | Some(RecordSummary::WorkItem { title, .. })
        | Some(RecordSummary::Board { title, .. }) => title,
        _ => "",
    }
}

fn work_status(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::WorkItem { status, .. }) => status,
        _ => "",
    }
}

fn label_line(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}  "), Style::default().fg(MUTED)),
        Span::raw(value.into()),
    ])
}

fn help_line(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<18}"), Style::default().fg(WARN)),
        Span::raw(description.to_owned()),
    ])
}

fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use casefile_core::{CasefileSnapshot, Revision};
    use ratatui::{Terminal, backend::TestBackend};

    fn entry(
        path: &str,
        classification: Classification,
        kind: Option<Kind>,
        summary: Option<RecordSummary>,
        bytes: &[u8],
    ) -> EntrySnapshot {
        EntrySnapshot {
            path: path.into(),
            classification,
            kind,
            identity: summary.as_ref().and_then(|summary| match summary {
                RecordSummary::WorkItem { id, .. } | RecordSummary::Board { id, .. } => {
                    Some(id.clone())
                }
                _ => None,
            }),
            content_revision: Revision("sha256:entry".into()),
            summary,
            original_bytes: bytes.into(),
        }
    }

    fn scan() -> ScanResult {
        ScanResult {
            activation: ActivationState::Active,
            snapshot: CasefileSnapshot {
                revision: Revision("sha256:scan".into()),
                entries: vec![
                    entry(
                        "a-ticket.md",
                        Classification::Governed,
                        Some(Kind::Ticket),
                        Some(RecordSummary::WorkItem {
                            id: "HMD-013".into(),
                            title: "Navigator".into(),
                            status: "accepted".into(),
                            rank: Some(3),
                        }),
                        b"first line\nsecond line\n\x1b[31mnot a colour",
                    ),
                    entry(
                        "b-board.toml",
                        Classification::Governed,
                        Some(Kind::Board),
                        Some(RecordSummary::Board {
                            id: "HMD-board".into(),
                            title: "Board".into(),
                            columns: vec!["Ready".into(), "Done".into()],
                        }),
                        b"board",
                    ),
                    entry(
                        "c-legacy.txt",
                        Classification::Ungoverned,
                        None,
                        None,
                        b"legacy",
                    ),
                    entry(
                        "d-invalid.md",
                        Classification::Invalid,
                        Some(Kind::Ticket),
                        None,
                        &[0xff, 0x00, 0x10],
                    ),
                    entry("e-raw.txt", Classification::Raw, None, None, b"raw"),
                ],
            },
            diagnostics: vec![
                Diagnostic::new("d-invalid.md", "invalid_shape", "ticket is incomplete"),
                Diagnostic::new("a-ticket.md", "cross_record", "separate scanner channel"),
            ],
        }
    }

    fn render(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
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
    fn wide_work_queue_has_compact_navigation_and_overview() {
        let app = App::new(scan());
        let output = render(&app, 120, 32);
        assert!(output.contains("CASEFILE"));
        assert!(output.contains("[1] WORK 1"));
        assert!(output.contains("[2] RECORDS 5"));
        assert!(output.contains("ACTIVE"));
        assert!(output.contains("ACCEPTED"));
        assert!(output.contains("HMD-013"));
        assert!(output.contains("Overview"));
        assert!(output.contains("rank #3"));
        assert_eq!(layout_mode(Rect::new(0, 0, 120, 20)), LayoutMode::Wide);
    }

    #[test]
    fn narrow_layout_stacks_complete_list_and_detail_panels() {
        let app = App::new(scan());
        let output = render(&app, 72, 38);
        let queue = output.find("Work queue").expect("queue");
        let overview = output.find("Overview").expect("overview");
        assert!(queue < overview);
        assert!(output.contains("HMD-013"));
        assert_eq!(layout_mode(Rect::new(0, 0, 72, 30)), LayoutMode::Narrow);
    }

    #[test]
    fn content_keeps_lines_and_escapes_only_unsafe_controls() {
        let (safe, truncated) = safe_multiline("first\nsecond\t\x1b", TEXT_LIMIT);
        assert_eq!(safe, "first\nsecond    \\u{1b}");
        assert!(!truncated);
        assert_eq!(safe_inline("caf\u{e9}\x1b"), "caf\u{e9}\\u{1b}");
        assert_eq!(content_lines(b"")[0].to_string(), "Empty text record.");
        let mut app = App::new(scan());
        app.handle(KeyCode::Right);
        let output = render(&app, 120, 32);
        assert!(output.contains("first line"));
        assert!(output.contains("second line"));
        assert!(output.contains(r"\u{1b}[31mnot a colour"));
        assert!(!output.contains("first line\\nsecond line"));
        assert!(!output.chars().any(|character| character == '\x1b'));
    }

    #[test]
    fn metadata_and_diagnostics_cannot_inject_terminal_controls() {
        let control = "\x1b]0;metadata\x07";
        let path = format!("{control}-ticket.md");
        let scan = ScanResult {
            activation: ActivationState::Active,
            snapshot: CasefileSnapshot {
                revision: Revision("sha256:controls".into()),
                entries: vec![entry(
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
                )],
            },
            diagnostics: vec![
                Diagnostic::new(
                    &path,
                    &format!("code-{control}"),
                    format!("message-{control}"),
                )
                .field(&format!("field-{control}"))
                .section(&format!("section-{control}")),
            ],
        };
        let mut app = App::new(scan);
        app.handle(KeyCode::Char('2'));
        app.handle(KeyCode::Right);
        app.handle(KeyCode::Right);
        let output = render(&app, 160, 32);
        assert!(output.contains(r"code-\u{1b}]0;metadata\u{7}"));
        assert!(output.contains(r"field-\u{1b}]0;metadata\u{7}"));
        assert!(output.contains(r"section-\u{1b}]0;metadata\u{7}"));
        assert!(!output.contains('\x1b'));
        assert!(!output.contains('\x07'));
    }

    #[test]
    fn records_keep_classification_diagnostics_and_binary_separate() {
        let mut app = App::new(scan());
        app.handle(KeyCode::Char('2'));
        for _ in 0..3 {
            app.handle(KeyCode::Down);
        }
        let output = render(&app, 120, 32);
        for label in ["GOVERNED", "UNGOVERNED", "INVALID", "RAW"] {
            assert!(output.contains(label), "missing {label}");
        }
        app.handle(KeyCode::Right);
        let output = render(&app, 120, 32);
        assert!(output.contains("Binary content"));
        assert!(output.contains("ff 00 10"));
        app.handle(KeyCode::Right);
        let output = render(&app, 120, 32);
        assert!(output.contains("invalid_shape"));
        assert!(output.contains("ticket is incomplete"));
        assert!(!output.contains("cross_record"));
    }

    #[test]
    fn filter_empty_state_and_path_selection_remain_predictable() {
        let mut app = App::new(scan());
        app.handle(KeyCode::Char('2'));
        app.handle(KeyCode::Down);
        assert_eq!(app.selected_path.as_deref(), Some("b-board.toml"));
        app.handle(KeyCode::Char('/'));
        for key in "missing".chars().map(KeyCode::Char) {
            app.handle(key);
        }
        app.handle(KeyCode::Enter);
        assert_eq!(app.selected_path, None);
        assert!(render(&app, 90, 28).contains("No records match"));
        app.handle(KeyCode::Char('c'));
        assert_eq!(app.selected_path.as_deref(), Some("a-ticket.md"));
    }

    #[test]
    fn focus_page_navigation_detail_scrolling_and_help_are_visible() {
        let mut source = scan();
        source.snapshot.entries[0].original_bytes = "wrapped content ".repeat(300).into_bytes();
        let mut app = App::new(source);
        app.handle(KeyCode::Right);
        render(&app, 70, 24);
        app.handle(KeyCode::Tab);
        app.handle(KeyCode::PageDown);
        assert!(app.detail_scroll > 0);
        app.handle(KeyCode::Char('?'));
        let output = render(&app, 100, 30);
        assert!(output.contains("Keyboard help"));
        assert!(output.contains("Switch focus between list and detail"));
        app.handle(KeyCode::Esc);
        assert!(!app.show_help);
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

    #[test]
    fn empty_snapshot_has_useful_work_and_record_states() {
        let scan = ScanResult {
            activation: ActivationState::Unactivated,
            snapshot: CasefileSnapshot {
                revision: Revision("sha256:empty".into()),
                entries: Vec::new(),
            },
            diagnostics: Vec::new(),
        };
        let mut app = App::new(scan);
        assert!(render(&app, 100, 28).contains("No governed tickets or epics"));
        app.handle(KeyCode::Char('2'));
        let output = render(&app, 100, 28);
        assert!(output.contains("No records in this Casefile root"));
        assert!(output.contains("UNACTIVATED"));
    }
}
