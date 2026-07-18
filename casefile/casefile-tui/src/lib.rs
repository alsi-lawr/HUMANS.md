//! Flat, read-only terminal rendering for a scanned Casefile snapshot.

use casefile_core::{Classification, EntrySnapshot, RecordSummary};
use casefile_store::ScanResult;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};
use std::io::{self, Stdout};

const TEXT_LIMIT: usize = 2_048;
const BINARY_LIMIT: usize = 256;

/// Starts the read-only navigator for an already scanned snapshot.
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
}

struct App {
    scan: ScanResult,
    view: View,
    selected_path: Option<String>,
    filter: String,
    entering_filter: bool,
    quit: bool,
}

impl App {
    fn new(scan: ScanResult) -> Self {
        let mut app = Self {
            scan,
            view: View::WorkQueue,
            selected_path: None,
            filter: String::new(),
            entering_filter: false,
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
            KeyCode::Char('t') | KeyCode::Tab => {
                self.view = match self.view {
                    View::WorkQueue => View::Records,
                    View::Records => View::WorkQueue,
                };
                self.normalise_selection();
            }
            KeyCode::Char('/') => self.entering_filter = true,
            KeyCode::Char('c') => {
                self.filter.clear();
                self.normalise_selection();
            }
            KeyCode::Down | KeyCode::Char('j') => self.select_offset(1),
            KeyCode::Up | KeyCode::Char('k') => self.select_offset(-1),
            _ => {}
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

    fn selected(&self) -> Option<&EntrySnapshot> {
        let path = self.selected_path.as_deref()?;
        self.scan
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
    }

    fn normalise_selection(&mut self) {
        let paths: Vec<_> = self
            .entries()
            .into_iter()
            .map(|entry| &entry.path)
            .collect();
        if !paths
            .iter()
            .any(|path| Some(path.as_str()) == self.selected_path.as_deref())
        {
            self.selected_path = paths.first().map(|path| (*path).clone());
        }
    }

    fn select_offset(&mut self, offset: isize) {
        let entries = self.entries();
        let Some(index) = entries
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref())
        else {
            self.normalise_selection();
            return;
        };
        let next = (index as isize + offset).clamp(0, entries.len() as isize - 1) as usize;
        self.selected_path = Some(entries[next].path.clone());
    }

    fn render(&self, area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        let [header, body] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .areas(area);
        let filter = if self.entering_filter {
            " (typing)"
        } else {
            ""
        };
        Paragraph::new(format!(
            " {} | / filter{filter}: {} | t switch | j/k move | c clear | q quit ",
            self.view.title(),
            escape_terminal(&self.filter)
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Casefile (read-only)"),
        )
        .render(header, buffer);

        let [list_area, detail_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .areas(body);
        self.render_list(list_area, buffer);
        self.render_detail(detail_area, buffer);
    }

    fn render_list(&self, area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        let entries = self.entries();
        let items: Vec<_> = entries
            .iter()
            .map(|entry| ListItem::new(list_label(entry, self.view)))
            .collect();
        let selected = entries
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref());
        let mut state = ListState::default();
        state.select(selected);
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(self.view.title()),
            )
            .highlight_style(Style::default().reversed());
        StatefulWidget::render(list, area, buffer, &mut state);
    }

    fn render_detail(&self, area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        let text = self.selected().map_or_else(
            || Text::from("No matching records."),
            |entry| Text::from(detail(entry, &self.scan.diagnostics)),
        );
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Detail"))
            .wrap(Wrap { trim: false })
            .render(area, buffer);
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
        ) => Line::from(format!(
            "{} | {} | {}{}",
            escape_terminal(id),
            escape_terminal(title),
            escape_terminal(status),
            rank.map(|rank| format!(" | #{rank}")).unwrap_or_default()
        )),
        _ => Line::from(format!(
            "{} | {} | {}",
            escape_terminal(&entry.path),
            classification_name(entry.classification),
            entry.kind.map(kind_name).unwrap_or("unknown")
        ))
        .style(classification_style(entry.classification)),
    }
}

fn detail(entry: &EntrySnapshot, diagnostics: &[casefile_core::Diagnostic]) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("Path: {}", escape_terminal(&entry.path))),
        Line::from(format!(
            "Classification: {}",
            classification_name(entry.classification)
        ))
        .style(classification_style(entry.classification)),
        Line::from(format!(
            "Kind: {}",
            entry.kind.map(kind_name).unwrap_or("unknown")
        )),
    ];
    if let Some(identity) = &entry.identity {
        lines.push(Line::from(format!(
            "Identity: {}",
            escape_terminal(identity)
        )));
    }
    match entry.summary.as_ref() {
        Some(RecordSummary::Markdown { title }) => {
            lines.push(Line::from(format!("Title: {}", escape_terminal(title))))
        }
        Some(RecordSummary::Strategy {
            strategy_id,
            phase,
            adapter,
        }) => lines.push(Line::from(format!(
            "Strategy: {} | {} | {}",
            escape_terminal(strategy_id),
            escape_terminal(phase),
            escape_terminal(adapter)
        ))),
        Some(RecordSummary::WorkItem {
            id,
            title,
            status,
            rank,
        }) => lines.push(Line::from(format!(
            "Work item: {} | {} | status: {}{}",
            escape_terminal(id),
            escape_terminal(title),
            escape_terminal(status),
            rank.map(|rank| format!(" | rank: {rank}"))
                .unwrap_or_default()
        ))),
        Some(RecordSummary::Board { id, title, columns }) => lines.push(Line::from(format!(
            "Board: {} | {} | columns: {}",
            escape_terminal(id),
            escape_terminal(title),
            columns
                .iter()
                .map(|column| escape_terminal(column))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        Some(RecordSummary::Activation { projects })
        | Some(RecordSummary::ProjectMap { projects }) => {
            lines.push(Line::from(format!(
                "Projects: {}",
                projects
                    .iter()
                    .map(|project| escape_terminal(project))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        None => {}
    }
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path == entry.path)
        .collect();
    if !matching.is_empty() {
        lines.push(Line::from("Diagnostics:".bold()));
        lines.extend(matching.into_iter().map(|diagnostic| {
            let context = [
                diagnostic
                    .field
                    .as_deref()
                    .map(|field| format!("field={}", escape_terminal(field))),
                diagnostic
                    .section
                    .as_deref()
                    .map(|section| format!("section={}", escape_terminal(section))),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            Line::from(format!(
                "- {}: {}{}: {}",
                escape_terminal(&diagnostic.path),
                escape_terminal(&diagnostic.code),
                if context.is_empty() {
                    String::new()
                } else {
                    format!(" ({context})")
                },
                escape_terminal(&diagnostic.message)
            ))
        }));
    }
    lines.push(Line::from("Original content:".bold()));
    lines.push(Line::from(safe_content(&entry.original_bytes)));
    lines
}

fn safe_content(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let escaped = escape_terminal(&text.chars().take(TEXT_LIMIT).collect::<String>());
            if text.chars().count() > TEXT_LIMIT {
                format!("{escaped}... [truncated at {TEXT_LIMIT} characters]")
            } else {
                escaped
            }
        }
        Err(_) => {
            let preview = bytes
                .iter()
                .take(BINARY_LIMIT)
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            if bytes.len() > BINARY_LIMIT {
                format!("{preview} ... [truncated at {BINARY_LIMIT} bytes]")
            } else {
                preview
            }
        }
    }
}

fn escape_terminal(text: &str) -> String {
    text.chars().flat_map(char::escape_default).collect()
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
        Classification::Governed => Color::Green,
        Classification::Ungoverned => Color::Yellow,
        Classification::Invalid => Color::Red,
        Classification::Raw => Color::Magenta,
    })
}

fn kind_name(kind: casefile_core::Kind) -> &'static str {
    match kind {
        casefile_core::Kind::Activation => "activation",
        casefile_core::Kind::ProjectMap => "project_map",
        casefile_core::Kind::Request => "request",
        casefile_core::Kind::Decision => "decision",
        casefile_core::Kind::Evidence => "evidence",
        casefile_core::Kind::Review => "review",
        casefile_core::Kind::Plan => "plan",
        casefile_core::Kind::Closeout => "closeout",
        casefile_core::Kind::Strategy => "strategy",
        casefile_core::Kind::Ticket => "ticket",
        casefile_core::Kind::Epic => "epic",
        casefile_core::Kind::Board => "board",
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

#[cfg(test)]
mod tests {
    use super::*;
    use casefile_core::{CasefileSnapshot, Diagnostic, Kind, Revision};
    use casefile_store::ActivationState;
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
                        b"safe\ntext",
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
                        b"\x1b[31mnot a colour",
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

    fn render(app: &App) -> String {
        render_with_size(app, 110, 30)
    }

    fn render_with_size(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn record_metadata_and_diagnostics_escape_terminal_controls() {
        let path = "\x1b]0;path\x07-ticket.md";
        let metadata = "\x1b]0;metadata\x07";
        let scan = ScanResult {
            activation: ActivationState::Active,
            snapshot: CasefileSnapshot {
                revision: Revision("sha256:controls".into()),
                entries: vec![
                    entry(
                        path,
                        Classification::Governed,
                        Some(Kind::Ticket),
                        Some(RecordSummary::WorkItem {
                            id: format!("HMD-{metadata}"),
                            title: format!("title-{metadata}"),
                            status: format!("status-{metadata}"),
                            rank: None,
                        }),
                        b"content",
                    ),
                    entry(
                        "board.toml",
                        Classification::Governed,
                        Some(Kind::Board),
                        Some(RecordSummary::Board {
                            id: format!("board-{metadata}"),
                            title: format!("board-title-{metadata}"),
                            columns: vec![format!("column-{metadata}")],
                        }),
                        b"content",
                    ),
                ],
            },
            diagnostics: vec![
                Diagnostic::new(
                    path,
                    &format!("code-{metadata}"),
                    format!("message-{metadata}"),
                )
                .field(&format!("field-{metadata}"))
                .section(&format!("section-{metadata}")),
            ],
        };
        let mut app = App::new(scan);
        let output = render_with_size(&app, 240, 30);
        assert!(output.contains(r"\u{1b}]0;path\u{7}-ticket.md"));
        assert!(output.contains(r"HMD-\u{1b}]0;metadata\u{7}"));
        assert!(output.contains(r"field=field-\u{1b}]0;metadata\u{7}"));
        assert!(output.contains(r"section=section-\u{1b}]0;metadata\u{7}"));
        assert!(!output.chars().any(char::is_control));

        app.handle(KeyCode::Tab);
        app.handle(KeyCode::Down);
        let output = render_with_size(&app, 240, 30);
        assert!(output.contains(r"column-\u{1b}]0;metadata\u{7}"));
        assert!(!output.chars().any(char::is_control));
    }

    #[test]
    fn work_queue_renders_governed_work_items_and_navigation() {
        let mut app = App::new(scan());
        let output = render(&app);
        assert!(output.contains("HMD-013 | Navigator | accepted | #3"));
        assert!(!output.contains("c-legacy.txt"));
        assert_eq!(app.selected_path.as_deref(), Some("a-ticket.md"));
        app.handle(KeyCode::Down);
        assert_eq!(app.selected_path.as_deref(), Some("a-ticket.md"));
    }

    #[test]
    fn records_render_all_classifications_without_reclassification() {
        let mut app = App::new(scan());
        app.handle(KeyCode::Tab);
        let output = render(&app);
        for label in ["governed", "ungoverned", "invalid", "raw"] {
            assert!(output.contains(label), "missing {label}");
        }
        assert_eq!(app.selected_path.as_deref(), Some("a-ticket.md"));
        app.handle(KeyCode::Down);
        assert_eq!(app.selected_path.as_deref(), Some("b-board.toml"));
        let output = render(&app);
        assert!(output.contains("columns: Ready, Done"));
        assert!(output.contains(r"\u{1b}[31mnot a colour"));
    }

    #[test]
    fn filter_and_selection_are_keyed_by_path() {
        let mut app = App::new(scan());
        app.handle(KeyCode::Tab);
        app.handle(KeyCode::Down);
        assert_eq!(app.selected_path.as_deref(), Some("b-board.toml"));
        app.handle(KeyCode::Char('/'));
        for key in "board".chars().map(KeyCode::Char) {
            app.handle(key);
        }
        app.handle(KeyCode::Enter);
        assert_eq!(app.selected_path.as_deref(), Some("b-board.toml"));
        assert!(render(&app).contains("b-board.toml"));
        app.handle(KeyCode::Char('c'));
        assert_eq!(app.selected_path.as_deref(), Some("b-board.toml"));
    }

    #[test]
    fn detail_keeps_diagnostics_separate_and_binary_output_bounded() {
        let mut app = App::new(scan());
        app.handle(KeyCode::Tab);
        for _ in 0..3 {
            app.handle(KeyCode::Down);
        }
        let output = render(&app);
        assert!(output.contains("Classification: invalid"));
        assert!(output.contains("invalid_shape: ticket is incomplete"));
        assert!(output.contains("ff 00 10"));
        assert!(!output.contains("cross_record"));
        assert_eq!(
            safe_content(&vec![0xff; BINARY_LIMIT + 1]),
            format!(
                "{} ... [truncated at {BINARY_LIMIT} bytes]",
                vec!["ff"; BINARY_LIMIT].join(" ")
            )
        );
    }
}
