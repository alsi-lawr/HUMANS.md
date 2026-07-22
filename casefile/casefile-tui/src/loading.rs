use crate::ui::ACCENT;
use casefile_store::{DerivedSnapshot, ScanResult};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
};
use std::{
    io::{self, Stdout},
    sync::mpsc::{Receiver, TryRecvError},
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) enum Outcome {
    Ready(ScanResult, DerivedSnapshot),
    Quit,
    Failed(String),
}

struct App {
    receiver: Receiver<Result<(ScanResult, DerivedSnapshot), String>>,
}

impl App {
    fn new(receiver: Receiver<Result<(ScanResult, DerivedSnapshot), String>>) -> Self {
        Self { receiver }
    }

    fn receive(&self) -> Option<Outcome> {
        match self.receiver.try_recv() {
            Ok(Ok((scan, derived))) => Some(Outcome::Ready(scan, derived)),
            Ok(Err(message)) => Some(Outcome::Failed(message)),
            Err(TryRecvError::Disconnected) => Some(Outcome::Failed(
                "Casefile background loading stopped unexpectedly".into(),
            )),
            Err(TryRecvError::Empty) => None,
        }
    }

    fn handle(&self, event: Event) -> Option<Outcome> {
        match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) =>
            {
                Some(Outcome::Quit)
            }
            _ => None,
        }
    }

    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let [_, panel, _] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(7),
                Constraint::Fill(1),
            ])
            .areas(area);
        let [_, panel, _] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Percentage(70),
                Constraint::Fill(1),
            ])
            .areas(panel);
        Paragraph::new(vec![
            Line::from("Loading Casefile...").style(Style::default().fg(ACCENT).bold()),
            Line::from(""),
            Line::from("Scanning and deriving the planning root in the background."),
            Line::from("Press q or Esc to cancel."),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" CASEFILE ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT)),
        )
        .wrap(Wrap { trim: false })
        .render(panel, buffer);
    }
}

pub(crate) fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    receiver: Receiver<Result<(ScanResult, DerivedSnapshot), String>>,
) -> io::Result<Outcome> {
    let app = App::new(receiver);
    terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
    loop {
        if let Some(outcome) = app.receive() {
            return Ok(outcome);
        }
        if event::poll(POLL_INTERVAL)? {
            let event = event::read()?;
            if matches!(event, Event::Resize(_, _)) {
                terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
            }
            if let Some(outcome) = app.handle(event) {
                return Ok(outcome);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use std::sync::mpsc;

    fn text(buffer: &Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn loading_state_is_visible_and_cancellable() {
        let (_sender, receiver) = mpsc::sync_channel(1);
        let app = App::new(receiver);
        let area = Rect::new(0, 0, 100, 24);
        let mut buffer = Buffer::empty(area);

        app.render(area, &mut buffer);

        let content = text(&buffer);
        assert!(content.contains("Loading Casefile..."));
        assert!(content.contains("background"));
        assert!(matches!(
            app.handle(Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char('q'),
                crossterm::event::KeyModifiers::NONE,
            ))),
            Some(Outcome::Quit)
        ));
    }

    #[test]
    fn loader_failure_is_reported() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(Err("scan failed".into())).expect("send");
        let app = App::new(receiver);

        assert!(matches!(
            app.receive(),
            Some(Outcome::Failed(message)) if message == "scan failed"
        ));
    }
}
