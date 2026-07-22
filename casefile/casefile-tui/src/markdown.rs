use crate::ui::{ACCENT, BORDER, MUTED, WARN, safe_inline};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(crate) fn render(markdown: &str) -> Vec<Line<'static>> {
    let mut renderer = Renderer::default();
    for event in Parser::new_ext(markdown, Options::all()) {
        renderer.event(event);
    }
    renderer.finish()
}

#[derive(Default)]
struct Renderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    styles: Vec<Style>,
    lists: Vec<Option<u64>>,
    item_prefix: Option<String>,
    quote_depth: usize,
    code_block: bool,
    metadata: bool,
}

impl Renderer {
    fn event(&mut self, event: Event<'_>) {
        if self.metadata {
            if matches!(event, Event::End(TagEnd::MetadataBlock(_))) {
                self.metadata = false;
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(value) => self.text(&value, self.current_style()),
            Event::Code(value) => self.text(
                &value,
                self.current_style().fg(WARN).bg(Color::Rgb(30, 41, 59)),
            ),
            Event::InlineMath(value) | Event::DisplayMath(value) => {
                self.text(&value, self.current_style().fg(WARN))
            }
            Event::Html(value) | Event::InlineHtml(value) => {
                self.text(&value, self.current_style().fg(MUTED))
            }
            Event::FootnoteReference(value) => self.text(
                &format!("[{}]", safe_inline(&value)),
                self.current_style().fg(ACCENT),
            ),
            Event::SoftBreak => self.text(" ", self.current_style()),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.lines
                    .push(Line::from("─".repeat(32)).style(Style::default().fg(BORDER)));
                self.blank();
            }
            Event::TaskListMarker(checked) => {
                self.text(
                    if checked { "[x] " } else { "[ ] " },
                    self.current_style().fg(MUTED),
                );
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush();
                self.styles.push(heading_style(level));
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code_block = true;
            }
            Tag::List(first) => {
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush();
                self.item_prefix = Some(match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let prefix = format!("{number}. ");
                        *number += 1;
                        prefix
                    }
                    _ => "• ".into(),
                });
            }
            Tag::Emphasis => self.styles.push(self.current_style().italic()),
            Tag::Strong => self.styles.push(self.current_style().bold()),
            Tag::Strikethrough => self
                .styles
                .push(self.current_style().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { .. } => self.styles.push(
                self.current_style()
                    .fg(ACCENT)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Tag::Image { .. } => self.styles.push(self.current_style().fg(ACCENT)),
            Tag::Table(_) | Tag::TableHead | Tag::TableRow => self.flush(),
            Tag::TableCell => self.text("│ ", self.current_style().fg(BORDER)),
            Tag::MetadataBlock(_) => self.metadata = true,
            Tag::HtmlBlock
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Superscript
            | Tag::Subscript => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => {
                if matches!(tag, TagEnd::Heading(_)) {
                    self.styles.pop();
                }
                self.flush();
                self.blank();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.blank();
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.code_block = false;
                self.blank();
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                self.blank();
            }
            TagEnd::Item | TagEnd::TableRow => self.flush(),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Link
            | TagEnd::Image => {
                self.styles.pop();
            }
            TagEnd::Table | TagEnd::TableHead => {
                self.flush();
                self.blank();
            }
            TagEnd::MetadataBlock(_) => self.metadata = false,
            TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::TableCell
            | TagEnd::Superscript
            | TagEnd::Subscript => {}
        }
    }

    fn text(&mut self, value: &str, style: Style) {
        self.prefix();
        for (index, part) in value.split('\n').enumerate() {
            if index > 0 {
                self.flush();
                self.prefix();
            }
            if !part.is_empty() {
                self.spans.push(Span::styled(safe_inline(part), style));
            }
        }
    }

    fn prefix(&mut self) {
        if !self.spans.is_empty() {
            return;
        }
        let indent = "  ".repeat(self.lists.len().saturating_sub(1));
        if !indent.is_empty() {
            self.spans.push(Span::raw(indent));
        }
        for _ in 0..self.quote_depth {
            self.spans
                .push(Span::styled("│ ", Style::default().fg(BORDER)));
        }
        if let Some(prefix) = self.item_prefix.take() {
            self.spans
                .push(Span::styled(prefix, Style::default().fg(ACCENT)));
        }
    }

    fn current_style(&self) -> Style {
        self.styles.last().copied().unwrap_or_else(|| {
            if self.code_block {
                Style::default().fg(WARN).bg(Color::Rgb(15, 23, 42))
            } else {
                Style::default()
            }
        })
    }

    fn flush(&mut self) {
        if !self.spans.is_empty() {
            self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        }
    }

    fn blank(&mut self) {
        if self.lines.last().is_some_and(|line| !line.spans.is_empty()) {
            self.lines.push(Line::from(""));
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush();
        while self.lines.last().is_some_and(|line| line.spans.is_empty()) {
            self.lines.pop();
        }
        if self.lines.is_empty() {
            vec![Line::from("Empty Markdown record.").style(Style::default().fg(MUTED))]
        } else {
            self.lines
        }
    }
}

fn heading_style(level: HeadingLevel) -> Style {
    match level {
        HeadingLevel::H1 => Style::default().fg(ACCENT).bold(),
        HeadingLevel::H2 => Style::default().fg(Color::White).bold(),
        HeadingLevel::H3 => Style::default().fg(WARN).bold(),
        HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => Style::default().fg(Color::White),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_lists_code_and_terminal_safe_html() {
        let lines = render("# Title\n\n- **bold** and `code`\n\n<script>bad</script>");
        let text = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("Title"));
        assert!(text.contains("• "));
        assert!(text.contains("bold"));
        assert!(text.contains("code"));
        assert!(text.contains("<script>bad</script>"));
        assert!(!text.contains("# Title"));
    }
}
