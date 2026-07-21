use crate::{
    PAGE_SIZE,
    ui::{ACCENT, BAD, BORDER, GOOD, MUTED, panel, safe_inline, safe_multiline},
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use std::{
    cell::Cell,
    io::{self, Stdout},
};

/// A decision made after inspecting a proposed Store diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewDecision {
    Apply,
    Cancel,
}

pub(crate) struct ReviewApp {
    diff: String,
    scroll: u16,
    rows: Cell<u16>,
    decision: Option<ReviewDecision>,
}

impl ReviewApp {
    pub(crate) fn new(diff: &str) -> Self {
        Self {
            diff: diff.into(),
            scroll: 0,
            rows: Cell::new(1),
            decision: None,
        }
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> io::Result<ReviewDecision> {
        while self.decision.is_none() {
            terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle(key.code);
            }
        }
        Ok(self.decision.unwrap_or(ReviewDecision::Cancel))
    }

    fn handle(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('a') => self.decision = Some(ReviewDecision::Apply),
            KeyCode::Char('c') | KeyCode::Esc => self.decision = Some(ReviewDecision::Cancel),
            KeyCode::Down | KeyCode::Char('j') => self.scroll(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll(-1),
            KeyCode::PageDown => self.scroll(PAGE_SIZE),
            KeyCode::PageUp => self.scroll(-PAGE_SIZE),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll(),
            _ => {}
        }
    }

    fn scroll(&mut self, amount: isize) {
        self.scroll = (self.scroll as isize + amount).clamp(0, self.max_scroll() as isize) as u16;
    }

    fn max_scroll(&self) -> u16 {
        self.rows.get().saturating_sub(1)
    }

    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let [header, content, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .areas(area);
        Paragraph::new(Line::from(vec![
            Span::styled(" REVIEW CHANGES ", Style::default().fg(ACCENT).bold()),
            Span::styled(
                "Store preview; canonical files are unchanged",
                Style::default().fg(MUTED),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(BORDER)),
        )
        .render(header, buffer);

        let paragraph = Paragraph::new(diff_lines(&self.diff))
            .block(panel(" Changes ", true))
            .wrap(Wrap { trim: false });
        let inner = panel("", true).inner(content);
        let rows = paragraph
            .line_count(inner.width)
            .max(1)
            .min(usize::from(u16::MAX)) as u16;
        self.rows.set(rows);
        let scroll = self.scroll.min(rows.saturating_sub(1));
        paragraph.scroll((scroll, 0)).render(content, buffer);
        Paragraph::new(format!(
            " line {}/{}  j/k scroll  PgUp/PgDn page  Home/End edge  a Apply  c Cancel ",
            scroll.saturating_add(1),
            rows
        ))
        .style(Style::default().fg(MUTED))
        .render(footer, buffer);
    }
}

fn diff_lines(diff: &str) -> Vec<Line<'static>> {
    let (safe, _) = safe_multiline(diff, usize::MAX);
    let mut lines: Vec<_> = safe
        .split('\n')
        .map(|line| Line::from(Span::styled(safe_inline(line), diff_style(line))))
        .collect();
    if lines.is_empty() {
        lines.push(Line::from("No Store diff was produced.").style(Style::default().fg(MUTED)));
    }
    lines
}

fn diff_style(line: &str) -> Style {
    if line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("diff --git")
        || line.starts_with("@@")
    {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else if line.starts_with('+') {
        Style::default().fg(GOOD)
    } else if line.starts_with('-') {
        Style::default().fg(BAD)
    } else {
        Style::default().fg(Color::White)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn review_scrolls_store_diff_and_colours_additions_and_removals() {
        let diff = format!(
            "diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n@@ -1 +1 @@\n-old\n+new\n{}",
            "+more\n".repeat(200)
        );
        let mut app = ReviewApp::new(&diff);
        let backend = TestBackend::new(70, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
            .expect("draw");
        app.handle(KeyCode::PageDown);
        assert!(app.scroll > 0);
        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.symbol() == "+" && cell.fg == GOOD)
        );
        assert_eq!(diff_style("-old").fg, Some(BAD));
        assert_eq!(diff_style("+new").fg, Some(GOOD));
        assert_eq!(diff_style("@@ -1 +1 @@").fg, Some(ACCENT));
    }
}
