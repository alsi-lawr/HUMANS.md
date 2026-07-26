use casefile_core::{Classification, Kind, RecordSummary};
use ratatui::{
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders},
};

pub(crate) const ACCENT: Color = Color::Rgb(91, 192, 235);
pub(crate) const MUTED: Color = Color::Rgb(118, 126, 138);
pub(crate) const BORDER: Color = Color::Rgb(68, 75, 86);
pub(crate) const SELECTED: Color = Color::Rgb(37, 52, 67);
pub(crate) const GOOD: Color = Color::Rgb(117, 190, 96);
pub(crate) const WARN: Color = Color::Rgb(229, 192, 123);
pub(crate) const BAD: Color = Color::Rgb(224, 108, 117);
pub(crate) const RAW: Color = Color::Rgb(198, 120, 221);

pub(crate) fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { ACCENT } else { BORDER }))
}

pub(crate) fn safe_multiline(text: &str, limit: usize) -> (String, bool) {
    let mut output = String::new();
    let mut characters = text.chars().peekable();
    let mut count = 0;
    while let Some(character) = characters.next() {
        if count == limit {
            return (output, true);
        }
        count += 1;
        match character {
            '\r' if characters.peek() == Some(&'\n') => {}
            '\n' => output.push('\n'),
            '\t' => output.push_str("    "),
            character if character.is_control() => output.extend(character.escape_default()),
            character => output.push(character),
        }
    }
    (output, false)
}

pub(crate) fn safe_inline(text: &str) -> String {
    let mut output = String::new();
    for character in text.chars() {
        if character.is_control() {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn classification_name(classification: Classification) -> &'static str {
    match classification {
        Classification::Governed => "governed",
        Classification::Ungoverned => "ungoverned",
        Classification::Invalid => "invalid",
        Classification::Raw => "raw",
    }
}

pub(crate) fn classification_style(classification: Classification) -> Style {
    Style::default().fg(match classification {
        Classification::Governed => GOOD,
        Classification::Ungoverned => WARN,
        Classification::Invalid => BAD,
        Classification::Raw => RAW,
    })
}

pub(crate) fn status_style(status: &str) -> Style {
    let color = match status.to_ascii_lowercase().as_str() {
        "accepted" | "complete" | "completed" | "done" => GOOD,
        "rejected" | "blocked" | "failed" => BAD,
        "pending" | "proposed" | "review" => WARN,
        _ => ACCENT,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub(crate) fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Activation => "activation",
        Kind::ProjectMap => "project_map",
        Kind::Request => "request",
        Kind::Decision => "decision",
        Kind::Evidence => "evidence",
        Kind::Review => "review",
        Kind::Plan => "plan",
        Kind::Closeout => "closeout",
        Kind::Strategy => "strategy",
        Kind::StrategyBinding => "strategy binding",
        Kind::Ticket => "ticket",
        Kind::Epic => "epic",
        Kind::Board => "board",
        Kind::Progress => "progress",
    }
}

pub(crate) fn summary_title(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::Markdown { title })
        | Some(RecordSummary::WorkItem { title, .. })
        | Some(RecordSummary::Board { title, .. }) => title,
        _ => "",
    }
}

pub(crate) fn work_status(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::WorkItem { status, .. }) => status,
        _ => "",
    }
}
