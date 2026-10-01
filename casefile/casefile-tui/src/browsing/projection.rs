type VisibleKey = (u64, View, Option<String>, Option<String>, String);
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Projection {
    pub(super) indices: BTreeMap<String, usize>,
    facts: BTreeMap<String, Fact>,
    roots: BTreeMap<String, Vec<String>>,
    scopes: BTreeMap<(String, Option<String>), BTreeSet<String>>,
    filter: std::cell::RefCell<Option<FilterFacts>>,
    counts: BTreeMap<(String, Option<String>), [usize; 3]>,
    project_tickets: BTreeMap<String, usize>,
    generation: u64,
}

#[derive(Eq, PartialEq)]
struct Fact {
    scope: Option<(String, Option<String>)>,
    work: bool,
    strategy: bool,
    search: Vec<String>,
    classification: Classification,
    kind: Option<Kind>,
    identity: Option<String>,
    summary: Option<RecordSummary>,
}

impl Projection {
    pub(super) fn rebuild(&mut self, scan: &ScanResult) {
        let generation = self.generation;
        *self = Self::default();
        self.generation = generation;
        self.roots = scan.investigation_roots.clone();
        for (index, entry) in scan.snapshot.entries.iter().enumerate() {
            self.insert(scan, entry, index);
        }
        self.generation += 1;
    }

    pub(super) fn update(
        &mut self,
        scan: &ScanResult,
        indices: &BTreeMap<String, usize>,
        changed: &BTreeSet<String>,
    ) {
        let mut changed_facts = false;
        for path in changed {
            if let Some(index) = indices.get(path) {
                let entry = &scan.snapshot.entries[*index];
                if self.facts.get(path).is_some_and(|fact| {
                    fact.classification == entry.classification
                        && fact.kind == entry.kind
                        && fact.identity == entry.identity
                        && fact.summary == entry.summary
                }) {
                    self.indices.insert(path.clone(), *index);
                    continue;
                }
            }
            if let Some(old) = self.facts.remove(path) {
                self.count(&old, false);
                if let Some(scope) = &old.scope
                    && let Some(paths) = self.scopes.get_mut(scope)
                {
                    paths.remove(path);
                }
                if let Some(filter) = self.filter.get_mut() {
                    filter.remove(path, &old);
                }
            }
            changed_facts = true;
            self.indices.remove(path);
            if let Some(index) = indices.get(path) {
                self.insert(scan, &scan.snapshot.entries[*index], *index);
            }
        }
        if changed_facts {
            self.generation += 1;
        }
    }

    fn insert(&mut self, scan: &ScanResult, entry: &EntrySnapshot, index: usize) {
        let fact = Self::fact(scan, entry);
        self.count(&fact, true);
        if let Some(scope) = &fact.scope {
            self.scopes
                .entry(scope.clone())
                .or_default()
                .insert(entry.path.clone());
        }
        if let Some(filter) = self.filter.get_mut() {
            filter.insert(&entry.path, &fact);
        }
        self.indices.insert(entry.path.clone(), index);
        self.facts.insert(entry.path.clone(), fact);
    }

    fn fact(scan: &ScanResult, entry: &EntrySnapshot) -> Fact {
        Fact {
            scope: entry_scope(scan, entry).map(|(p, i)| (p.into(), i.map(Into::into))),
            classification: entry.classification,
            kind: entry.kind,
            identity: entry.identity.clone(),
            summary: entry.summary.clone(),
            work: is_work(entry),
            strategy: is_strategy(entry),
            search: [
                entry.path.as_str(),
                classification_name(entry.classification),
                entry.kind.map(kind_name).unwrap_or_default(),
                entry.identity.as_deref().unwrap_or_default(),
                summary_title(entry.summary.as_ref()),
                work_status(entry.summary.as_ref()),
                strategy_phase(entry.summary.as_ref()),
                strategy_role(entry.summary.as_ref()),
                strategy_model(entry.summary.as_ref()),
                strategy_reasoning(entry.summary.as_ref()),
            ]
            .into_iter()
            .map(str::to_lowercase)
            .collect(),
        }
    }

    fn count(&mut self, fact: &Fact, add: bool) {
        if let Some(scope) = &fact.scope {
            let counts = self.counts.entry(scope.clone()).or_default();
            for (index, included) in [fact.work, !fact.work, fact.strategy]
                .into_iter()
                .enumerate()
            {
                if included {
                    if add {
                        counts[index] += 1;
                    } else {
                        counts[index] -= 1;
                    }
                }
            }
            if fact.work {
                let count = self.project_tickets.entry(scope.0.clone()).or_default();
                if add {
                    *count += 1;
                } else {
                    *count -= 1;
                }
            }
        }
    }

    pub(super) fn visible(&self, browser: &Browser) -> Visible {
        let filter = browser.filter.to_lowercase();
        let ignore_filter = browser.view == View::Boards || filter.is_empty();
        if !ignore_filter
            && self
                .filter
                .borrow()
                .as_ref()
                .is_none_or(|cached| cached.needle != filter)
        {
            let mut cached = FilterFacts {
                needle: filter.clone(),
                ..FilterFacts::default()
            };
            for (path, fact) in &self.facts {
                cached.insert(path, fact);
            }
            *self.filter.borrow_mut() = Some(cached);
        }
        let cached = self.filter.borrow();
        let empty = BTreeSet::new();
        let scoped = browser
            .selected_project
            .as_ref()
            .filter(|_| matches!(browser.view, View::Tickets | View::Files | View::Strategies));
        let exact = scoped
            .and_then(|project| {
                self.scopes
                    .get(&(project.clone(), browser.selected_investigation.clone()))
            })
            .unwrap_or(&empty);
        let project = scoped
            .filter(|_| browser.view == View::Files)
            .and_then(|project| self.scopes.get(&(project.clone(), None)))
            .unwrap_or(&empty);
        let paths = exact
            .union(project)
            .filter(|path| {
                (ignore_filter
                    || cached
                        .as_ref()
                        .is_some_and(|filter| filter.paths.contains(*path)))
                    && match browser.view {
                        View::Tickets => self.facts[*path].work,
                        View::Files => !self.facts[*path].work,
                        View::Strategies => {
                            self.facts[*path].strategy && browser.selected_investigation.is_some()
                        }
                        View::Projects | View::Investigations | View::Boards => false,
                    }
            })
            .cloned()
            .collect::<Vec<_>>();
        let projects = self
            .roots
            .keys()
            .filter(|project| {
                ignore_filter
                    || project.to_lowercase().contains(&filter)
                    || cached
                        .as_ref()
                        .is_some_and(|cached| cached.projects.contains_key(*project))
            })
            .cloned()
            .collect();
        let investigations = browser
            .selected_project
            .as_ref()
            .and_then(|project| self.roots.get(project).map(|roots| (project, roots)))
            .map(|(project, roots)| {
                roots
                    .iter()
                    .filter(|investigation| {
                        ignore_filter
                            || investigation.to_lowercase().contains(&filter)
                            || cached.as_ref().is_some_and(|cached| {
                                cached.scopes.contains_key(&(
                                    project.clone(),
                                    Some((*investigation).clone()),
                                ))
                            })
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let positions = paths
            .iter()
            .enumerate()
            .map(|(index, path)| (path.clone(), index))
            .collect();
        let mut previous = String::new();
        let mut ends = Vec::with_capacity(paths.len());
        let mut end = 0;
        let directories = paths
            .iter()
            .map(|path| {
                let prefix = self.facts[path]
                    .scope
                    .as_ref()
                    .map(|(project, investigation)| match investigation {
                        Some(investigation) => {
                            format!("projects/{project}/investigations/{investigation}/")
                        }
                        None => format!("projects/{project}/"),
                    })
                    .unwrap_or_default();
                let directory = parent_directory(path.strip_prefix(&prefix).unwrap_or(path));
                let show = browser.view == View::Files && directory != previous;
                previous.clone_from(&directory);
                end += if show { 2 } else { 1 };
                ends.push(end);
                show.then_some(directory)
            })
            .collect();
        Visible {
            key: browser.projection_key(self.generation),
            projects,
            investigations,
            paths,
            positions,
            directories,
            ends,
            counts: self.scope_counts(browser),
        }
    }

    pub(super) fn scope_counts(&self, browser: &Browser) -> [usize; 3] {
        let Some(project) = &browser.selected_project else {
            return [0; 3];
        };
        let exact = self
            .counts
            .get(&(project.clone(), browser.selected_investigation.clone()))
            .copied()
            .unwrap_or_default();
        let project_files = if browser.selected_investigation.is_some() {
            self.counts
                .get(&(project.clone(), None))
                .map_or(0, |c| c[1])
        } else {
            0
        };
        [
            exact[0],
            exact[1] + project_files,
            if browser.selected_investigation.is_some() {
                exact[2]
            } else {
                0
            },
        ]
    }

    pub(super) fn project_counts(&self, project: &str) -> (usize, usize) {
        (
            self.roots.get(project).map_or(0, Vec::len),
            self.project_tickets.get(project).copied().unwrap_or(0),
        )
    }

    pub(super) fn tickets(&self, project: &str, investigation: &str) -> usize {
        self.counts
            .get(&(project.into(), Some(investigation.into())))
            .map_or(0, |c| c[0])
    }
}

#[derive(Default)]
pub(super) struct Visible {
    key: Option<VisibleKey>,
    pub(super) projects: Vec<String>,
    pub(super) investigations: Vec<String>,
    pub(super) paths: Vec<String>,
    pub(super) positions: BTreeMap<String, usize>,
    pub(super) directories: Vec<Option<String>>,
    ends: Vec<usize>,
    pub(super) counts: [usize; 3],
}

impl Visible {
    pub(super) fn row_height(&self, index: usize) -> usize {
        if self.directories[index].is_some() {
            2
        } else {
            1
        }
    }
    pub(super) fn row_end(&self, view: View, index: usize) -> usize {
        if matches!(view, View::Projects | View::Investigations) {
            index + 1
        } else {
            self.ends[index]
        }
    }
    pub(super) fn start_row(&self, view: View, row: usize) -> usize {
        if matches!(view, View::Projects | View::Investigations) {
            row
        } else {
            self.ends.partition_point(|end| *end <= row)
        }
    }
}

impl Browser {
    fn projection_key(&self, generation: u64) -> Option<VisibleKey> {
        Some((
            generation,
            self.view,
            self.selected_project.clone(),
            self.selected_investigation.clone(),
            self.filter.clone(),
        ))
    }

    pub(super) fn visible(&self) -> std::cell::Ref<'_, Visible> {
        let current = self.visible.borrow().key.as_ref().is_some_and(
            |(generation, view, project, investigation, filter)| {
                *generation == self.projection.generation
                    && *view == self.view
                    && project == &self.selected_project
                    && investigation == &self.selected_investigation
                    && filter == &self.filter
            },
        );
        if !current {
            *self.visible.borrow_mut() = self.projection.visible(self);
        }
        self.visible.borrow()
    }
}

#[derive(Default)]
struct FilterFacts {
    needle: String,
    paths: BTreeSet<String>,
    projects: BTreeMap<String, usize>,
    scopes: BTreeMap<(String, Option<String>), usize>,
}

impl FilterFacts {
    fn insert(&mut self, path: &str, fact: &Fact) {
        if fact.search.iter().any(|field| field.contains(&self.needle)) {
            self.paths.insert(path.into());
            if let Some(scope) = &fact.scope {
                *self.projects.entry(scope.0.clone()).or_default() += 1;
                *self.scopes.entry(scope.clone()).or_default() += 1;
            }
        }
    }
    fn remove(&mut self, path: &str, fact: &Fact) {
        if self.paths.remove(path)
            && let Some(scope) = &fact.scope
        {
            decrement(&mut self.projects, &scope.0);
            decrement(&mut self.scopes, scope);
        }
    }
}
fn decrement<K: Ord>(counts: &mut BTreeMap<K, usize>, key: &K) {
    if let Some(count) = counts.get_mut(key) {
        *count -= 1;
        if *count == 0 {
            counts.remove(key);
        }
    }
}
