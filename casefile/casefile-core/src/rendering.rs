use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

pub fn render_markdown_html(markdown: &str) -> String {
    let events = Parser::new_ext(markdown, Options::all()).map(safe_event);
    let mut output = String::new();
    html::push_html(&mut output, events);
    output
}

fn safe_event(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        }),
        other => other,
    }
}

fn safe_destination(destination: CowStr<'_>) -> CowStr<'_> {
    let value = destination.as_ref();
    let lower = value.to_ascii_lowercase();
    let allowed = !value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
        && (value.starts_with('#')
            || value.starts_with('/')
            || value.starts_with("./")
            || value.starts_with("../")
            || ["http://", "https://", "mailto:"]
                .iter()
                .any(|prefix| lower.starts_with(prefix)));
    if allowed {
        destination
    } else {
        CowStr::from("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_common_markdown_without_trusting_embedded_html_or_script_urls() {
        let rendered = render_markdown_html(
            "# Title\n\n- **bold**\n- `code`\n\n| Name | Value |\n| --- | --- |\n| safe | yes |\n\n<script>alert(1)</script>\n\n[bad](JaVaScRiPt:alert(1)) [good](https://example.com)",
        );

        assert!(rendered.contains("<h1>Title</h1>"));
        assert!(rendered.contains("<strong>bold</strong>"));
        assert!(rendered.contains("<code>code</code>"));
        assert!(rendered.contains("<table>"));
        assert!(rendered.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!rendered.to_ascii_lowercase().contains("javascript:"));
        assert!(rendered.contains("href=\"https://example.com\""));
    }
}
