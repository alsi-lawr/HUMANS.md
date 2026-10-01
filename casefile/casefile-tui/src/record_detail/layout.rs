use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

pub(crate) struct Layout {
    width: u16,
    lines: Vec<(usize, Buffer)>,
    rows: usize,
    offsets: Vec<usize>,
}

impl Layout {
    pub(crate) fn new(lines: &[Line<'_>], width: u16) -> Self {
        let mut rendered = Vec::with_capacity(lines.len());
        let mut rows = 0usize;
        let mut offsets = Vec::with_capacity(lines.len() + 1);
        for line in lines {
            offsets.push(rows);
            let paragraph = Paragraph::new(Line {
                style: line.style,
                alignment: line.alignment,
                spans: line
                    .spans
                    .iter()
                    .map(|span| Span::styled(span.content.as_ref(), span.style))
                    .collect(),
            })
            .wrap(Wrap { trim: false });
            let height = paragraph.line_count(width).max(1);
            // Paragraph's native scroll/viewport coordinates are u16. Keep the second
            // segment too, so the last reachable viewport never loses its trailing rows.
            for start in [0, usize::from(u16::MAX)] {
                if start >= height {
                    break;
                }
                let visible = (height - start).min(usize::from(u16::MAX)) as u16;
                let area = Rect::new(0, 0, width, visible);
                let mut buffer = Buffer::empty(area);
                paragraph
                    .clone()
                    .scroll((start as u16, 0))
                    .render(area, &mut buffer);
                rendered.push((rows + start, buffer));
            }
            rows += height;
        }
        offsets.push(rows);
        Self {
            width,
            lines: rendered,
            rows: rows.max(1),
            offsets,
        }
    }

    pub(crate) fn width(&self) -> u16 {
        self.width
    }
    pub(crate) fn height(&self) -> u16 {
        self.rows.min(usize::from(u16::MAX)) as u16
    }

    pub(crate) fn line_range(&self, line: usize) -> std::ops::Range<usize> {
        self.offsets[line]..self.offsets[line + 1]
    }

    pub(crate) fn render(&self, scroll: u16, area: Rect, buffer: &mut Buffer) {
        let mut index = self
            .lines
            .partition_point(|(start, _)| *start <= usize::from(scroll))
            .saturating_sub(1);
        for row in 0..area.height {
            let source = usize::from(scroll) + usize::from(row);
            while self
                .lines
                .get(index + 1)
                .is_some_and(|(start, _)| *start <= source)
            {
                index += 1;
            }
            let Some((start, cached)) = self.lines.get(index) else {
                break;
            };
            let source_row = source.saturating_sub(*start);
            if source_row >= usize::from(cached.area.height) {
                continue;
            }
            for column in 0..area.width.min(self.width) {
                buffer[(area.x + column, area.y + row)] =
                    cached[(column, source_row as u16)].clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    #[test]
    fn scrolling_and_resize_preserve_supported_unicode_wrap_and_styles() {
        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "  wide \u{754c}\u{754c} words ",
                    Style::default().fg(Color::Green),
                ),
                Span::raw("e\u{301} emoji \u{1f980} tail"),
            ]),
            Line::from(""),
            Line::from("+ literal hunk with trailing spaces   "),
            Line::from(" short"),
        ];
        for width in [9, 23, 9] {
            let cached = Layout::new(&lines, width);
            for scroll in [0, 2, 7] {
                let area = Rect::new(0, 0, width, 5);
                let mut expected = Buffer::empty(area);
                Paragraph::new(lines.clone())
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0))
                    .render(area, &mut expected);
                let mut actual = Buffer::empty(area);
                cached.render(scroll, area, &mut actual);
                assert_eq!(actual, expected);
            }
        }
    }
}
