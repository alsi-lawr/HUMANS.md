use crate::ui::{
    ACCENT, BAD, BORDER, GOOD, MUTED, SELECTED, WARN, classification_name, classification_style,
    kind_name, panel, safe_inline, status_style, summary_title, work_status,
};
use casefile_core::{Classification, EntrySnapshot, RecordSummary};
use casefile_store::{ActivationState, ScanResult};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    Projects,
    Investigations,
    Tickets,
    Files,
}

impl View {
    const ALL: [Self; 4] = [
        Self::Projects,
        Self::Investigations,
        Self::Tickets,
        Self::Files,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Projects => "Projects",
            Self::Investigations => "Investigations",
            Self::Tickets => "Tickets",
            Self::Files => "Files",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|view| *view == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

pub(crate) struct Browser {
    view: View,
    selected_project: Option<String>,
    selected_investigation: Option<String>,
    selected_path: Option<String>,
    filter: String,
    entering_filter: bool,
}

impl Browser {
    pub(crate) fn new(scan: &ScanResult) -> Self {
        let mut browser = Self {
            view: View::Projects,
            selected_project: None,
            selected_investigation: None,
            selected_path: None,
            filter: String::new(),
            entering_filter: false,
        };
        browser.normalise_selection(scan);
        browser
    }

    pub(crate) fn set_view(&mut self, scan: &ScanResult, view: View) {
        self.view = view;
        self.normalise_selection(scan);
    }

    pub(crate) fn cycle_view(&mut self, scan: &ScanResult) {
        self.set_view(scan, self.view.next());
    }

    pub(crate) fn drill_down(&mut self, scan: &ScanResult) -> bool {
        let next = match self.view {
            View::Projects => View::Investigations,
            View::Investigations => View::Tickets,
            View::Tickets | View::Files => return false,
        };
        self.set_view(scan, next);
        true
    }

    pub(crate) fn go_up(&mut self, scan: &ScanResult) -> bool {
        let next = match self.view {
            View::Projects => return false,
            View::Investigations => View::Projects,
            View::Tickets | View::Files => View::Investigations,
        };
        self.set_view(scan, next);
        true
    }

    pub(crate) fn is_entering_filter(&self) -> bool {
        self.entering_filter
    }

    pub(crate) fn start_filter(&mut self) {
        self.entering_filter = true;
    }

    pub(crate) fn close_filter(&mut self) {
        self.entering_filter = false;
    }

    pub(crate) fn push_filter(&mut self, scan: &ScanResult, character: char) -> bool {
        self.filter.push(character);
        self.normalise_selection(scan)
    }

    pub(crate) fn pop_filter(&mut self, scan: &ScanResult) -> bool {
        self.filter.pop();
        self.normalise_selection(scan)
    }

    pub(crate) fn clear_filter(&mut self, scan: &ScanResult) -> bool {
        self.filter.clear();
        self.normalise_selection(scan)
    }

    pub(crate) fn entries<'a>(&self, scan: &'a ScanResult) -> Vec<&'a EntrySnapshot> {
        scan.snapshot
            .entries
            .iter()
            .filter(|entry| self.matches_scope(scan, entry))
            .filter(|entry| self.matches_view(entry) && self.matches_entry_filter(entry))
            .collect()
    }

    pub(crate) fn selected<'a>(&self, scan: &'a ScanResult) -> Option<&'a EntrySnapshot> {
        if !matches!(self.view, View::Tickets | View::Files) {
            return None;
        }
        let path = self.selected_path.as_deref()?;
        scan.snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
    }

    pub(crate) fn select_offset(&mut self, scan: &ScanResult, offset: isize) -> bool {
        let changed = match self.view {
            View::Projects => {
                let values = self.projects(scan);
                let changed = select_value(&mut self.selected_project, &values, offset);
                if changed {
                    self.selected_investigation = None;
                    self.selected_path = None;
                }
                changed
            }
            View::Investigations => {
                let values = self.investigations(scan);
                select_value(&mut self.selected_investigation, &values, offset)
            }
            View::Tickets | View::Files => {
                let values = self
                    .entries(scan)
                    .into_iter()
                    .map(|entry| entry.path.clone())
                    .collect::<Vec<_>>();
                select_value(&mut self.selected_path, &values, offset)
            }
        };
        if changed {
            self.normalise_selection(scan);
        }
        changed
    }

    pub(crate) fn select_edge(&mut self, scan: &ScanResult, end: bool) -> bool {
        let offset = if end { isize::MAX } else { isize::MIN };
        self.select_offset(scan, offset)
    }

    pub(crate) fn render_header(&self, scan: &ScanResult, area: Rect, buffer: &mut Buffer) {
        let counts = [
            self.projects(scan).len(),
            self.investigations(scan).len(),
            self.ticket_count(scan),
            self.file_count(scan),
        ];
        let mut tabs = vec![Span::styled(
            " CASEFILE ",
            Style::default().fg(ACCENT).bold(),
        )];
        for (index, (view, count)) in View::ALL.into_iter().zip(counts).enumerate() {
            tabs.push(Span::raw(" "));
            tabs.push(Span::styled(
                format!(" [{}] {} {count} ", index + 1, view.title().to_uppercase()),
                tab_style(self.view == view),
            ));
        }
        tabs.extend([
            Span::raw("   "),
            Span::styled(
                activation_name(scan.activation),
                activation_style(scan.activation).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  |  {} diagnostics", scan.diagnostics.len()),
                Style::default().fg(MUTED),
            ),
        ]);
        let filter = if self.filter.is_empty() {
            "none".to_owned()
        } else {
            format!("\"{}\"", safe_inline(&self.filter))
        };
        let scope = match (&self.selected_project, &self.selected_investigation) {
            (Some(project), Some(investigation)) => format!("{project} / {investigation}"),
            (Some(project), None) => project.clone(),
            _ => "none".into(),
        };
        let lines = vec![
            Line::from(tabs),
            Line::from(vec![
                Span::styled(" Scope ", Style::default().fg(MUTED)),
                Span::styled(safe_inline(&scope), Style::default().fg(Color::White)),
                Span::styled("  |  Filter ", Style::default().fg(MUTED)),
                Span::styled(filter, Style::default().fg(Color::White)),
                if self.entering_filter {
                    Span::styled("  TYPE TO FILTER", Style::default().fg(WARN).bold())
                } else {
                    Span::raw("")
                },
            ]),
        ];
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(BORDER)),
            )
            .render(area, buffer);
    }

    pub(crate) fn render_list(
        &self,
        scan: &ScanResult,
        focused: bool,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let (items, selected) = self.list_items(scan);
        let position = selected
            .map(|index| format!("{} / {}", index + 1, items.len()))
            .unwrap_or_else(|| format!("0 / {}", items.len()));
        let block = panel(format!(" {}  {position} ", self.view.title()), focused);
        if items.is_empty() {
            let message = if self.filter.is_empty() {
                match self.view {
                    View::Projects => "No projects are present in this Casefile root.",
                    View::Investigations => "This project has no investigations.",
                    View::Tickets => "This investigation has no governed tickets or epics.",
                    View::Files => "This scope has no non-ticket files.",
                }
            } else {
                "Nothing matches the active filter. Press c to clear it."
            };
            Paragraph::new(message)
                .style(Style::default().fg(MUTED))
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, buffer);
            return;
        }
        let mut state = ListState::default();
        state.select(selected);
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(SELECTED)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">");
        StatefulWidget::render(list, area, buffer, &mut state);
    }

    fn list_items(&self, scan: &ScanResult) -> (Vec<ListItem<'static>>, Option<usize>) {
        match self.view {
            View::Projects => {
                let values = self.projects(scan);
                let selected = selected_index(&values, self.selected_project.as_deref());
                let items = values
                    .iter()
                    .map(|project| {
                        let investigations = all_investigations(scan, project).len();
                        let tickets = work_entries(scan)
                            .into_iter()
                            .filter(|entry| {
                                entry_scope(scan, entry).is_some_and(|scope| scope.0 == project)
                            })
                            .count();
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!(" {project} "),
                                Style::default().fg(ACCENT).bold(),
                            ),
                            Span::styled(
                                format!("  {investigations} investigations  {tickets} tickets"),
                                Style::default().fg(MUTED),
                            ),
                        ]))
                    })
                    .collect();
                (items, selected)
            }
            View::Investigations => {
                let values = self.investigations(scan);
                let selected = selected_index(&values, self.selected_investigation.as_deref());
                let project = self.selected_project.as_deref().unwrap_or_default();
                let items = values
                    .iter()
                    .map(|investigation| {
                        let tickets = work_entries(scan)
                            .into_iter()
                            .filter(|entry| {
                                entry_scope(scan, entry).is_some_and(|scope| {
                                    scope.0 == project && scope.1 == Some(investigation.as_str())
                                })
                            })
                            .count();
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!(" {investigation} "),
                                Style::default().fg(Color::White).bold(),
                            ),
                            Span::styled(
                                format!("  {tickets} tickets"),
                                Style::default().fg(MUTED),
                            ),
                        ]))
                    })
                    .collect();
                (items, selected)
            }
            View::Tickets => self.entry_items(scan, false),
            View::Files => self.entry_items(scan, true),
        }
    }

    fn entry_items(
        &self,
        scan: &ScanResult,
        directories: bool,
    ) -> (Vec<ListItem<'static>>, Option<usize>) {
        let entries = self.entries(scan);
        let selected = entries
            .iter()
            .position(|entry| Some(entry.path.as_str()) == self.selected_path.as_deref());
        let mut previous_directory = String::new();
        let items = entries
            .into_iter()
            .map(|entry| {
                let directory = relative_parent_directory(
                    scan,
                    entry,
                    self.selected_project.as_deref(),
                    self.selected_investigation.as_deref(),
                );
                let show_directory = directories && directory != previous_directory;
                previous_directory = directory.clone();
                let mut lines = Vec::new();
                if show_directory {
                    lines.push(
                        Line::from(format!(" {}/", safe_inline(&directory)))
                            .style(Style::default().fg(ACCENT).bold()),
                    );
                }
                lines.push(entry_label(entry, self.view));
                ListItem::new(lines)
            })
            .collect();
        (items, selected)
    }

    fn projects(&self, scan: &ScanResult) -> Vec<String> {
        all_projects(scan)
            .into_iter()
            .filter(|project| {
                self.filter.is_empty()
                    || project.to_lowercase().contains(&self.filter.to_lowercase())
                    || scan.snapshot.entries.iter().any(|entry| {
                        entry_scope(scan, entry).is_some_and(|scope| scope.0 == project)
                            && self.matches_entry_filter(entry)
                    })
            })
            .collect()
    }

    fn investigations(&self, scan: &ScanResult) -> Vec<String> {
        let Some(project) = self.selected_project.as_deref() else {
            return Vec::new();
        };
        all_investigations(scan, project)
            .into_iter()
            .filter(|investigation| {
                self.filter.is_empty()
                    || investigation
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
                    || scan.snapshot.entries.iter().any(|entry| {
                        entry_scope(scan, entry).is_some_and(|scope| {
                            scope.0 == project && scope.1 == Some(investigation.as_str())
                        }) && self.matches_entry_filter(entry)
                    })
            })
            .collect()
    }

    fn ticket_count(&self, scan: &ScanResult) -> usize {
        work_entries(scan)
            .into_iter()
            .filter(|entry| self.matches_scope(scan, entry))
            .count()
    }

    fn file_count(&self, scan: &ScanResult) -> usize {
        scan.snapshot
            .entries
            .iter()
            .filter(|entry| self.matches_scope(scan, entry) && !is_work(entry))
            .count()
    }

    fn matches_scope(&self, scan: &ScanResult, entry: &EntrySnapshot) -> bool {
        let Some((project, investigation)) = entry_scope(scan, entry) else {
            return false;
        };
        self.selected_project.as_deref() == Some(project)
            && match self.view {
                View::Projects | View::Investigations => true,
                View::Tickets => self.selected_investigation.as_deref() == investigation,
                View::Files => {
                    investigation.is_none()
                        || self.selected_investigation.as_deref() == investigation
                }
            }
    }

    fn matches_view(&self, entry: &EntrySnapshot) -> bool {
        match self.view {
            View::Tickets => is_work(entry),
            View::Files => !is_work(entry),
            View::Projects | View::Investigations => false,
        }
    }

    fn matches_entry_filter(&self, entry: &EntrySnapshot) -> bool {
        let filter = self.filter.to_lowercase();
        filter.is_empty()
            || [
                entry.path.as_str(),
                classification_name(entry.classification),
                entry.kind.map(kind_name).unwrap_or_default(),
                entry.identity.as_deref().unwrap_or_default(),
                summary_title(entry.summary.as_ref()),
                work_status(entry.summary.as_ref()),
            ]
            .into_iter()
            .any(|field| field.to_lowercase().contains(&filter))
    }

    fn normalise_selection(&mut self, scan: &ScanResult) -> bool {
        let previous = (
            self.selected_project.clone(),
            self.selected_investigation.clone(),
            self.selected_path.clone(),
        );
        let projects = self.projects(scan);
        normalise_value(&mut self.selected_project, &projects);
        let investigations = self.investigations(scan);
        normalise_value(&mut self.selected_investigation, &investigations);
        let paths = self
            .entries(scan)
            .into_iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        normalise_value(&mut self.selected_path, &paths);
        previous
            != (
                self.selected_project.clone(),
                self.selected_investigation.clone(),
                self.selected_path.clone(),
            )
    }
}

fn all_projects(scan: &ScanResult) -> Vec<String> {
    scan.snapshot
        .entries
        .iter()
        .filter_map(|entry| entry_scope(scan, entry))
        .map(|scope| scope.0.to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn all_investigations(scan: &ScanResult, project: &str) -> Vec<String> {
    scan.snapshot
        .entries
        .iter()
        .filter_map(|entry| entry_scope(scan, entry))
        .filter(|scope| scope.0 == project)
        .filter_map(|scope| scope.1.map(str::to_owned))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn work_entries(scan: &ScanResult) -> Vec<&EntrySnapshot> {
    scan.snapshot
        .entries
        .iter()
        .filter(|entry| is_work(entry))
        .collect()
}

fn is_work(entry: &EntrySnapshot) -> bool {
    entry.classification == Classification::Governed
        && matches!(entry.summary, Some(RecordSummary::WorkItem { .. }))
}

fn entry_scope<'a>(
    scan: &'a ScanResult,
    entry: &'a EntrySnapshot,
) -> Option<(&'a str, Option<&'a str>)> {
    scan.scope_for_path(&entry.path)
}

fn relative_parent_directory(
    scan: &ScanResult,
    entry: &EntrySnapshot,
    project: Option<&str>,
    investigation: Option<&str>,
) -> String {
    let project_prefix = project.map(|project| format!("projects/{project}/"));
    let investigation_prefix = project.zip(investigation).map(|(project, investigation)| {
        format!("projects/{project}/investigations/{investigation}/")
    });
    let prefix = match entry_scope(scan, entry) {
        Some((_, Some(_))) => investigation_prefix.as_deref(),
        Some((_, None)) => project_prefix.as_deref(),
        None => None,
    };
    let relative = prefix
        .and_then(|prefix| entry.path.strip_prefix(prefix))
        .unwrap_or(&entry.path);
    parent_directory(relative)
}

fn parent_directory(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(|| ".".into(), |(directory, _)| directory.into())
}

fn entry_label(entry: &EntrySnapshot, view: View) -> Line<'static> {
    match (view, entry.summary.as_ref()) {
        (
            View::Tickets,
            Some(RecordSummary::WorkItem {
                id,
                title,
                status,
                rank,
            }),
        ) => Line::from(vec![
            Span::styled(
                format!(" {:^10} ", safe_inline(status).to_uppercase()),
                status_style(status),
            ),
            Span::styled(
                format!(" {} ", safe_inline(id)),
                Style::default().fg(ACCENT),
            ),
            Span::raw(safe_inline(title)),
            Span::styled(
                rank.map(|rank| format!("  #{rank}")).unwrap_or_default(),
                Style::default().fg(MUTED),
            ),
        ]),
        _ => Line::from(vec![
            Span::styled(
                format!(
                    " {:^10} ",
                    classification_name(entry.classification).to_uppercase()
                ),
                classification_style(entry.classification),
            ),
            Span::styled(
                format!(" {} ", entry.kind.map(kind_name).unwrap_or("file")),
                Style::default().fg(MUTED),
            ),
            Span::styled(
                safe_inline(entry.path.rsplit('/').next().unwrap_or(&entry.path)),
                Style::default().fg(Color::White),
            ),
        ]),
    }
}

fn select_value(selected: &mut Option<String>, values: &[String], offset: isize) -> bool {
    if values.is_empty() {
        return selected.take().is_some();
    }
    let index = selected_index(values, selected.as_deref()).unwrap_or(0);
    let next = if offset == isize::MAX {
        values.len() - 1
    } else if offset == isize::MIN {
        0
    } else {
        (index as isize + offset).clamp(0, values.len() as isize - 1) as usize
    };
    if selected.as_deref() == Some(values[next].as_str()) {
        false
    } else {
        *selected = Some(values[next].clone());
        true
    }
}

fn normalise_value(selected: &mut Option<String>, values: &[String]) {
    if !selected
        .as_ref()
        .is_some_and(|current| values.contains(current))
    {
        *selected = values.first().cloned();
    }
}

fn selected_index(values: &[String], selected: Option<&str>) -> Option<usize> {
    values
        .iter()
        .position(|value| Some(value.as_str()) == selected)
}

fn tab_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED)
    }
}

fn activation_name(activation: ActivationState) -> &'static str {
    match activation {
        ActivationState::Active => "ACTIVE",
        ActivationState::Unactivated => "UNACTIVATED",
        ActivationState::Invalid => "INVALID ACTIVATION",
    }
}

fn activation_style(activation: ActivationState) -> Style {
    Style::default().fg(match activation {
        ActivationState::Active => GOOD,
        ActivationState::Unactivated => WARN,
        ActivationState::Invalid => BAD,
    })
}
