use crate::{
    Interaction, PAGE_SIZE,
    browsing::{Browser, View},
    interaction::edit_selection,
    record_detail::RecordDetail,
    ui::{ACCENT, MUTED, WARN},
};
use casefile_store::ScanResult;
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
    browser: Browser,
    detail: RecordDetail,
    focus: Focus,
    show_help: bool,
    feedback: Option<String>,
    interaction: Option<Interaction>,
}

impl App {
    pub(crate) fn new(scan: ScanResult) -> Self {
        let browser = Browser::new(&scan);
        Self {
            scan,
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
            KeyCode::Char('t') => {
                self.browser.cycle_view(&self.scan);
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
                if self.browser.select_offset(&self.scan, offset) {
                    self.detail.reset_scroll();
                }
            }
            Focus::Detail => self.detail.scroll(offset),
        }
    }

    fn move_to_edge(&mut self, end: bool) {
        match self.focus {
            Focus::List => {
                if self.browser.select_edge(&self.scan, end) {
                    self.detail.reset_scroll();
                }
            }
            Focus::Detail => self.detail.move_to_edge(end),
        }
    }

    fn request_edit(&mut self) {
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
        self.browser.render_header(&self.scan, header, buffer);

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
            " 1-4 tabs  Enter drill down  Backspace up  Tab focus  j/k move  h/l detail  e edit  / filter  ? help  q quit "
        };
        Paragraph::new(footer_text)
            .style(Style::default().fg(MUTED))
            .render(footer, buffer);

        if self.show_help {
            render_help(area, buffer);
        }
    }

    fn render_body(&self, list: Rect, detail: Rect, buffer: &mut Buffer) {
        self.browser
            .render_list(&self.scan, self.focus == Focus::List, list, buffer);
        self.detail.render(
            self.browser.selected(&self.scan),
            &self.scan.diagnostics,
            self.focus == Focus::Detail,
            detail,
            buffer,
        );
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
            "1 / 2 / 3 / 4",
            "Open Projects, Investigations, Tickets, or Files",
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
