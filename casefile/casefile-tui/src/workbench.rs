mod boards;
mod facts;
mod projection;

use crate::{
    Interaction, PAGE_SIZE,
    browsing::{Browser, BrowserState, PromotionNotice, SelectionAnchor, View},
    interaction::edit_selection,
    progressive::{Coordinator, ProjectionChange, UiProjection},
    record_detail::{DetailState, RecordDetail},
    ui::{ACCENT, MUTED, WARN, safe_inline},
    watching::{SelectedScope, WatchCoordinator},
};
use casefile_store::{DerivedSnapshot, PresentationTarget, ScanResult};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
};
use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    time::Duration,
};

const WIDE_MINIMUM: u16 = 96;
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayoutMode {
    Wide,
    Narrow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    List,
    Detail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchResume {
    browser: BrowserState,
    anchor: SelectionAnchor,
    detail: DetailState,
    focus: Focus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefreshIntent {
    Current,
    Store,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::List => Self::Detail,
            Self::Detail => Self::List,
        }
    }
}

pub(crate) struct App {
    scan: ScanResult,
    entry_indices: BTreeMap<String, usize>,
    record_indices: BTreeMap<String, usize>,
    derived: DerivedSnapshot,
    relationships:
        BTreeMap<casefile_store::ScopedIdentity, Vec<casefile_store::DerivedRelationship>>,
    board_records: BTreeMap<casefile_store::ScopedIdentity, casefile_store::DerivedBoard>,
    facts: facts::Facts,
    body_owners: BTreeMap<String, std::sync::Arc<casefile_store::PresentationEntry>>,
    boards: std::cell::RefCell<boards::Cache>,
    board_generation: u64,
    browser: Browser,
    detail: RecordDetail,
    focus: Focus,
    show_help: bool,
    feedback: Option<String>,
    status: Option<String>,
    freshness: Option<String>,
    provisional: bool,
    unavailable: BTreeMap<String, String>,
    resume_anchor: Option<SelectionAnchor>,
    refresh_intent: Option<RefreshIntent>,
    interaction: Option<Interaction>,
}

impl App {
    pub(crate) fn new(mut scan: ScanResult, mut derived: DerivedSnapshot) -> Self {
        for roots in scan.investigation_roots.values_mut() {
            roots.sort();
            roots.dedup();
        }
        let browser = Browser::new(&scan);
        let mut facts = facts::Facts::default();
        facts.rebuild(&scan);
        scan.diagnostics.clear();
        derived.diagnostics.clear();
        let board_records = std::mem::take(&mut derived.boards)
            .into_iter()
            .map(|board| (board.identity.clone(), board))
            .collect();
        let mut relationships: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for edge in std::mem::take(&mut derived.relationships) {
            relationships
                .entry(edge.source.clone())
                .or_default()
                .push(edge);
        }
        Self {
            entry_indices: scan
                .snapshot
                .entries
                .iter()
                .enumerate()
                .map(|(i, entry)| (entry.path.clone(), i))
                .collect(),
            record_indices: derived
                .records
                .iter()
                .enumerate()
                .map(|(i, record)| (record.path.clone(), i))
                .collect(),
            scan,
            derived,
            relationships,
            board_records,
            facts,
            body_owners: BTreeMap::new(),
            boards: std::cell::RefCell::default(),
            board_generation: 0,
            browser,
            detail: RecordDetail::new(),
            focus: Focus::List,
            show_help: false,
            feedback: None,
            status: None,
            freshness: None,
            provisional: false,
            unavailable: BTreeMap::new(),
            resume_anchor: None,
            refresh_intent: None,
            interaction: None,
        }
    }

    pub(crate) fn from_projection(
        projection: UiProjection,
        resume: Option<WorkbenchResume>,
    ) -> Self {
        let mut app = Self::new(projection.scan, projection.derived);
        if let Some(diagnostics) = projection.catalogue_diagnostics {
            app.facts
                .replace_catalogue_diagnostics(&app.scan, diagnostics);
        }
        app.body_owners = projection.body_owners;
        app.provisional = projection.provisional;
        app.unavailable = projection.unavailable;
        if let Some(resume) = resume {
            app.browser.restore(resume.browser);
            app.detail.restore(resume.detail);
            app.focus = resume.focus;
            app.resume_anchor = Some(resume.anchor);
        }
        app.browser.apply_partial(&app.scan);
        app
    }

    pub(crate) fn resume(&self) -> WorkbenchResume {
        let board_paths = self.board_card_paths();
        WorkbenchResume {
            browser: self.browser.state(),
            anchor: self.browser.anchor(&self.scan, &board_paths),
            detail: self.detail.state(),
            focus: self.focus,
        }
    }

    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = Some(status.into());
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> io::Result<Interaction> {
        let mut dirty = true;
        while self.interaction.is_none() {
            if dirty {
                terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
                dirty = false;
            }
            if event::poll(EVENT_POLL_INTERVAL)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle(key.code);
                        dirty = true;
                    }
                    Event::Resize(_, _) => dirty = true,
                    _ => {}
                }
            }
        }
        Ok(self.interaction.take().unwrap_or(Interaction::Quit))
    }

    pub(crate) fn run_progressive(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        coordinator: &mut Coordinator,
    ) -> io::Result<(Interaction, WorkbenchResume)> {
        let mut dirty = true;
        while self.interaction.is_none() {
            let update = coordinator.drain();
            if update.projection != ProjectionChange::None {
                self.apply_projection(coordinator.take_projection(), update.projection);
            }
            if update.dirty {
                self.status = Some(coordinator.status().into());
                dirty = true;
            }
            if coordinator.request_content(self.browser.selected_path()) {
                self.status = Some(coordinator.status().into());
                dirty = true;
            }
            if dirty {
                terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
                dirty = false;
            }
            if event::poll(if coordinator.loading() {
                Duration::from_millis(1)
            } else {
                EVENT_POLL_INTERVAL
            })? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle(key.code);
                        if let Some(intent) = self.refresh_intent.take() {
                            let target = self.refresh_target(coordinator, intent);
                            match coordinator.refresh(target) {
                                Ok(()) => self.status = Some(coordinator.status().into()),
                                Err(message) => self.feedback = Some(message),
                            }
                        }
                        dirty = true;
                    }
                    Event::Resize(_, _) => dirty = true,
                    _ => {}
                }
            }
        }
        let interaction = self.interaction.take().unwrap_or(Interaction::Quit);
        Ok((interaction, self.resume()))
    }

    pub(crate) fn run_progressive_watched(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        coordinator: &mut Coordinator,
        watcher: &mut WatchCoordinator,
    ) -> io::Result<(Interaction, WorkbenchResume)> {
        let mut dirty = true;
        let mut catalogue_generation = None;
        while self.interaction.is_none() {
            dirty |= watcher.drain();
            let update = coordinator.drain();
            if update.projection != ProjectionChange::None {
                self.apply_projection(coordinator.take_projection(), update.projection);
                if (catalogue_generation != Some(coordinator.catalogue_generation())
                    || !watcher.has_catalogue())
                    && let Some(catalogue) = coordinator.catalogue()
                {
                    watcher.rebuild(catalogue);
                    catalogue_generation = Some(coordinator.catalogue_generation());
                }
            }
            if update.dirty {
                self.status = Some(coordinator.status().into());
                dirty = true;
            }
            if coordinator.request_content(self.browser.selected_path()) {
                self.status = Some(coordinator.status().into());
                dirty = true;
            }
            self.freshness = watcher.warning(&self.selected_scope());
            if dirty {
                terminal.draw(|frame| self.render(frame.area(), frame.buffer_mut()))?;
                dirty = false;
            }
            if event::poll(if coordinator.loading() {
                Duration::from_millis(1)
            } else {
                EVENT_POLL_INTERVAL
            })? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle(key.code);
                        if let Some(intent) = self.refresh_intent.take() {
                            let target = self.refresh_target(coordinator, intent);
                            match coordinator.refresh(target) {
                                Ok(()) => self.status = Some(coordinator.status().into()),
                                Err(message) => self.feedback = Some(message),
                            }
                        }
                        dirty = true;
                    }
                    Event::Resize(_, _) => dirty = true,
                    _ => {}
                }
            }
        }
        let interaction = self.interaction.take().unwrap_or(Interaction::Quit);
        Ok((interaction, self.resume()))
    }

    fn selected_scope(&self) -> SelectedScope {
        match self.browser.view() {
            View::Projects => SelectedScope::Store,
            View::Investigations => self
                .browser
                .selected_project()
                .map(|project| SelectedScope::Project {
                    project: project.into(),
                })
                .unwrap_or(SelectedScope::Store),
            View::Tickets | View::Files | View::Strategies | View::Boards => self
                .browser
                .selected_project()
                .zip(self.browser.selected_investigation())
                .map(|(project, identity)| SelectedScope::Investigation {
                    project: project.into(),
                    identity: identity.into(),
                })
                .or_else(|| {
                    self.browser
                        .selected_project()
                        .map(|project| SelectedScope::Project {
                            project: project.into(),
                        })
                })
                .unwrap_or(SelectedScope::Store),
        }
    }

    fn apply_projection(&mut self, projection: UiProjection, change: ProjectionChange) {
        let anchor = (change == ProjectionChange::Complete).then(|| {
            self.resume_anchor
                .clone()
                .unwrap_or_else(|| self.browser.anchor(&self.scan, &self.board_card_paths()))
        });
        let previous_revision = self
            .browser
            .selected(&self.scan)
            .map(|entry| entry.content_revision.clone());
        if !projection.incremental || self.boards_affected(&projection) {
            self.board_generation = self.board_generation.wrapping_add(1);
        }
        self.provisional = projection.provisional;
        if projection.incremental {
            if projection.catalogue_changed
                || !projection.scan.snapshot.entries.is_empty()
                || !projection.removed.is_empty()
                || !projection.relationship_updates.is_empty()
                || !projection.availability_changed.is_empty()
                || !projection.unavailable.is_empty()
            {
                self.merge_projection(projection);
            }
        } else {
            self.scan = projection.scan;
            self.derived = projection.derived;
            self.unavailable = projection.unavailable;
            self.body_owners = projection.body_owners;
            self.browser.rebuild(&self.scan);
            self.facts.rebuild(&self.scan);
            if let Some(diagnostics) = projection.catalogue_diagnostics {
                self.facts
                    .replace_catalogue_diagnostics(&self.scan, diagnostics);
            }
            self.scan.diagnostics.clear();
            self.derived.diagnostics.clear();
            self.relationships.clear();
            for edge in std::mem::take(&mut self.derived.relationships) {
                self.relationships
                    .entry(edge.source.clone())
                    .or_default()
                    .push(edge);
            }
            self.board_records = std::mem::take(&mut self.derived.boards)
                .into_iter()
                .map(|board| (board.identity.clone(), board))
                .collect();
            self.detail.invalidate();
            self.reindex();
        }
        match change {
            ProjectionChange::Complete => {
                self.resume_anchor = None;
                let board_paths = self.board_card_paths();
                self.feedback = match self.browser.promote(
                    &self.scan,
                    anchor.as_ref().expect("complete anchor"),
                    &board_paths,
                ) {
                    Some(PromotionNotice::FilteredOut) => {
                        Some("Selected item no longer matches the filter.".into())
                    }
                    Some(PromotionNotice::Ambiguous) => {
                        Some("Selected governed identity is ambiguous; selection cleared.".into())
                    }
                    None => None,
                };
            }
            ProjectionChange::Partial => self.browser.apply_partial(&self.scan),
            ProjectionChange::Content => {}
            ProjectionChange::None => unreachable!("projection changes are applied explicitly"),
        }
        let current_revision = self
            .browser
            .selected(&self.scan)
            .map(|entry| entry.content_revision.clone());
        if previous_revision != current_revision
            && !(change == ProjectionChange::Partial && current_revision.is_none())
        {
            self.detail.reset_scroll();
        }
    }

    fn refresh_target(
        &self,
        coordinator: &Coordinator,
        intent: RefreshIntent,
    ) -> PresentationTarget {
        if intent == RefreshIntent::Store {
            return PresentationTarget::Store;
        }
        match self.browser.view() {
            View::Projects => self
                .browser
                .selected_project()
                .map(|project| PresentationTarget::Project {
                    project: project.into(),
                })
                .unwrap_or(PresentationTarget::Store),
            View::Investigations
            | View::Tickets
            | View::Files
            | View::Strategies
            | View::Boards => self
                .browser
                .selected_project()
                .zip(self.browser.selected_investigation())
                .and_then(|(project, investigation)| {
                    coordinator.investigation_target(project, investigation)
                })
                .unwrap_or(PresentationTarget::Store),
        }
    }

    pub(crate) fn handle(&mut self, key: KeyCode) {
        if self.show_help {
            match key {
                KeyCode::Char('q') => self.interaction = Some(Interaction::Quit),
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => self.show_help = false,
                _ => {}
            }
            return;
        }
        if self.browser.is_entering_filter() {
            let selection_changed = match key {
                KeyCode::Esc | KeyCode::Enter => {
                    self.browser.close_filter();
                    false
                }
                KeyCode::Backspace => self.browser.pop_filter(&self.scan),
                KeyCode::Char(character) => self.browser.push_filter(&self.scan, character),
                _ => false,
            };
            if selection_changed {
                self.detail.reset_scroll();
            }
            return;
        }
        self.feedback = None;
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.interaction = Some(Interaction::Quit),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('1') => self.set_view(View::Projects),
            KeyCode::Char('2') => self.set_view(View::Investigations),
            KeyCode::Char('3') => self.set_view(View::Tickets),
            KeyCode::Char('4') => self.set_view(View::Files),
            KeyCode::Char('5') => self.set_view(View::Strategies),
            KeyCode::Char('6') => self.set_view(View::Boards),
            KeyCode::Char('t') => {
                self.browser.cycle_view(&self.scan);
                if self.browser.view() == View::Boards {
                    self.select_board(0);
                }
                self.detail.reset_scroll();
            }
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Enter => self.drill_down(),
            KeyCode::Backspace => self.go_up(),
            KeyCode::Char('/') => self.browser.start_filter(),
            KeyCode::Char('c') => self.clear_filter(),
            KeyCode::Left | KeyCode::Char('h') => self.detail.select_tab(-1),
            KeyCode::Right | KeyCode::Char('l') => self.detail.select_tab(1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focus(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_focus(-1),
            KeyCode::PageDown => self.move_focus(PAGE_SIZE),
            KeyCode::PageUp => self.move_focus(-PAGE_SIZE),
            KeyCode::Home => self.move_to_edge(false),
            KeyCode::End => self.move_to_edge(true),
            KeyCode::Char('e') => self.request_edit(),
            KeyCode::Char('r') => self.refresh_intent = Some(RefreshIntent::Current),
            KeyCode::Char('R') => self.refresh_intent = Some(RefreshIntent::Store),
            _ => {}
        }
    }

    fn set_view(&mut self, view: View) {
        self.browser.set_view(&self.scan, view);
        if view == View::Boards {
            self.select_board(0);
        }
        self.detail.reset_scroll();
    }

    fn clear_filter(&mut self) {
        if self.browser.clear_filter(&self.scan) {
            self.detail.reset_scroll();
        }
    }

    fn drill_down(&mut self) {
        if self.focus == Focus::List && self.browser.drill_down(&self.scan) {
            self.detail.reset_scroll();
        }
    }

    fn go_up(&mut self) {
        if self.focus == Focus::List && self.browser.go_up(&self.scan) {
            self.detail.reset_scroll();
        }
    }

    fn move_focus(&mut self, offset: isize) {
        match self.focus {
            Focus::List => {
                let changed = if self.browser.view() == View::Boards {
                    self.select_board(offset)
                } else {
                    self.browser.select_offset(&self.scan, offset)
                };
                if changed {
                    self.detail.reset_scroll();
                }
            }
            Focus::Detail => self.detail.scroll(offset),
        }
    }

    fn move_to_edge(&mut self, end: bool) {
        match self.focus {
            Focus::List => {
                let changed = if self.browser.view() == View::Boards {
                    self.select_board(if end { isize::MAX } else { isize::MIN })
                } else {
                    self.browser.select_edge(&self.scan, end)
                };
                if changed {
                    self.detail.reset_scroll();
                }
            }
            Focus::Detail => self.detail.move_to_edge(end),
        }
    }

    fn request_edit(&mut self) {
        if self.browser.view() == View::Boards {
            self.feedback = Some("Read-only".into());
            return;
        }
        match edit_selection(self.browser.selected(&self.scan)) {
            Ok(interaction) => self.interaction = Some(interaction),
            Err(feedback) => self.feedback = Some(feedback.into()),
        }
    }

    pub(crate) fn render(&self, area: Rect, buffer: &mut Buffer) {
        let [header, status, body, footer] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .areas(area);
        self.browser.render_header(
            &self.scan,
            self.board_count(),
            self.facts.diagnostic_count(),
            header,
            buffer,
        );
        let mut status_text = self.status.clone().unwrap_or_else(|| {
            if self.provisional {
                "Loading…".into()
            } else {
                String::new()
            }
        });
        if self.provisional
            && self.browser.selected_path().is_some()
            && self.browser.selected(&self.scan).is_none()
        {
            status_text.push_str("  |  Selected item is still loading...");
        }
        if let Some(fields) = self
            .browser
            .selected_path()
            .and_then(|path| self.unavailable.get(path))
        {
            if !status_text.is_empty() {
                status_text.push_str("  |  ");
            }
            status_text.push_str(&format!("Unavailable: {fields}"));
        }
        if let Some(freshness) = &self.freshness {
            if !status_text.is_empty() {
                status_text.push_str("  |  ");
            }
            status_text.push_str(freshness);
        }
        Paragraph::new(status_text)
            .style(
                Style::default().fg(if self.provisional || self.freshness.is_some() {
                    WARN
                } else {
                    MUTED
                }),
            )
            .render(status, buffer);

        match layout_mode(body) {
            LayoutMode::Wide => {
                let [list, detail] = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(64), Constraint::Min(32)])
                    .areas(body);
                self.render_body(list, detail, buffer);
            }
            LayoutMode::Narrow => {
                let [list, detail] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                    .areas(body);
                self.render_body(list, detail, buffer);
            }
        }

        let footer_text = if self.browser.is_entering_filter() {
            " Type to filter  Enter accept  Esc close "
        } else if let Some(feedback) = &self.feedback {
            feedback
        } else {
            " 1-6 views  Enter open  Backspace up  Tab focus  j/k move  h/l detail  e edit  r/R refresh  / filter  ? help  q quit "
        };
        Paragraph::new(footer_text)
            .style(Style::default().fg(MUTED))
            .render(footer, buffer);

        if self.show_help {
            render_help(area, buffer);
        }
    }

    fn render_body(&self, list: Rect, detail: Rect, buffer: &mut Buffer) {
        let selected = self.browser.selected(&self.scan);
        let derived = if self.derived.source_revision == self.scan.snapshot.revision {
            selected.and_then(|entry| {
                self.record_indices
                    .get(&entry.path)
                    .and_then(|index| self.derived.records.get(*index))
            })
        } else {
            None
        };
        if self.browser.view() == View::Boards {
            self.render_boards(list, buffer);
        } else {
            self.browser
                .render_list(&self.scan, self.focus == Focus::List, list, buffer);
        }
        self.detail.render(
            crate::record_detail::DetailRecord {
                entry: selected,
                derived,
                bytes: self
                    .browser
                    .selected_path()
                    .and_then(|path| self.body_bytes(path)),
                diagnostics: self
                    .facts
                    .diagnostics(selected.map(|entry| entry.path.as_str())),
            },
            self.focus == Focus::Detail,
            detail,
            buffer,
        );
    }

    fn body_bytes(&self, path: &str) -> Option<&[u8]> {
        if let Some(entry) = self.body_owners.get(path) {
            return match &entry.body {
                casefile_store::PresentationFact::Available(bytes) => Some(bytes.as_slice()),
                casefile_store::PresentationFact::Unavailable => None,
            };
        }
        self.entry_indices
            .get(path)
            .and_then(|index| self.scan.snapshot.entries.get(*index))
            .map(|entry| entry.original_bytes.as_slice())
    }

    fn prepare_boards(&self) {
        self.boards.borrow_mut().prepare(
            self.board_generation,
            self.browser.scope(),
            &self.scan,
            &self.derived.source_revision,
            &self.board_records,
            &self.facts,
        );
    }

    fn board_card_paths(&self) -> Vec<String> {
        self.prepare_boards();
        self.boards.borrow().paths.clone()
    }

    fn select_board(&mut self, offset: isize) -> bool {
        self.prepare_boards();
        self.browser
            .select_board_offset(&self.boards.borrow().paths, offset)
    }

    fn board_count(&self) -> usize {
        self.prepare_boards();
        self.boards.borrow().count
    }

    fn render_boards(&self, area: Rect, buffer: &mut Buffer) {
        self.prepare_boards();
        self.boards.borrow_mut().render(
            self.browser.selected_path(),
            self.focus == Focus::List,
            area,
            buffer,
        );
    }
}

fn render_help(area: Rect, buffer: &mut Buffer) {
    let popup = centred(area, 68, 20);
    Clear.render(popup, buffer);
    let lines = vec![
        Line::from("MOVE").style(Style::default().fg(ACCENT).bold()),
        help_line("j / k, Up / Down", "Move selection or scroll focused pane"),
        help_line("PgUp / PgDn", "Page through the focused pane"),
        help_line("Home / End", "Jump to the first or last position"),
        help_line("Tab", "Switch focus between list and detail"),
        Line::from(""),
        Line::from("VIEW").style(Style::default().fg(ACCENT).bold()),
        help_line(
            "1 / 2 / 3 / 4 / 5 / 6",
            "Open Projects, Investigations, Tickets, Files, Strategies, or Boards",
        ),
        help_line(
            "Enter / Backspace",
            "Drill into the selected scope or go up",
        ),
        help_line(
            "h / l, Left / Right",
            "Switch Overview, Rendered, Source, Diagnostics",
        ),
        help_line("/", "Enter filter mode"),
        help_line("c", "Clear the active filter"),
        help_line("e", "Edit selected governed ticket, epic, or board"),
        help_line("r / R", "Refresh current scope or the whole Store"),
        Line::from(""),
        help_line("? / Esc / Enter", "Close this help"),
        help_line("q", "Quit Casefile"),
    ];
    Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Keyboard help ")
                .title_style(Style::default().fg(ACCENT).bold())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT)),
        )
        .wrap(Wrap { trim: false })
        .render(popup, buffer);
}

fn help_line(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<18}"), Style::default().fg(WARN)),
        Span::raw(description.to_owned()),
    ])
}

fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn layout_mode(area: Rect) -> LayoutMode {
    if area.width >= WIDE_MINIMUM {
        LayoutMode::Wide
    } else {
        LayoutMode::Narrow
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod progressive_tests;

#[cfg(test)]
mod relationship_tests;

#[cfg(test)]
mod flow_tests;

enum CardPathResolution {
    Resolved(String),
    Missing,
    Ambiguous,
}
