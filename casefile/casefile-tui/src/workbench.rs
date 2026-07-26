use crate::{
    Interaction, PAGE_SIZE,
    browsing::{Browser, View},
    interaction::edit_selection,
    record_detail::RecordDetail,
    ui::{ACCENT, MUTED, WARN, safe_inline},
};
use casefile_store::{DerivedBoard, DerivedSnapshot, ScanResult};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};
use std::io::{self, Stdout};

const WIDE_MINIMUM: u16 = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayoutMode {
    Wide,
    Narrow,
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

pub(crate) struct App {
    scan: ScanResult,
    derived: DerivedSnapshot,
    browser: Browser,
    detail: RecordDetail,
    focus: Focus,
    show_help: bool,
    feedback: Option<String>,
    interaction: Option<Interaction>,
}

impl App {
    pub(crate) fn new(scan: ScanResult, derived: DerivedSnapshot) -> Self {
        let browser = Browser::new(&scan);
        Self {
            scan,
            derived,
            browser,
            detail: RecordDetail::new(),
            focus: Focus::List,
            show_help: false,
            feedback: None,
            interaction: None,
        }
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> io::Result<Interaction> {
        while self.interaction.is_none() {
            terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle(key.code);
            }
        }
        Ok(self.interaction.take().unwrap_or(Interaction::Quit))
    }

    fn handle(&mut self, key: KeyCode) {
        if self.show_help {
            match key {
                KeyCode::Char('q') => self.interaction = Some(Interaction::Quit),
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => self.show_help = false,
                _ => {}
            }
            return;
        }
        if self.browser.is_entering_filter() {
            let selection_changed = match key {
                KeyCode::Esc | KeyCode::Enter => {
                    self.browser.close_filter();
                    false
                }
                KeyCode::Backspace => self.browser.pop_filter(&self.scan),
                KeyCode::Char(character) => self.browser.push_filter(&self.scan, character),
                _ => false,
            };
            if selection_changed {
                self.detail.reset_scroll();
            }
            return;
        }
        self.feedback = None;
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.interaction = Some(Interaction::Quit),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('1') => self.set_view(View::Projects),
            KeyCode::Char('2') => self.set_view(View::Investigations),
            KeyCode::Char('3') => self.set_view(View::Tickets),
            KeyCode::Char('4') => self.set_view(View::Files),
            KeyCode::Char('5') => self.set_view(View::Strategies),
            KeyCode::Char('6') => self.set_view(View::Boards),
            KeyCode::Char('t') => {
                self.browser.cycle_view(&self.scan);
                if self.browser.view() == View::Boards {
                    self.browser
                        .select_board_offset(&self.board_card_paths(), 0);
                }
                self.detail.reset_scroll();
            }
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Enter => self.drill_down(),
            KeyCode::Backspace => self.go_up(),
            KeyCode::Char('/') => self.browser.start_filter(),
            KeyCode::Char('c') => self.clear_filter(),
            KeyCode::Left | KeyCode::Char('h') => self.detail.select_tab(-1),
            KeyCode::Right | KeyCode::Char('l') => self.detail.select_tab(1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focus(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focus(-1),
            KeyCode::PageDown => self.move_focus(PAGE_SIZE),
            KeyCode::PageUp => self.move_focus(-PAGE_SIZE),
            KeyCode::Home => self.move_to_edge(false),
            KeyCode::End => self.move_to_edge(true),
            KeyCode::Char('e') => self.request_edit(),
            _ => {}
        }
    }

    fn set_view(&mut self, view: View) {
        self.browser.set_view(&self.scan, view);
        if view == View::Boards {
            self.browser
                .select_board_offset(&self.board_card_paths(), 0);
        }
        self.detail.reset_scroll();
    }

    fn clear_filter(&mut self) {
        if self.browser.clear_filter(&self.scan) {
            self.detail.reset_scroll();
        }
    }

    fn drill_down(&mut self) {
        if self.focus == Focus::List && self.browser.drill_down(&self.scan) {
            self.detail.reset_scroll();
        }
    }

    fn go_up(&mut self) {
        if self.focus == Focus::List && self.browser.go_up(&self.scan) {
            self.detail.reset_scroll();
        }
    }

    fn move_focus(&mut self, offset: isize) {
        match self.focus {
            Focus::List => {
                let changed = if self.browser.view() == View::Boards {
                    self.browser
                        .select_board_offset(&self.board_card_paths(), offset)
                } else {
                    self.browser.select_offset(&self.scan, offset)
                };
                if changed {
                    self.detail.reset_scroll();
                }
            }
            Focus::Detail => self.detail.scroll(offset),
        }
    }

    fn move_to_edge(&mut self, end: bool) {
        match self.focus {
            Focus::List => {
                let changed = if self.browser.view() == View::Boards {
                    self.browser.select_board_offset(
                        &self.board_card_paths(),
                        if end { isize::MAX } else { isize::MIN },
                    )
                } else {
                    self.browser.select_edge(&self.scan, end)
                };
                if changed {
                    self.detail.reset_scroll();
                }
            }
            Focus::Detail => self.detail.move_to_edge(end),
        }
    }

    fn request_edit(&mut self) {
        if self.browser.view() == View::Boards {
            self.feedback =
                Some("Read-only: Boards do not change ticket progress or placement.".into());
            return;
        }
        match edit_selection(self.browser.selected(&self.scan)) {
            Ok(interaction) => self.interaction = Some(interaction),
            Err(feedback) => self.feedback = Some(feedback.into()),
        }
    }

    pub(crate) fn render(&self, area: Rect, buffer: &mut Buffer) {
        let [header, body, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .areas(area);
        self.browser
            .render_header(&self.scan, self.board_count(), header, buffer);

        match layout_mode(body) {
            LayoutMode::Wide => {
                let [list, detail] = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(64), Constraint::Min(32)])
                    .areas(body);
                self.render_body(list, detail, buffer);
            }
            LayoutMode::Narrow => {
                let [list, detail] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                    .areas(body);
                self.render_body(list, detail, buffer);
            }
        }

        let footer_text = if self.browser.is_entering_filter() {
            " Type to filter  Enter accept  Esc close "
        } else if let Some(feedback) = &self.feedback {
            feedback
        } else {
            " 1-6 tabs  Enter drill down  Backspace up  Tab focus  j/k move  h/l detail  e edit  / filter  ? help  q quit "
        };
        Paragraph::new(footer_text)
            .style(Style::default().fg(MUTED))
            .render(footer, buffer);

        if self.show_help {
            render_help(area, buffer);
        }
    }

    fn render_body(&self, list: Rect, detail: Rect, buffer: &mut Buffer) {
        let selected = self.browser.selected(&self.scan);
        let derived = if self.derived.source_revision == self.scan.snapshot.revision {
            selected.and_then(|entry| {
                self.derived
                    .records
                    .iter()
                    .find(|record| record.path == entry.path)
            })
        } else {
            None
        };
        if self.browser.view() == View::Boards {
            self.render_boards(list, buffer);
        } else {
            self.browser
                .render_list(&self.scan, self.focus == Focus::List, list, buffer);
        }
        self.detail.render(
            selected,
            derived,
            &self.scan.diagnostics,
            self.focus == Focus::Detail,
            detail,
            buffer,
        );
    }

    fn board_card_paths(&self) -> Vec<String> {
        if self.derived.source_revision != self.scan.snapshot.revision {
            return Vec::new();
        }
        let Some((project, investigation)) = self.browser.scope() else {
            return Vec::new();
        };
        self.derived
            .boards
            .iter()
            .filter(|board| board_matches_scope(board, project, investigation))
            .flat_map(|board| board.columns.iter())
            .flat_map(|column| column.cards.iter())
            .filter_map(|card| match canonical_card_path(&self.scan, card) {
                CardPathResolution::Resolved(path) => Some(path),
                CardPathResolution::Missing | CardPathResolution::Ambiguous => None,
            })
            .collect()
    }

    fn board_count(&self) -> usize {
        if self.derived.source_revision != self.scan.snapshot.revision {
            return 0;
        }
        let Some((project, investigation)) = self.browser.scope() else {
            return 0;
        };
        self.derived
            .boards
            .iter()
            .filter(|board| board_matches_scope(board, project, investigation))
            .count()
    }

    fn render_boards(&self, area: Rect, buffer: &mut Buffer) {
        let block = crate::ui::panel(" Boards ", self.focus == Focus::List);
        if self.derived.source_revision != self.scan.snapshot.revision {
            return Paragraph::new(
                "Board projection is stale. Refresh to load the current investigation.",
            )
            .style(Style::default().fg(WARN))
            .block(block)
            .wrap(Wrap { trim: false })
            .render(area, buffer);
        }
        let Some((project, investigation)) = self.browser.scope() else {
            return Paragraph::new("Select an investigation to inspect its boards.")
                .style(Style::default().fg(MUTED))
                .block(block)
                .render(area, buffer);
        };
        let invalid_diagnostics = scoped_board_diagnostics(&self.scan, project, investigation);
        if !invalid_diagnostics.is_empty() {
            let mut lines = vec![
                Line::from("Board definitions or the progress log are invalid.")
                    .style(Style::default().fg(WARN).bold()),
                Line::from("Inspect Files or Diagnostics for the canonical validation details.")
                    .style(Style::default().fg(MUTED)),
            ];
            for diagnostic in invalid_diagnostics {
                lines.push(
                    Line::from(format!(
                        "{}: {}",
                        safe_inline(&diagnostic.code),
                        safe_inline(&diagnostic.message),
                    ))
                    .style(Style::default().fg(WARN)),
                );
            }
            return Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, buffer);
        }
        let boards = self
            .derived
            .boards
            .iter()
            .filter(|board| board_matches_scope(board, project, investigation))
            .collect::<Vec<_>>();
        if boards.is_empty() {
            return Paragraph::new("This investigation has no board definitions.")
                .style(Style::default().fg(MUTED))
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, buffer);
        }
        let mut lines = vec![
            Line::from("Read-only; record filter does not alter cards.")
                .style(Style::default().fg(MUTED)),
        ];
        for board in boards {
            lines.push(Line::from(""));
            lines.push(
                Line::from(format!(
                    "{}  [{:?}]",
                    safe_inline(&board.title),
                    board.status_source
                ))
                .style(Style::default().fg(ACCENT).bold()),
            );
            board_lines(
                board,
                &self.scan,
                self.browser
                    .selected(&self.scan)
                    .map(|entry| entry.path.as_str()),
                &mut lines,
            );
        }
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .render(area, buffer);
    }
}

fn board_matches_scope(board: &DerivedBoard, project: &str, investigation: &str) -> bool {
    board.identity.scope.project == project
        && board.identity.scope.investigation.as_deref() == Some(investigation)
}

enum CardPathResolution {
    Resolved(String),
    Missing,
    Ambiguous,
}

fn canonical_card_path(
    scan: &ScanResult,
    card: &casefile_store::DerivedCard,
) -> CardPathResolution {
    let matches = scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| {
            entry.identity.as_deref() == Some(card.identity.identity.as_str())
                && scan.scope_for_path(&entry.path).is_some_and(|scope| {
                    scope.0 == card.identity.scope.project
                        && scope.1 == card.identity.scope.investigation.as_deref()
                })
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [entry] => CardPathResolution::Resolved(entry.path.clone()),
        [] => CardPathResolution::Missing,
        _ => CardPathResolution::Ambiguous,
    }
}

fn scoped_board_diagnostics<'a>(
    scan: &'a ScanResult,
    project: &str,
    investigation: &str,
) -> Vec<&'a casefile_core::Diagnostic> {
    let prefix = format!("projects/{project}/investigations/{investigation}/");
    scan.diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.path.starts_with(&format!("{prefix}boards/"))
                || diagnostic.path == format!("{prefix}progress/log.toml")
        })
        .collect()
}

fn board_lines(
    board: &DerivedBoard,
    scan: &ScanResult,
    selected_path: Option<&str>,
    lines: &mut Vec<Line<'static>>,
) {
    for column in &board.columns {
        lines.push(
            Line::from(format!(
                "  {} ({})",
                safe_inline(&column.name),
                column.cards.len()
            ))
            .style(Style::default().fg(ACCENT)),
        );
        if column.cards.is_empty() {
            lines.push(Line::from("    No cards.").style(Style::default().fg(MUTED)));
            continue;
        }
        for card in &column.cards {
            let resolution = canonical_card_path(scan, card);
            let selected = matches!(&resolution, CardPathResolution::Resolved(path) if Some(path.as_str()) == selected_path);
            let (marker, suffix) = match &resolution {
                CardPathResolution::Resolved(_) if selected => (">", "  [selected]"),
                CardPathResolution::Resolved(_) => (" ", ""),
                CardPathResolution::Missing => ("!", "  [detail unavailable: missing identity]"),
                CardPathResolution::Ambiguous => {
                    ("!", "  [detail unavailable: ambiguous identity]")
                }
            };
            lines.push(
                Line::from(format!(
                    "  {marker} {}  {}  {}{}",
                    safe_inline(&card.identity.identity),
                    safe_inline(&card.status),
                    safe_inline(&card.title),
                    suffix,
                ))
                .style(if selected {
                    Style::default().fg(ACCENT).bold()
                } else if !matches!(resolution, CardPathResolution::Resolved(_)) {
                    Style::default().fg(WARN)
                } else {
                    Style::default()
                }),
            );
        }
    }
}

fn render_help(area: Rect, buffer: &mut Buffer) {
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
        help_line(
            "1 / 2 / 3 / 4 / 5 / 6",
            "Open Projects, Investigations, Tickets, Files, Strategies, or Boards",
        ),
        help_line(
            "Enter / Backspace",
            "Drill into the selected scope or go up",
        ),
        help_line(
            "h / l, Left / Right",
            "Switch Overview, Rendered, Source, Diagnostics",
        ),
        help_line("/", "Enter filter mode"),
        help_line("c", "Clear the active filter"),
        help_line("e", "Edit selected governed ticket, epic, or board"),
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

fn layout_mode(area: Rect) -> LayoutMode {
    if area.width >= WIDE_MINIMUM {
        LayoutMode::Wide
    } else {
        LayoutMode::Narrow
    }
}

#[cfg(test)]
mod tests;
