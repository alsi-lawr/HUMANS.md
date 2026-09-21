use super::{Flow, Stage, Treatment};
use crate::ui::MUTED;
use ratatui::{
    style::Style,
    text::{Line, Span},
};

const CANVAS_WIDTH: usize = 54;

pub(super) fn render(flow: &Flow, width: u16) -> Vec<Line<'static>> {
    let mut canvas = match flow.treatment {
        Treatment::Pipeline => pipeline(flow),
        Treatment::Dialogue => dialogue(flow),
        Treatment::Linear | Treatment::Review | Treatment::Correction => sequence(flow),
    };
    if flow.preflight {
        canvas.blank();
        canvas.push(
            10,
            if flow.treatment == Treatment::Pipeline {
                "During Implement N · optional"
            } else {
                "During Implement · optional"
            },
            ratatui::style::Color::White,
        );
        canvas.push(10, "Preflight N+1", ratatui::style::Color::White);
    }
    canvas.lines(width)
}

fn sequence(flow: &Flow) -> Canvas {
    let mut canvas = Canvas::new(flow.main.len() * 2 - 1);
    for (index, stage) in flow.main.iter().enumerate() {
        canvas.label(14, index * 2, stage.label());
        if index > 0 {
            canvas.put(14, index * 2 - 1, "▼");
        }
    }
    match flow.treatment {
        Treatment::Correction => {
            let from = (flow.main.len() - 2) * 2;
            canvas.put(20, 0, "◀──────────┐");
            for y in 1..from {
                canvas.put(31, y, "│");
            }
            canvas.put(18, from, "─────────────┘");
            canvas.colored(33, from / 2, "Fix", ratatui::style::Color::White);
        }
        Treatment::Review => revision(&mut canvas, (flow.main.len() - 2) * 2, 14),
        Treatment::Linear => {}
        Treatment::Pipeline | Treatment::Dialogue => {
            unreachable!("branched treatments have their own compact layout")
        }
    }
    canvas
}

fn pipeline(flow: &Flow) -> Canvas {
    let mut canvas = Canvas::new((flow.main.len() - 1) * 2 + 3);
    canvas.label(27, 0, "Implement N");
    canvas.put(14, 1, "┌────────────┴────────────┐");
    canvas.put(14, 2, "│");
    canvas.colored(33, 2, "if independent", ratatui::style::Color::White);
    canvas.put(14, 3, "▼");
    canvas.put(40, 3, "▼");
    canvas.label(40, 4, "Implement N+1");
    for (index, stage) in flow.main.iter().skip(1).enumerate() {
        let y = index * 2 + 4;
        canvas.label(14, y, &format!("{} N", stage.label()));
        if index > 0 {
            canvas.put(14, y - 1, "▼");
        }
    }
    let correction = (flow.main.len() - 3) * 2 + 4;
    canvas.put(2, 0, "┌─────────────────▶");
    for y in 1..correction {
        canvas.put(2, y, "│");
    }
    canvas.colored(4, 2, "Fix", ratatui::style::Color::White);
    canvas.put(2, correction, "└──────");
    canvas
}

fn dialogue(flow: &Flow) -> Canvas {
    let mut canvas = Canvas::new(9);
    canvas.label(14, 0, flow.main[0].label());
    canvas.put(18, 0, "──────────────┐");
    canvas.put(14, 1, "│");
    canvas.colored(34, 1, "spawn", ratatui::style::Color::White);
    canvas.put(32, 1, "▼");
    canvas.label(32, 2, "Challenger");
    canvas.put(14, 2, "│");
    canvas.put(14, 3, "└────────┬────────┘");
    canvas.put(23, 4, "▼");
    canvas.label(23, 5, Stage::Reconcile.label());
    canvas.put(23, 6, "▼");
    canvas.label(23, 7, Stage::Done.label());
    revision(&mut canvas, 5, 23);
    canvas.rows.pop();
    canvas
}

fn revision(canvas: &mut Canvas, y: usize, center: usize) {
    let start = center + 6;
    canvas.put(start, y, "── Fix ──▶");
    canvas.label(start + 14, y, "Revise");
}

struct Canvas {
    rows: Vec<Vec<(char, ratatui::style::Color)>>,
}

impl Canvas {
    fn new(height: usize) -> Self {
        Self {
            rows: vec![vec![(' ', MUTED); CANVAS_WIDTH]; height],
        }
    }
    fn blank(&mut self) {
        self.rows.push(vec![(' ', MUTED); CANVAS_WIDTH]);
    }
    fn push(&mut self, x: usize, text: &str, color: ratatui::style::Color) {
        self.blank();
        self.colored(x, self.rows.len() - 1, text, color);
    }
    fn put(&mut self, x: usize, y: usize, text: &str) {
        self.colored(x, y, text, MUTED);
    }
    fn label(&mut self, center: usize, y: usize, text: &str) {
        self.colored(
            center - text.len() / 2,
            y,
            text,
            ratatui::style::Color::White,
        );
    }
    fn colored(&mut self, x: usize, y: usize, text: &str, color: ratatui::style::Color) {
        for (offset, character) in text.chars().enumerate() {
            self.rows[y][x + offset] = (character, color);
        }
    }
    fn lines(self, width: u16) -> Vec<Line<'static>> {
        let first = self
            .rows
            .iter()
            .flat_map(|row| row.iter().position(|(c, _)| *c != ' '))
            .min()
            .unwrap_or(0);
        let last = self
            .rows
            .iter()
            .flat_map(|row| row.iter().rposition(|(c, _)| *c != ' '))
            .max()
            .unwrap_or(first);
        let width = usize::from(width);
        if width <= last - first {
            return vec![
                Line::from("Too narrow".chars().take(width).collect::<String>())
                    .style(Style::default().fg(MUTED)),
            ];
        }
        let (start, end, padding) = if width >= CANVAS_WIDTH {
            (0, CANVAS_WIDTH, (width - CANVAS_WIDTH) / 2)
        } else {
            (first, last + 1, (width - (last - first + 1)) / 2)
        };
        self.rows
            .into_iter()
            .map(|row| {
                let mut spans = vec![Span::raw(" ".repeat(padding))];
                for (character, color) in &row[start..end] {
                    let style = Style::default().fg(*color);
                    if let Some(previous) = spans.last_mut()
                        && previous.style == style
                    {
                        previous.content.to_mut().push(*character);
                    } else {
                        spans.push(Span::styled(character.to_string(), style));
                    }
                }
                Line::from(spans)
            })
            .collect()
    }
}
