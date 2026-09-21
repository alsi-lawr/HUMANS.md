use super::Runtime;
use crate::ui::{MUTED, WARN, safe_inline};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

pub(super) const WIDTH: u16 = 54;
const BOX_WIDTH: u16 = 22;

pub(super) struct Chart {
    pub(super) nodes: Vec<NodeBox>,
    marks: Vec<(u16, u16, String, Color)>,
    pub(super) height: u16,
}

pub(super) struct NodeBox {
    pub(super) area: Rect,
    label: String,
    model: Option<(String, Color)>,
}

impl Chart {
    pub(super) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            marks: Vec::new(),
            height: 0,
        }
    }
    pub(super) fn node(&mut self, x: u16, y: u16, label: &str, model: Option<&Runtime>) -> Rect {
        let model = model.map(|value| {
            (
                safe_inline(value.text()),
                if matches!(value, Runtime::Model(_)) {
                    Color::White
                } else {
                    WARN
                },
            )
        });
        let model_height = model.as_ref().map_or(0, |(text, _)| {
            Paragraph::new(text.as_str())
                .wrap(Wrap { trim: false })
                .line_count(BOX_WIDTH - 2) as u16
        });
        let area = Rect::new(x, y, BOX_WIDTH, 3 + model_height);
        self.height = self.height.max(area.bottom());
        self.nodes.push(NodeBox {
            area,
            label: label.into(),
            model,
        });
        area
    }
    pub(super) fn put(&mut self, x: u16, y: u16, text: impl Into<String>) {
        self.mark(x, y, text, MUTED);
    }
    pub(super) fn text(&mut self, x: u16, y: u16, text: impl Into<String>) {
        self.mark(x, y, text, Color::White);
    }
    fn mark(&mut self, x: u16, y: u16, text: impl Into<String>, color: Color) {
        self.height = self.height.max(y + 1);
        self.marks.push((x, y, text.into(), color));
    }
    pub(super) fn vertical(&mut self, x: u16, start: u16, end: u16) {
        for y in start..end {
            self.put(x, y, "│");
        }
    }
    pub(super) fn down(&mut self, area: Rect) {
        let x = area.x + area.width / 2;
        self.put(x, area.bottom() - 1, "┬");
        self.put(x, area.bottom(), "▼");
    }
    pub(super) fn buffer(&self) -> Buffer {
        let mut buffer = Buffer::empty(Rect::new(0, 0, WIDTH, self.height));
        for node in &self.nodes {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(MUTED));
            let inner = block.inner(node.area);
            block.render(node.area, &mut buffer);
            centered(
                &node.label,
                Color::White,
                Rect::new(inner.x, inner.y, inner.width, 1),
                &mut buffer,
            );
            if let Some((text, color)) = &node.model {
                centered(
                    text,
                    *color,
                    Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1),
                    &mut buffer,
                );
            }
        }
        for (x, y, text, color) in &self.marks {
            buffer.set_string(*x, *y, text, Style::default().fg(*color));
        }
        buffer
    }
    pub(super) fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let buffer = self.buffer();
        let first = (0..WIDTH)
            .find(|x| (0..self.height).any(|y| buffer[(*x, y)].symbol() != " "))
            .unwrap_or(0);
        let last = (0..WIDTH)
            .rfind(|x| (0..self.height).any(|y| buffer[(*x, y)].symbol() != " "))
            .unwrap_or(first);
        if width <= last - first {
            return vec![
                Line::from(
                    "Too narrow"
                        .chars()
                        .take(usize::from(width))
                        .collect::<String>(),
                )
                .style(Style::default().fg(MUTED)),
            ];
        }
        let (start, end, padding) = if width >= WIDTH {
            (0, WIDTH, (width - WIDTH) / 2)
        } else {
            (first, last + 1, (width - (last - first + 1)) / 2)
        };
        (0..self.height)
            .map(|y| {
                let mut spans = vec![Span::raw(" ".repeat(usize::from(padding)))];
                let mut x = start;
                while x < end {
                    let cell = &buffer[(x, y)];
                    let span = Span::styled(cell.symbol().to_owned(), cell.style());
                    x += span.width().max(1) as u16;
                    if let Some(previous) = spans.last_mut()
                        && previous.style == span.style
                    {
                        previous.content.to_mut().push_str(&span.content);
                    } else {
                        spans.push(span);
                    }
                }
                Line::from(spans)
            })
            .collect()
    }
}

fn centered(text: &str, color: Color, area: Rect, target: &mut Buffer) {
    let mut wrapped = Buffer::empty(Rect::new(0, 0, area.width, area.height));
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(color))
        .render(wrapped.area, &mut wrapped);
    for y in 0..area.height {
        let used = (0..area.width)
            .rfind(|x| wrapped[(*x, y)].symbol() != " ")
            .map_or(0, |x| {
                x + Span::raw(wrapped[(x, y)].symbol()).width() as u16
            });
        let offset = (area.width - used) / 2;
        for x in 0..used {
            target[(area.x + offset + x, area.y + y)] = wrapped[(x, y)].clone();
        }
    }
}
