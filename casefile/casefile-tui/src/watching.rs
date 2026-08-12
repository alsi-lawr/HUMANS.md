use crate::{ObservationHandoff, RefreshMinimumScope, RefreshObservation, RefreshReport};
use casefile_store::{PresentationCatalogue, PresentationTarget, is_store_path_excluded};
use notify::{Config, Event, EventKind, EventKindMask, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RETRY_INTERVAL: Duration = Duration::from_millis(250);
const REDRAW_COALESCE: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Eq, PartialEq)]
enum ScopeImpact {
    Store,
    Project { project: String },
    Investigation { project: String, identity: String },
    Ignore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatchHealth {
    Healthy,
    Degraded,
}

#[derive(Debug)]
enum NativeObservation {
    Event(Event),
    Health { health: WatchHealth, reason: String },
}

enum Control {
    Stop,
    Retry,
}

/// Owns the single native recursive subscription. Dropping it synchronously unregisters the
/// callback and joins the adapter thread; it never waits for a presentation loader.
struct NativeSubscription {
    observations: Receiver<NativeObservation>,
    control: Sender<Control>,
    worker: Option<JoinHandle<()>>,
}

impl NativeSubscription {
    fn start(root: PathBuf) -> (Self, WatchHealth) {
        let (observation_tx, observation_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker =
            thread::spawn(move || run_native_adapter(root, observation_tx, control_rx, ready_tx));
        let initial = ready_rx.recv().unwrap_or(WatchHealth::Degraded);
        (
            Self {
                observations: observation_rx,
                control: control_tx,
                worker: Some(worker),
            },
            initial,
        )
    }
}

impl Drop for NativeSubscription {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_native_adapter(
    root: PathBuf,
    output: Sender<NativeObservation>,
    control: Receiver<Control>,
    ready: mpsc::SyncSender<WatchHealth>,
) {
    let (backend_tx, backend_rx) = mpsc::channel();
    let mut first = true;
    loop {
        match control.try_recv() {
            Ok(Control::Stop) | Err(TryRecvError::Disconnected) => return,
            Ok(Control::Retry) | Err(TryRecvError::Empty) => {}
        }
        let callback_tx = backend_tx.clone();
        let attempt = RecommendedWatcher::new(
            move |event| {
                let _ = callback_tx.send(event);
            },
            Config::default().with_event_kinds(EventKindMask::CORE),
        )
        .and_then(|mut watcher| {
            if std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
                return Err(notify::Error::generic("Store root must not be a symlink"));
            }
            watcher.watch(&root, RecursiveMode::Recursive)?;
            Ok(watcher)
        });

        let mut watcher = match attempt {
            Ok(watcher) => {
                if first {
                    let _ = ready.send(WatchHealth::Healthy);
                    first = false;
                } else {
                    let _ = output.send(NativeObservation::Health {
                        health: WatchHealth::Healthy,
                        reason: "native filesystem observation restored".into(),
                    });
                }
                watcher
            }
            Err(error) => {
                if first {
                    let _ = ready.send(WatchHealth::Degraded);
                    first = false;
                }
                let _ = output.send(NativeObservation::Health {
                    health: WatchHealth::Degraded,
                    reason: format!("native filesystem observation unavailable: {error}"),
                });
                match control.recv_timeout(RETRY_INTERVAL) {
                    Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    Ok(Control::Retry) | Err(RecvTimeoutError::Timeout) => continue,
                }
            }
        };

        let mut disconnected = None;
        loop {
            match control.recv_timeout(Duration::from_millis(20)) {
                Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = watcher.unwatch(&root);
                    return;
                }
                Ok(Control::Retry) => {
                    disconnected = Some("configured Store root changed".into());
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            if disconnected.is_some() {
                break;
            }
            while let Ok(event) = backend_rx.try_recv() {
                match event {
                    Ok(event) => {
                        if output.send(NativeObservation::Event(event)).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        disconnected = Some(error.to_string());
                        break;
                    }
                }
            }
            if disconnected.is_some() {
                break;
            }
        }
        let _ = watcher.unwatch(&root);
        drop(watcher);
        let _ = output.send(NativeObservation::Health {
            health: WatchHealth::Degraded,
            reason: format!(
                "native filesystem observation disconnected: {}",
                disconnected.unwrap_or_else(|| "unknown backend error".into())
            ),
        });
        match control.recv_timeout(RETRY_INTERVAL) {
            Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Control::Retry) | Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[derive(Clone, Debug)]
struct ActivationRoot {
    relative: String,
    project: String,
    identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootNamespace {
    DotRelative,
    FilesystemRoot,
    Scoped,
}

#[derive(Clone, Debug)]
struct PathClassifier {
    root: String,
    root_namespace: RootNamespace,
    windows: bool,
    activation_roots: Vec<ActivationRoot>,
    projects: BTreeMap<String, String>,
    catalogue_known: bool,
}

impl PathClassifier {
    fn native(root: &Path) -> Self {
        Self::new(root.to_string_lossy().as_ref(), cfg!(windows))
    }

    fn new(root: &str, windows: bool) -> Self {
        let normalized =
            lexical(root, windows).unwrap_or_else(|| normalize_separators(root, windows));
        let root_namespace = if normalized.is_empty() {
            if is_lexically_absolute(root, windows) {
                RootNamespace::FilesystemRoot
            } else {
                RootNamespace::DotRelative
            }
        } else {
            RootNamespace::Scoped
        };
        Self {
            root: normalized,
            root_namespace,
            windows,
            activation_roots: Vec::new(),
            projects: BTreeMap::new(),
            catalogue_known: false,
        }
    }

    fn rebuild(&mut self, catalogue: &PresentationCatalogue) {
        self.projects = catalogue
            .projects
            .iter()
            .map(|project| {
                (
                    if self.windows {
                        project.slug.to_ascii_lowercase()
                    } else {
                        project.slug.clone()
                    },
                    project.slug.clone(),
                )
            })
            .collect();
        self.activation_roots = catalogue
            .projects
            .iter()
            .flat_map(|project| {
                project.investigations.iter().filter_map(|investigation| {
                    lexical(&investigation.path, self.windows).map(|relative| ActivationRoot {
                        relative: relative.trim_matches('/').into(),
                        project: project.slug.clone(),
                        identity: investigation.identity.clone(),
                    })
                })
            })
            .collect();
        self.activation_roots.sort_by(|left, right| {
            right
                .relative
                .len()
                .cmp(&left.relative.len())
                .then_with(|| left.relative.cmp(&right.relative))
        });
        self.catalogue_known = true;
    }

    fn classify(&self, path: &Path) -> Result<ScopeImpact, ()> {
        let value = path.to_str().ok_or(())?;
        self.classify_str(value)
    }

    fn is_root(&self, path: &Path) -> bool {
        let Some(value) = path.to_str() else {
            return false;
        };
        let Some(normalized) = lexical(value, self.windows) else {
            return false;
        };
        matches!(self.root_relative(value, &normalized), Ok(Some("")))
    }

    fn classify_str(&self, value: &str) -> Result<ScopeImpact, ()> {
        let normalized = lexical(value, self.windows).ok_or(())?;
        let Some(relative) = self.root_relative(value, &normalized)? else {
            return Ok(ScopeImpact::Ignore);
        };
        if relative.is_empty() {
            return Ok(ScopeImpact::Store);
        }
        if is_store_path_excluded(Path::new(relative)) {
            return Ok(ScopeImpact::Ignore);
        }
        if matches!(relative, "casefile.toml" | "projects.toml") {
            return Ok(ScopeImpact::Store);
        }
        if !self.catalogue_known {
            return Ok(ScopeImpact::Store);
        }
        for root in &self.activation_roots {
            if relative == root.relative || strip_lexical_prefix(relative, &root.relative).is_some()
            {
                return Ok(ScopeImpact::Investigation {
                    project: root.project.clone(),
                    identity: root.identity.clone(),
                });
            }
        }
        let mut parts = relative.split('/');
        if parts.next() == Some("projects")
            && let Some(project) = parts.next()
        {
            let lookup = if self.windows {
                project.to_ascii_lowercase()
            } else {
                project.into()
            };
            if let Some(project) = self.projects.get(&lookup) {
                return Ok(ScopeImpact::Project {
                    project: project.clone(),
                });
            }
        }
        Ok(ScopeImpact::Store)
    }

    fn root_relative<'a>(&self, value: &str, normalized: &'a str) -> Result<Option<&'a str>, ()> {
        match self.root_namespace {
            RootNamespace::DotRelative => {
                if is_windows_drive_relative(value, self.windows) {
                    return Err(());
                }
                if is_lexically_absolute(value, self.windows) {
                    return Ok(None);
                }
                Ok(Some(normalized))
            }
            RootNamespace::FilesystemRoot => {
                if is_windows_drive_relative(value, self.windows) {
                    return Err(());
                }
                if self.windows && is_windows_qualified_absolute(value) {
                    return Ok(None);
                }
                Ok(Some(normalized.strip_prefix('/').unwrap_or(normalized)))
            }
            RootNamespace::Scoped => {
                if normalized == self.root {
                    Ok(Some(""))
                } else {
                    Ok(strip_lexical_prefix(normalized, &self.root))
                }
            }
        }
    }

    fn investigation_identity(&self, project: &str, path: &str) -> Option<String> {
        let path = lexical(path, self.windows)?;
        self.activation_roots
            .iter()
            .find(|root| root.project == project && root.relative == path)
            .map(|root| root.identity.clone())
    }
}

fn is_lexically_absolute(value: &str, windows: bool) -> bool {
    if !windows {
        // This branch models a POSIX namespace even when the pure classifier tests execute on a
        // Windows host. `Path::is_absolute` applies host rules, where `/` is rooted but not fully
        // qualified, and would collapse POSIX filesystem-root and dot-relative namespaces.
        return value.starts_with('/');
    }
    let value = value.replace('\\', "/");
    value.starts_with('/')
        || (value.as_bytes().get(1) == Some(&b':') && value.as_bytes().get(2) == Some(&b'/'))
}

fn is_windows_drive_relative(value: &str, windows: bool) -> bool {
    if !windows {
        return false;
    }
    let value = value.replace('\\', "/");
    value.as_bytes().get(1) == Some(&b':') && value.as_bytes().get(2) != Some(&b'/')
}

fn is_windows_qualified_absolute(value: &str) -> bool {
    let value = value.replace('\\', "/");
    value.starts_with("//")
        || (value.as_bytes().get(1) == Some(&b':') && value.as_bytes().get(2) == Some(&b'/'))
}

fn normalize_separators(value: &str, windows: bool) -> String {
    let value = if windows {
        value.replace('\\', "/")
    } else {
        value.into()
    };
    if windows {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn lexical(value: &str, windows: bool) -> Option<String> {
    let normalized = normalize_separators(value, windows);
    let mut prefix = String::new();
    let mut components = Vec::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Prefix(value) => prefix = value.as_os_str().to_str()?.into(),
            Component::RootDir => prefix.push('/'),
            Component::CurDir => {}
            Component::Normal(value) => components.push(value.to_str()?.to_owned()),
            Component::ParentDir => return None,
        }
    }
    let joined = components.join("/");
    if prefix.is_empty() {
        Some(joined)
    } else if joined.is_empty() {
        Some(prefix.trim_end_matches('/').to_owned())
    } else {
        Some(format!("{}/{joined}", prefix.trim_end_matches('/')))
    }
}

fn strip_lexical_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    path.strip_prefix(root)?.strip_prefix('/')
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StaleRecord {
    generation: u64,
    impact: ScopeImpact,
    direct_clears: BTreeSet<(String, String)>,
    degraded: bool,
    reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SelectedScope {
    Store,
    Project { project: String },
    Investigation { project: String, identity: String },
}

struct FreshnessReducer {
    generation: u64,
    records: Vec<StaleRecord>,
    health: WatchHealth,
    started: BTreeMap<u64, (PresentationTarget, u64)>,
}

impl FreshnessReducer {
    fn new(initial: WatchHealth) -> Self {
        let mut reducer = Self {
            generation: 0,
            records: Vec::new(),
            health: initial,
            started: BTreeMap::new(),
        };
        if initial == WatchHealth::Degraded {
            reducer.mark(
                ScopeImpact::Store,
                true,
                "native filesystem observation unavailable".into(),
            );
        }
        reducer
    }

    fn mark(&mut self, impact: ScopeImpact, degraded: bool, reason: String) -> bool {
        if impact == ScopeImpact::Ignore {
            return false;
        }
        self.generation = self.generation.saturating_add(1);
        self.records.push(StaleRecord {
            generation: self.generation,
            impact,
            direct_clears: BTreeSet::new(),
            degraded,
            reason,
        });
        true
    }

    fn health(&mut self, health: WatchHealth, reason: String) -> bool {
        if self.health == health {
            return false;
        }
        self.health = health;
        self.generation = self.generation.saturating_add(1);
        if health == WatchHealth::Degraded {
            self.records.push(StaleRecord {
                generation: self.generation,
                impact: ScopeImpact::Store,
                direct_clears: BTreeSet::new(),
                degraded: true,
                reason,
            });
        }
        true
    }

    fn observation(&self) -> RefreshObservation {
        let store_reason = self.records.iter().rev().find_map(|record| {
            (record.degraded || record.impact == ScopeImpact::Store).then(|| record.reason.clone())
        });
        RefreshObservation {
            generation: self.generation,
            minimum_scope: store_reason.map_or(RefreshMinimumScope::Contextual, |reason| {
                RefreshMinimumScope::Store { reason }
            }),
        }
    }

    fn report(&mut self, report: RefreshReport, classifier: &PathClassifier) -> bool {
        match report {
            RefreshReport::Started {
                generation,
                target,
                observation_generation,
            } => {
                self.started.clear();
                self.started
                    .insert(generation, (target, observation_generation));
                false
            }
            RefreshReport::Failed { generation, .. } => {
                self.started.remove(&generation);
                false
            }
            RefreshReport::Succeeded {
                generation,
                target,
                started_observation_generation,
                completed_observation_generation,
            } => {
                let valid =
                    self.started
                        .remove(&generation)
                        .is_some_and(|(started_target, started)| {
                            started_target == target && started == started_observation_generation
                        });
                if !valid || started_observation_generation != completed_observation_generation {
                    return false;
                }
                let before = self.records.clone();
                match target {
                    PresentationTarget::Store => {
                        self.records.retain(|record| {
                            record.generation > started_observation_generation
                                || (record.degraded && self.health == WatchHealth::Degraded)
                        });
                    }
                    PresentationTarget::Project { project } => {
                        self.records.retain(|record| {
                            record.generation > started_observation_generation
                                || !matches!(&record.impact, ScopeImpact::Project { project: stale }
                                    | ScopeImpact::Investigation { project: stale, .. } if stale == &project)
                        });
                    }
                    PresentationTarget::Investigation { project, path } => {
                        if let Some(identity) = classifier.investigation_identity(&project, &path) {
                            for record in &mut self.records {
                                if record.generation <= started_observation_generation {
                                    record
                                        .direct_clears
                                        .insert((project.clone(), identity.clone()));
                                }
                            }
                        }
                    }
                }
                before != self.records
            }
        }
    }

    fn warning(&self, scope: &SelectedScope) -> Option<String> {
        let relevant = self
            .records
            .iter()
            .filter(|record| match (scope, &record.impact) {
                (_, ScopeImpact::Store) => true,
                (
                    SelectedScope::Store,
                    ScopeImpact::Project { .. } | ScopeImpact::Investigation { .. },
                ) => true,
                (
                    SelectedScope::Project { project },
                    ScopeImpact::Project { project: stale }
                    | ScopeImpact::Investigation { project: stale, .. },
                ) => project == stale,
                (
                    SelectedScope::Investigation { project, .. },
                    ScopeImpact::Project { project: stale }
                    | ScopeImpact::Investigation { project: stale, .. },
                ) => project == stale,
                _ => false,
            })
            .collect::<Vec<_>>();
        if relevant.is_empty() {
            return None;
        }
        if let Some(record) = relevant.iter().rev().find(|record| record.degraded) {
            return Some(format!(
                "DEGRADED: {}; data may be stale; press R to refresh the Store",
                record.reason
            ));
        }
        let direct = matches!(scope, SelectedScope::Investigation { project, identity }
            if relevant.iter().any(|record| matches!(&record.impact,
                ScopeImpact::Investigation { project: stale_project, identity: stale_identity }
                    if stale_project == project && stale_identity == identity)
                && !record.direct_clears.contains(&(project.clone(), identity.clone()))));
        let inherited = matches!(scope, SelectedScope::Investigation { .. }) && !direct;
        let minimum = if self
            .records
            .iter()
            .any(|record| record.impact == ScopeImpact::Store)
        {
            "R (Store)"
        } else {
            "r/R"
        };
        let projects = relevant
            .iter()
            .filter_map(|record| match &record.impact {
                ScopeImpact::Project { project } | ScopeImpact::Investigation { project, .. } => {
                    Some(project.as_str())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let project_note = if matches!(scope, SelectedScope::Store) && !projects.is_empty() {
            format!(" (project {})", projects.join(", "))
        } else {
            String::new()
        };
        Some(format!(
            "STALE {}{project_note}: disk changed; refresh with {minimum}",
            if direct {
                "DIRECT"
            } else if inherited {
                "INHERITED"
            } else {
                "SCOPE"
            }
        ))
    }
}

pub(crate) struct WatchCoordinator {
    native: NativeSubscription,
    classifier: PathClassifier,
    reducer: FreshnessReducer,
    observation_tx: Sender<RefreshObservation>,
    report_rx: Receiver<RefreshReport>,
    redraw_at: Option<Instant>,
}

impl WatchCoordinator {
    pub(crate) fn start(root: &Path) -> (Self, ObservationHandoff, RefreshObservation) {
        let classifier = PathClassifier::native(root);
        let (native, health) = NativeSubscription::start(root.to_owned());
        let reducer = FreshnessReducer::new(health);
        let (observation_tx, observation_rx) = mpsc::channel();
        let (report_tx, report_rx) = mpsc::channel();
        let mut coordinator = Self {
            native,
            classifier,
            reducer,
            observation_tx,
            report_rx,
            redraw_at: None,
        };
        // Callback events already delivered after subscription are covered by the generation
        // captured immediately before the initial Store load.
        let _ = coordinator.drain();
        let initial = coordinator.reducer.observation();
        (
            coordinator,
            ObservationHandoff::new(observation_rx, report_tx),
            initial,
        )
    }

    pub(crate) fn rebuild(&mut self, catalogue: &PresentationCatalogue) {
        self.classifier.rebuild(catalogue);
    }

    pub(crate) fn drain(&mut self) -> bool {
        let was_fresh = self.reducer.records.is_empty();
        let mut changed = false;
        while let Ok(report) = self.report_rx.try_recv() {
            changed |= self.reducer.report(report, &self.classifier);
        }
        while let Ok(observation) = self.native.observations.try_recv() {
            match observation {
                NativeObservation::Health { health, reason } => {
                    changed |= self.reducer.health(health, reason)
                }
                NativeObservation::Event(event) => {
                    if matches!(event.kind, EventKind::Access(_)) {
                        continue;
                    }
                    if event.need_rescan() {
                        changed |= self.reducer.mark(
                            ScopeImpact::Store,
                            true,
                            "filesystem observation overflowed; events may have been lost".into(),
                        );
                        continue;
                    }
                    if event.paths.is_empty() {
                        changed |= self.reducer.mark(
                            ScopeImpact::Store,
                            true,
                            "filesystem event could not be classified".into(),
                        );
                    }
                    let root_lost =
                        matches!(
                            event.kind,
                            EventKind::Remove(_)
                                | EventKind::Modify(notify::event::ModifyKind::Name(_))
                        ) && event.paths.iter().any(|path| self.classifier.is_root(path));
                    if root_lost {
                        changed |= self.reducer.health(
                            WatchHealth::Degraded,
                            "configured Store root was removed or renamed".into(),
                        );
                        let _ = self.native.control.send(Control::Retry);
                    }
                    for path in event.paths {
                        match self.classifier.classify(&path) {
                            Ok(impact) => {
                                changed |= self.reducer.mark(
                                    impact,
                                    false,
                                    "Store files changed on disk".into(),
                                )
                            }
                            Err(()) => {
                                changed |= self.reducer.mark(
                                    ScopeImpact::Store,
                                    true,
                                    "filesystem event path could not be classified".into(),
                                )
                            }
                        }
                    }
                }
            }
        }
        if changed {
            let _ = self.observation_tx.send(self.reducer.observation());
            if was_fresh && !self.reducer.records.is_empty() {
                self.redraw_at = Some(Instant::now() + REDRAW_COALESCE);
                return true;
            }
            self.redraw_at = Some(Instant::now() + REDRAW_COALESCE);
        }
        if self
            .redraw_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.redraw_at = None;
            return true;
        }
        false
    }

    pub(crate) fn warning(&self, scope: &SelectedScope) -> Option<String> {
        self.reducer.warning(scope)
    }

    #[cfg(test)]
    fn fake(
        root: &Path,
    ) -> (
        Self,
        Sender<NativeObservation>,
        Receiver<RefreshObservation>,
        Sender<RefreshReport>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (native_tx, native_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_worker = Arc::clone(&stopped);
        let worker = thread::spawn(move || {
            let _ = control_rx.recv();
            stopped_worker.store(true, Ordering::SeqCst);
        });
        let native = NativeSubscription {
            observations: native_rx,
            control: control_tx,
            worker: Some(worker),
        };
        let (observation_tx, observation_rx) = mpsc::channel();
        let (report_tx, report_rx) = mpsc::channel();
        (
            Self {
                native,
                classifier: PathClassifier::native(root),
                reducer: FreshnessReducer::new(WatchHealth::Healthy),
                observation_tx,
                report_rx,
                redraw_at: None,
            },
            native_tx,
            observation_rx,
            report_tx,
            stopped,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casefile_store::{ActivationState, PresentationInvestigation, PresentationProject};
    use notify::event::{Flag, ModifyKind, RenameMode};
    use std::sync::atomic::Ordering;

    fn catalogue() -> PresentationCatalogue {
        PresentationCatalogue {
            activation: ActivationState::Active,
            projects: vec![PresentationProject {
                slug: "alpha".into(),
                prefix: "A".into(),
                investigations: vec![
                    PresentationInvestigation {
                        identity: "short".into(),
                        path: "projects/alpha/investigations".into(),
                    },
                    PresentationInvestigation {
                        identity: "deep".into(),
                        path: "projects/alpha/investigations/deep".into(),
                    },
                ],
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn classifier_is_lexical_longest_root_and_exact_root_exclusions() {
        let mut classifier = PathClassifier::new("/store", false);
        assert_eq!(
            classifier.classify_str("/store/anything"),
            Ok(ScopeImpact::Store)
        );
        classifier.rebuild(&catalogue());
        assert_eq!(
            classifier.classify_str("/store/.git/config"),
            Ok(ScopeImpact::Ignore)
        );
        assert_eq!(classifier.classify_str("/store"), Ok(ScopeImpact::Store));
        assert_eq!(
            classifier.classify_str("/store/casefile.toml"),
            Ok(ScopeImpact::Store)
        );
        assert_eq!(
            classifier.classify_str("/store/projects.toml"),
            Ok(ScopeImpact::Store)
        );
        assert!(
            matches!(classifier.classify_str("/store/projects/alpha/investigations/deep/.git/x"), Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep")
        );
        assert!(
            matches!(classifier.classify_str("/store/projects/alpha/investigations/deep/tickets/A-1.md"), Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep")
        );
        assert_eq!(
            classifier.classify_str("/outside/x"),
            Ok(ScopeImpact::Ignore)
        );
        assert_eq!(classifier.classify_str("/store/../escape"), Err(()));
    }

    #[test]
    fn classifier_models_windows_case_and_backslash_equivalence() {
        let mut classifier = PathClassifier::new(r"C:\Store", true);
        classifier.rebuild(&catalogue());
        assert!(
            matches!(classifier.classify_str(r"c:\STORE\projects\alpha\investigations\deep\x.md"), Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep")
        );
        assert_eq!(
            classifier.classify_str(r"c:\storehouse\x"),
            Ok(ScopeImpact::Ignore)
        );
    }

    #[test]
    fn classifier_keeps_dot_root_relative_and_absolute_namespaces_distinct() {
        let mut classifier = PathClassifier::new(".", false);
        assert_eq!(classifier.root_namespace, RootNamespace::DotRelative);
        classifier.rebuild(&catalogue());
        assert!(matches!(
            classifier.classify_str("./projects/alpha/investigations/deep/ticket.md"),
            Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep"
        ));
        assert_eq!(
            classifier.classify_str("./.agent-workspace/session/log"),
            Ok(ScopeImpact::Ignore)
        );
        assert_eq!(
            classifier.classify_str("/outside/projects/alpha/investigations/deep/ticket.md"),
            Ok(ScopeImpact::Ignore)
        );
        assert_eq!(classifier.classify_str("../outside"), Err(()));

        let mut windows = PathClassifier::new(".", true);
        windows.rebuild(&catalogue());
        assert!(matches!(
            windows.classify_str(r".\PROJECTS\ALPHA\INVESTIGATIONS\DEEP\ticket.md"),
            Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep"
        ));
        assert_eq!(
            windows.classify_str(r"C:\outside\ticket.md"),
            Ok(ScopeImpact::Ignore)
        );
        assert_eq!(windows.classify_str(r"C:outside\ticket.md"), Err(()));
        assert_eq!(
            windows.classify_str(r"\\server\share\ticket.md"),
            Ok(ScopeImpact::Ignore)
        );

        let mut unc = PathClassifier::new(r"\\Server\Share\Store", true);
        unc.rebuild(&catalogue());
        assert!(matches!(
            unc.classify_str(
                r"\\server\share\store\projects\alpha\investigations\deep\ticket.md"
            ),
            Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep"
        ));
        assert_eq!(
            unc.classify_str(r"\\server\other\store\ticket.md"),
            Ok(ScopeImpact::Ignore)
        );
    }

    #[test]
    fn classifier_preserves_posix_filesystem_root_namespace() {
        let mut classifier = PathClassifier::new("/", false);
        classifier.rebuild(&catalogue());
        assert_eq!(classifier.root_namespace, RootNamespace::FilesystemRoot);
        assert_eq!(classifier.classify_str("/"), Ok(ScopeImpact::Store));
        assert_eq!(
            classifier.classify_str("/casefile.toml"),
            Ok(ScopeImpact::Store)
        );
        assert_eq!(
            classifier.classify_str("/.git/config"),
            Ok(ScopeImpact::Ignore)
        );
        assert!(matches!(
            classifier.classify_str(
                "/projects/alpha/investigations/deep/.git/nested-control.md"
            ),
            Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep"
        ));
        assert!(matches!(
            classifier.classify_str(
                "/projects/alpha/investigations/deep/tickets/changed.md"
            ),
            Ok(ScopeImpact::Investigation { identity, .. }) if identity == "deep"
        ));
        assert_eq!(
            classifier.classify_str("/outside-the-catalogue/file.md"),
            Ok(ScopeImpact::Store),
            "every absolute POSIX path is contained when the watched root is /"
        );
        assert_eq!(
            classifier.classify_str("relative-callback.md"),
            Ok(ScopeImpact::Store),
            "a backend-relative callback is also inside the filesystem root"
        );
        assert_eq!(classifier.classify_str("../unsafe"), Err(()));
    }

    #[test]
    fn reducer_preserves_later_observation_and_failure_never_clears() {
        let mut reducer = FreshnessReducer::new(WatchHealth::Healthy);
        reducer.mark(
            ScopeImpact::Project {
                project: "alpha".into(),
            },
            false,
            "first".into(),
        );
        let classifier = PathClassifier::new("/store", false);
        reducer.report(
            RefreshReport::Started {
                generation: 1,
                target: PresentationTarget::Project {
                    project: "alpha".into(),
                },
                observation_generation: 1,
            },
            &classifier,
        );
        reducer.mark(
            ScopeImpact::Project {
                project: "alpha".into(),
            },
            false,
            "later".into(),
        );
        assert!(!reducer.report(
            RefreshReport::Succeeded {
                generation: 1,
                target: PresentationTarget::Project {
                    project: "alpha".into()
                },
                started_observation_generation: 1,
                completed_observation_generation: 2
            },
            &classifier
        ));
        assert_eq!(reducer.records.len(), 2);
        reducer.report(
            RefreshReport::Started {
                generation: 2,
                target: PresentationTarget::Store,
                observation_generation: 2,
            },
            &classifier,
        );
        reducer.report(
            RefreshReport::Failed {
                generation: 2,
                target: PresentationTarget::Store,
                started_observation_generation: 2,
                completed_observation_generation: 2,
                message: "no".into(),
            },
            &classifier,
        );
        assert_eq!(reducer.records.len(), 2);
    }

    #[test]
    fn project_change_is_direct_for_one_investigation_and_inherited_for_sibling() {
        let mut reducer = FreshnessReducer::new(WatchHealth::Healthy);
        reducer.mark(
            ScopeImpact::Investigation {
                project: "alpha".into(),
                identity: "one".into(),
            },
            false,
            "change".into(),
        );
        assert!(
            reducer
                .warning(&SelectedScope::Investigation {
                    project: "alpha".into(),
                    identity: "one".into()
                })
                .unwrap()
                .contains("DIRECT")
        );
        assert!(
            reducer
                .warning(&SelectedScope::Investigation {
                    project: "alpha".into(),
                    identity: "two".into()
                })
                .unwrap()
                .contains("INHERITED")
        );
        assert!(
            reducer
                .warning(&SelectedScope::Project {
                    project: "other".into()
                })
                .is_none()
        );
        assert!(reducer.warning(&SelectedScope::Store).is_some());
    }

    #[test]
    fn restored_health_does_not_claim_freshness() {
        let mut reducer = FreshnessReducer::new(WatchHealth::Degraded);
        assert!(reducer.health(WatchHealth::Healthy, "restored".into()));
        assert!(!reducer.records.is_empty());
        assert!(reducer.warning(&SelectedScope::Store).is_some());
    }

    #[test]
    fn investigation_refresh_clears_only_direct_warning_and_store_refresh_is_required_for_store_wide()
     {
        let mut classifier = PathClassifier::new("/store", false);
        classifier.rebuild(&catalogue());
        let mut reducer = FreshnessReducer::new(WatchHealth::Healthy);
        reducer.mark(
            ScopeImpact::Investigation {
                project: "alpha".into(),
                identity: "deep".into(),
            },
            false,
            "change".into(),
        );
        reducer.report(
            RefreshReport::Started {
                generation: 7,
                target: PresentationTarget::Investigation {
                    project: "alpha".into(),
                    path: "projects/alpha/investigations/deep".into(),
                },
                observation_generation: 1,
            },
            &classifier,
        );
        assert!(reducer.report(
            RefreshReport::Succeeded {
                generation: 7,
                target: PresentationTarget::Investigation {
                    project: "alpha".into(),
                    path: "projects/alpha/investigations/deep".into(),
                },
                started_observation_generation: 1,
                completed_observation_generation: 1,
            },
            &classifier,
        ));
        assert!(
            reducer
                .warning(&SelectedScope::Investigation {
                    project: "alpha".into(),
                    identity: "deep".into(),
                })
                .unwrap()
                .contains("INHERITED")
        );

        reducer.mark(ScopeImpact::Store, false, "configuration".into());
        reducer.report(
            RefreshReport::Started {
                generation: 8,
                target: PresentationTarget::Project {
                    project: "alpha".into(),
                },
                observation_generation: 2,
            },
            &classifier,
        );
        reducer.report(
            RefreshReport::Succeeded {
                generation: 8,
                target: PresentationTarget::Project {
                    project: "alpha".into(),
                },
                started_observation_generation: 2,
                completed_observation_generation: 2,
            },
            &classifier,
        );
        assert!(matches!(
            reducer.observation().minimum_scope,
            RefreshMinimumScope::Store { .. }
        ));
    }

    #[test]
    fn fake_watcher_covers_rename_overflow_coalescing_and_teardown() {
        let root = Path::new("/store");
        let (mut watcher, events, observations, _reports, stopped) = WatchCoordinator::fake(root);
        watcher.rebuild(&catalogue());
        let rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(PathBuf::from(
                "/store/projects/alpha/investigations/deep/old.md",
            ))
            .add_path(PathBuf::from(
                "/store/projects/alpha/investigations/deep/new.md",
            ));
        events
            .send(NativeObservation::Event(rename))
            .expect("fake rename");
        assert!(watcher.drain(), "first stale redraw must be immediate");
        assert_eq!(
            observations.recv().expect("rename observation").generation,
            2,
            "both rename endpoints are classified"
        );
        assert!(
            watcher
                .warning(&SelectedScope::Store)
                .unwrap()
                .contains("project alpha")
        );

        let overflow = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        events
            .send(NativeObservation::Event(overflow))
            .expect("fake overflow");
        assert!(!watcher.drain(), "burst redraw is coalesced");
        assert!(matches!(
            observations
                .recv()
                .expect("overflow observation")
                .minimum_scope,
            RefreshMinimumScope::Store { .. }
        ));
        thread::sleep(REDRAW_COALESCE + Duration::from_millis(20));
        assert!(watcher.drain(), "coalesced redraw deadline fires");
        drop(watcher);
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn native_recursive_subscription_reports_a_disk_change_and_tears_down() {
        let directory = tempfile::tempdir().expect("temporary Store");
        let nested = directory.path().join("projects/alpha/investigations/one");
        std::fs::create_dir_all(&nested).expect("nested Store path");
        let (subscription, health) = NativeSubscription::start(directory.path().to_owned());
        assert_eq!(health, WatchHealth::Healthy);
        let changed = nested.join("ticket.md");
        std::fs::write(&changed, "changed").expect("write observed file");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = false;
        while Instant::now() < deadline {
            if let Ok(NativeObservation::Event(event)) = subscription
                .observations
                .recv_timeout(Duration::from_millis(100))
            {
                observed |= event.paths.iter().any(|path| path == &changed);
                if observed {
                    break;
                }
            }
        }
        assert!(observed, "native backend did not report the changed path");
        drop(subscription);
    }
}
