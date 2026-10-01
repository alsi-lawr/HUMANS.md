mod projection;
use crate::ui::{
    ACCENT, BAD, BORDER, GOOD, MUTED, SELECTED, WARN, classification_name, classification_style,
    kind_name, panel, safe_inline, status_style, summary_title, work_status,
};
use casefile_core::{Classification, EntrySnapshot, Kind, RecordSummary};
use casefile_store::{ActivationState, ScanResult};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    Projects,
    Investigations,
    Tickets,
    Files,
    Strategies,
    Boards,
}

impl View {
    const ALL: [Self; 6] = [
        Self::Projects,
        Self::Investigations,
        Self::Tickets,
        Self::Files,
        Self::Strategies,
        Self::Boards,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Projects => "Projects",
            Self::Investigations => "Investigations",
            Self::Tickets => "Tickets",
            Self::Files => "Files",
            Self::Strategies => "Strategies",
            Self::Boards => "Boards",
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
    projection: projection::Projection,
    visible: std::cell::RefCell<projection::Visible>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowserState {
    view: View,
    selected_project: Option<String>,
    selected_investigation: Option<String>,
    selected_path: Option<String>,
    filter: String,
    entering_filter: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectionAnchor {
    project: Option<String>,
    project_index: Option<usize>,
    investigation: Option<String>,
    investigation_index: Option<usize>,
    record: Option<RecordAnchor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordAnchor {
    path: String,
    visible_index: Option<usize>,
    governed: Option<GovernedAnchor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GovernedAnchor {
    project: String,
    investigation: Option<String>,
    kind: Kind,
    identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PromotionNotice {
    FilteredOut,
    Ambiguous,
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
            projection: projection::Projection::default(),
            visible: std::cell::RefCell::default(),
        };
        browser.projection.rebuild(scan);
        browser.normalise_selection(scan);
        browser
    }

    pub(crate) fn rebuild(&mut self, scan: &ScanResult) {
        self.projection.rebuild(scan);
    }

    pub(crate) fn update(
        &mut self,
        scan: &ScanResult,
        indices: &std::collections::BTreeMap<String, usize>,
        changed: &std::collections::BTreeSet<String>,
    ) {
        self.projection.update(scan, indices, changed);
    }

    pub(crate) fn set_view(&mut self, scan: &ScanResult, view: View) {
        self.view = view;
        self.normalise_selection(scan);
    }

    pub(crate) fn cycle_view(&mut self, scan: &ScanResult) {
        self.set_view(scan, self.view.next());
    }

    pub(crate) fn view(&self) -> View {
        self.view
    }

    pub(crate) fn state(&self) -> BrowserState {
        BrowserState {
            view: self.view,
            selected_project: self.selected_project.clone(),
            selected_investigation: self.selected_investigation.clone(),
            selected_path: self.selected_path.clone(),
            filter: self.filter.clone(),
            entering_filter: self.entering_filter,
        }
    }

    pub(crate) fn restore(&mut self, state: BrowserState) {
        self.view = state.view;
        self.selected_project = state.selected_project;
        self.selected_investigation = state.selected_investigation;
        self.selected_path = state.selected_path;
        self.filter = state.filter;
        self.entering_filter = state.entering_filter;
    }

    pub(crate) fn selected_project(&self) -> Option<&str> {
        self.selected_project.as_deref()
    }

    pub(crate) fn selected_investigation(&self) -> Option<&str> {
        self.selected_investigation.as_deref()
    }

    pub(crate) fn selected_path(&self) -> Option<&str> {
        self.selected_path.as_deref()
    }

    pub(crate) fn apply_partial(&mut self, scan: &ScanResult) {
        if self.selected_project.is_none() {
            self.selected_project = self.projects(scan).first().cloned();
        }
        if self.selected_investigation.is_none() {
            self.selected_investigation = self.investigations(scan).first().cloned();
        }
        if self.selected_path.is_none() && self.view != View::Boards {
            self.selected_path = self.entries(scan).first().map(|entry| entry.path.clone());
        }
    }

    pub(crate) fn anchor(&self, scan: &ScanResult, board_paths: &[String]) -> SelectionAnchor {
        let project_values = self.projects(scan);
        let investigation_values = self.investigations(scan);
        let record = self.selected_path.as_ref().map(|path| {
            let entry = scan
                .snapshot
                .entries
                .iter()
                .find(|entry| &entry.path == path);
            let governed = entry.and_then(|entry| {
                let (project, investigation) = scan.scope_for_path(&entry.path)?;
                (entry.classification == Classification::Governed).then_some(GovernedAnchor {
                    project: project.into(),
                    investigation: investigation.map(Into::into),
                    kind: entry.kind?,
                    identity: entry.identity.clone()?,
                })
            });
            RecordAnchor {
                path: path.clone(),
                visible_index: self
                    .visible_record_paths(scan, board_paths)
                    .iter()
                    .position(|candidate| candidate == path),
                governed,
            }
        });
        SelectionAnchor {
            project: self.selected_project.clone(),
            project_index: selected_index(&project_values, self.selected_project.as_deref()),
            investigation: self.selected_investigation.clone(),
            investigation_index: selected_index(
                &investigation_values,
                self.selected_investigation.as_deref(),
            ),
            record,
        }
    }

    pub(crate) fn promote(
        &mut self,
        scan: &ScanResult,
        anchor: &SelectionAnchor,
        board_paths: &[String],
    ) -> Option<PromotionNotice> {
        let all_projects = all_projects(scan);
        let visible_projects = self.projects(scan);
        let mut notice = None;
        self.selected_project = resolve_exact_or_nearest(
            anchor.project.as_deref(),
            anchor.project_index,
            &all_projects,
            &visible_projects,
            &mut notice,
        );
        let project_changed = self.selected_project.as_deref() != anchor.project.as_deref();
        if project_changed {
            self.selected_investigation = None;
            self.selected_path = None;
        }

        let all_investigations = self
            .selected_project
            .as_deref()
            .map(|project| all_investigations(scan, project))
            .unwrap_or_default();
        let visible_investigations = self.investigations(scan);
        self.selected_investigation = resolve_exact_or_nearest(
            (!project_changed)
                .then_some(anchor.investigation.as_deref())
                .flatten(),
            (!project_changed)
                .then_some(anchor.investigation_index)
                .flatten(),
            &all_investigations,
            &visible_investigations,
            &mut notice,
        );
        let investigation_changed = project_changed
            || self.selected_investigation.as_deref() != anchor.investigation.as_deref();
        if investigation_changed {
            self.selected_path = None;
        }

        if matches!(
            self.view,
            View::Tickets | View::Files | View::Strategies | View::Boards
        ) {
            self.selected_path = self.resolve_record(
                scan,
                (!investigation_changed)
                    .then_some(anchor.record.as_ref())
                    .flatten(),
                board_paths,
                &mut notice,
            );
        } else {
            self.selected_path = None;
        }
        notice
    }

    fn resolve_record(
        &self,
        scan: &ScanResult,
        anchor: Option<&RecordAnchor>,
        board_paths: &[String],
        notice: &mut Option<PromotionNotice>,
    ) -> Option<String> {
        let visible = self.visible_record_paths(scan, board_paths);
        let Some(anchor) = anchor else {
            return visible.first().cloned();
        };
        if self.view == View::Boards {
            if visible.contains(&anchor.path) {
                return Some(anchor.path.clone());
            }
            if let Some(governed) = &anchor.governed {
                let matches = governed_matches(scan, governed);
                if matches.len() > 1 {
                    *notice = Some(PromotionNotice::Ambiguous);
                    return None;
                }
                if let [entry] = matches.as_slice()
                    && visible.contains(&entry.path)
                {
                    return Some(entry.path.clone());
                }
            }
            return nearest(&visible, anchor.visible_index);
        }
        if scan
            .snapshot
            .entries
            .iter()
            .any(|entry| entry.path == anchor.path)
        {
            if visible.contains(&anchor.path) {
                return Some(anchor.path.clone());
            }
            *notice = Some(PromotionNotice::FilteredOut);
            return None;
        }
        if let Some(governed) = &anchor.governed {
            let matches = governed_matches(scan, governed);
            if matches.len() > 1 {
                *notice = Some(PromotionNotice::Ambiguous);
                return None;
            }
            match matches.as_slice() {
                [entry] if visible.contains(&entry.path) => return Some(entry.path.clone()),
                [entry] => {
                    debug_assert!(!visible.contains(&entry.path));
                    *notice = Some(PromotionNotice::FilteredOut);
                    return None;
                }
                [] => {}
                _ => unreachable!("ambiguous identities were handled above"),
            }
        }
        nearest(&visible, anchor.visible_index)
    }

    fn visible_record_paths(&self, scan: &ScanResult, board_paths: &[String]) -> Vec<String> {
        if self.view == View::Boards {
            board_paths.to_vec()
        } else {
            self.entries(scan)
                .into_iter()
                .map(|entry| entry.path.clone())
                .collect()
        }
    }

    pub(crate) fn drill_down(&mut self, scan: &ScanResult) -> bool {
        let next = match self.view {
            View::Projects => View::Investigations,
            View::Investigations => View::Tickets,
            View::Tickets | View::Files | View::Strategies | View::Boards => return false,
        };
        self.set_view(scan, next);
        true
    }

    pub(crate) fn go_up(&mut self, scan: &ScanResult) -> bool {
        let next = match self.view {
            View::Projects => return false,
            View::Investigations => View::Projects,
            View::Tickets | View::Files | View::Strategies | View::Boards => View::Investigations,
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
        self.visible()
            .paths
            .iter()
            .filter_map(|path| self.projection.indices.get(path))
            .map(|index| &scan.snapshot.entries[*index])
            .collect()
    }

    pub(crate) fn selected<'a>(&self, scan: &'a ScanResult) -> Option<&'a EntrySnapshot> {
        if !matches!(
            self.view,
            View::Tickets | View::Files | View::Strategies | View::Boards
        ) {
            return None;
        }
        self.selected_path
            .as_ref()
            .and_then(|path| self.projection.indices.get(path))
            .and_then(|index| scan.snapshot.entries.get(*index))
    }

    pub(crate) fn select_offset(&mut self, scan: &ScanResult, offset: isize) -> bool {
        let next = {
            let visible = self.visible();
            let (selected, values) = match self.view {
                View::Projects => (&self.selected_project, &visible.projects),
                View::Investigations => (&self.selected_investigation, &visible.investigations),
                View::Tickets | View::Files | View::Strategies | View::Boards => {
                    (&self.selected_path, &visible.paths)
                }
            };
            next_value(selected.as_deref(), values, offset)
        };
        match self.view {
            View::Projects => {
                if self.selected_project == next {
                    return false;
                }
                self.selected_project = next;
                self.selected_investigation = None;
                self.selected_path = None;
                self.normalise_selection(scan);
            }
            View::Investigations => {
                if self.selected_investigation == next {
                    return false;
                }
                self.selected_investigation = next;
                self.normalise_selection(scan);
            }
            View::Tickets | View::Files | View::Strategies | View::Boards => {
                if self.selected_path == next {
                    return false;
                }
                self.selected_path = next;
            }
        }
        true
    }

    pub(crate) fn select_edge(&mut self, scan: &ScanResult, end: bool) -> bool {
        let offset = if end { isize::MAX } else { isize::MIN };
        self.select_offset(scan, offset)
    }

    pub(crate) fn select_board_offset(&mut self, paths: &[String], offset: isize) -> bool {
        select_value(&mut self.selected_path, paths, offset)
    }

    pub(crate) fn scope(&self) -> Option<(&str, &str)> {
        self.selected_project
            .as_deref()
            .zip(self.selected_investigation.as_deref())
    }

    pub(crate) fn render_header(
        &self,
        scan: &ScanResult,
        board_count: usize,
        diagnostic_count: usize,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let visible = self.visible();
        let [tickets, files, strategies] = visible.counts;
        let counts = [
            visible.projects.len(),
            visible.investigations.len(),
            tickets,
            files,
            strategies,
            board_count,
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
                format!("  |  {} diagnostics", diagnostic_count),
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
        let visible = self.visible();
        let (total, absolute_selected) = match self.view {
            View::Projects => (
                visible.projects.len(),
                selected_index(&visible.projects, self.selected_project.as_deref()),
            ),
            View::Investigations => (
                visible.investigations.len(),
                selected_index(
                    &visible.investigations,
                    self.selected_investigation.as_deref(),
                ),
            ),
            View::Tickets | View::Files | View::Strategies => (
                visible.paths.len(),
                self.selected_path
                    .as_ref()
                    .and_then(|path| visible.positions.get(path).copied()),
            ),
            View::Boards => (0, None),
        };
        let height = usize::from(panel("", focused).inner(area).height);
        let selected_end = absolute_selected.map_or(0, |index| visible.row_end(self.view, index));
        let offset = visible.start_row(self.view, selected_end.saturating_sub(height));
        let (items, _) = self.list_items(scan, offset, height);
        let selected = absolute_selected.map(|index| index.saturating_sub(offset));
        let position = selected
            .map(|index| format!("{} / {}", absolute_selected.unwrap_or(index) + 1, total))
            .unwrap_or_else(|| format!("0 / {total}"));
        let block = panel(format!(" {}  {position} ", self.view.title()), focused);
        if items.is_empty() {
            let message = if self.filter.is_empty() {
                match self.view {
                    View::Projects => "No projects are present in this Casefile root.",
                    View::Investigations => "This project has no investigations.",
                    View::Tickets => "This investigation has no governed tickets or epics.",
                    View::Files => "This scope has no non-ticket files.",
                    View::Strategies => "This investigation has no strategy records.",
                    View::Boards => "This investigation has no board definitions.",
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

    fn list_items(
        &self,
        scan: &ScanResult,
        offset: usize,
        height: usize,
    ) -> (Vec<ListItem<'static>>, Option<usize>) {
        match self.view {
            View::Projects => {
                let visible = self.visible();
                let values = &visible.projects;
                let selected = selected_index(values, self.selected_project.as_deref());
                let items = values
                    .iter()
                    .skip(offset)
                    .take(height)
                    .map(|project| {
                        let (investigations, tickets) = self.projection.project_counts(project);
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
                let visible = self.visible();
                let values = &visible.investigations;
                let selected = selected_index(values, self.selected_investigation.as_deref());
                let project = self.selected_project.as_deref().unwrap_or_default();
                let items = values
                    .iter()
                    .skip(offset)
                    .take(height)
                    .map(|investigation| {
                        let tickets = self.projection.tickets(project, investigation);
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
            View::Tickets => self.entry_items(scan, false, offset, height),
            View::Files => self.entry_items(scan, true, offset, height),
            View::Strategies => self.entry_items(scan, false, offset, height),
            View::Boards => (Vec::new(), None),
        }
    }

    fn entry_items(
        &self,
        scan: &ScanResult,
        directories: bool,
        offset: usize,
        height: usize,
    ) -> (Vec<ListItem<'static>>, Option<usize>) {
        let visible = self.visible();
        let selected = self
            .selected_path
            .as_ref()
            .and_then(|path| visible.positions.get(path).copied());
        let mut used = 0;
        let items = visible
            .paths
            .iter()
            .enumerate()
            .skip(offset)
            .take_while(|(index, _)| {
                let include = used < height;
                used += visible.row_height(*index);
                include
            })
            .map(|(index, path)| {
                let entry = &scan.snapshot.entries[self.projection.indices[path]];
                let directory = visible.directories[index].as_deref().unwrap_or_default();
                let show_directory = directories && visible.directories[index].is_some();
                let mut lines = Vec::new();
                if show_directory {
                    lines.push(
                        Line::from(format!(" {}/", safe_inline(directory)))
                            .style(Style::default().fg(ACCENT).bold()),
                    );
                }
                lines.push(entry_label(entry, self.view));
                ListItem::new(lines)
            })
            .collect();
        (items, selected)
    }

    fn projects(&self, _scan: &ScanResult) -> Vec<String> {
        self.visible().projects.clone()
    }
    fn investigations(&self, _scan: &ScanResult) -> Vec<String> {
        self.visible().investigations.clone()
    }

    fn normalise_selection(&mut self, _scan: &ScanResult) -> bool {
        let previous = (
            self.selected_project.clone(),
            self.selected_investigation.clone(),
            self.selected_path.clone(),
        );
        let project = normalised(self.selected_project.as_deref(), &self.visible().projects);
        self.selected_project = project;
        let investigation = normalised(
            self.selected_investigation.as_deref(),
            &self.visible().investigations,
        );
        self.selected_investigation = investigation;
        if self.view != View::Boards {
            let path = normalised(self.selected_path.as_deref(), &self.visible().paths);
            self.selected_path = path;
        }
        previous
            != (
                self.selected_project.clone(),
                self.selected_investigation.clone(),
                self.selected_path.clone(),
            )
    }
}

fn governed_matches<'a>(scan: &'a ScanResult, governed: &GovernedAnchor) -> Vec<&'a EntrySnapshot> {
    scan.snapshot
        .entries
        .iter()
        .filter(|entry| {
            entry.classification == Classification::Governed
                && entry.kind == Some(governed.kind)
                && entry.identity.as_deref() == Some(governed.identity.as_str())
                && scan.scope_for_path(&entry.path).is_some_and(|scope| {
                    scope.0 == governed.project && scope.1 == governed.investigation.as_deref()
                })
        })
        .collect()
}

fn all_projects(scan: &ScanResult) -> Vec<String> {
    scan.investigation_roots.keys().cloned().collect()
}

fn all_investigations(scan: &ScanResult, project: &str) -> Vec<String> {
    scan.investigation_roots
        .get(project)
        .cloned()
        .unwrap_or_default()
}

fn resolve_exact_or_nearest(
    anchor: Option<&str>,
    old_index: Option<usize>,
    all: &[String],
    visible: &[String],
    notice: &mut Option<PromotionNotice>,
) -> Option<String> {
    if let Some(anchor) = anchor
        && all.iter().any(|value| value == anchor)
    {
        if visible.iter().any(|value| value == anchor) {
            return Some(anchor.into());
        }
        *notice = Some(PromotionNotice::FilteredOut);
        return None;
    }
    nearest(visible, old_index)
}

fn nearest(values: &[String], old_index: Option<usize>) -> Option<String> {
    let index = old_index
        .unwrap_or_default()
        .min(values.len().saturating_sub(1));
    values.get(index).cloned()
}

fn is_work(entry: &EntrySnapshot) -> bool {
    entry.classification == Classification::Governed
        && matches!(entry.summary, Some(RecordSummary::WorkItem { .. }))
}

fn is_strategy(entry: &EntrySnapshot) -> bool {
    matches!(
        entry.classification,
        Classification::Governed | Classification::Invalid
    ) && matches!(entry.kind, Some(Kind::Strategy | Kind::StrategyBinding))
}

fn strategy_phase(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::Strategy { phase, .. }) => phase,
        _ => "",
    }
}

fn strategy_role(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::StrategyBinding { binding }) => &binding.role,
        _ => "",
    }
}

fn strategy_model(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::StrategyBinding { binding }) => &binding.model,
        _ => "",
    }
}

fn strategy_reasoning(summary: Option<&RecordSummary>) -> &str {
    match summary {
        Some(RecordSummary::StrategyBinding { binding }) => &binding.reasoning_effort,
        _ => "",
    }
}

fn entry_scope<'a>(
    scan: &'a ScanResult,
    entry: &'a EntrySnapshot,
) -> Option<(&'a str, Option<&'a str>)> {
    scan.scope_for_path(&entry.path)
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
        (
            View::Strategies,
            Some(RecordSummary::Strategy {
                strategy_id, phase, ..
            }),
        ) => Line::from(vec![
            Span::styled(
                format!(" {:^12} ", safe_inline(phase).to_uppercase()),
                classification_style(entry.classification),
            ),
            Span::styled(
                format!(" {} ", kind_name(Kind::Strategy)),
                Style::default().fg(MUTED),
            ),
            Span::styled(safe_inline(strategy_id), Style::default().fg(Color::White)),
        ]),
        (View::Strategies, Some(RecordSummary::StrategyBinding { binding })) => Line::from(vec![
            Span::styled(
                " IMPLEMENTATION ",
                classification_style(entry.classification),
            ),
            Span::styled(" writer ", Style::default().fg(MUTED)),
            Span::styled(
                format!(
                    "{} / {}",
                    safe_inline(&binding.model),
                    safe_inline(&binding.reasoning_effort)
                ),
                Style::default().fg(Color::White),
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

fn next_value(selected: Option<&str>, values: &[String], offset: isize) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    let index = selected.and_then(|selected| {
        values
            .binary_search_by(|value| value.as_str().cmp(selected))
            .ok()
    });
    let next = navigation_index(index, values.len(), offset);
    Some(values[next].clone())
}

fn normalised(selected: Option<&str>, values: &[String]) -> Option<String> {
    selected
        .and_then(|selected| {
            values
                .binary_search_by(|value| value.as_str().cmp(selected))
                .ok()
        })
        .map(|index| values[index].clone())
        .or_else(|| values.first().cloned())
}

fn select_value(selected: &mut Option<String>, values: &[String], offset: isize) -> bool {
    if values.is_empty() {
        return selected.take().is_some();
    }
    let index = selected_index(values, selected.as_deref());
    let next = navigation_index(index, values.len(), offset);
    if selected.as_deref() == Some(values[next].as_str()) {
        false
    } else {
        *selected = Some(values[next].clone());
        true
    }
}

fn navigation_index(index: Option<usize>, count: usize, offset: isize) -> usize {
    match (index, offset) {
        (_, isize::MAX) => count - 1,
        (_, isize::MIN) => 0,
        (Some(0), -1) => count - 1,
        (Some(index), 1) if index == count - 1 => 0,
        _ => (index.unwrap_or(0) as isize + offset).clamp(0, count as isize - 1) as usize,
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
