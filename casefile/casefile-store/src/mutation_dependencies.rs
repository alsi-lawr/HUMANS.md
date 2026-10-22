use super::mutation_metadata::{Header, from_bytes, header, list, stem};
use crate::{
    activation::{activation, scope_for},
    layout::kind_for_path,
    mutation::Overlay,
    store::StoreError,
};
use casefile_core::Kind;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::Arc,
};

pub(super) struct Dependencies {
    pub(super) paths: BTreeSet<String>,
    pub(super) existence: BTreeSet<String>,
    pub(super) validation_paths: BTreeSet<String>,
    pub(super) progress_inputs: BTreeMap<String, ProgressInput>,
    identities: BTreeSet<String>,
    written_identities: BTreeSet<String>,
}
pub(super) enum ProgressFacts {
    Full(Arc<casefile_core::ProgressLog>),
    Operations(Vec<(String, String)>),
}
pub(super) struct ProgressInput {
    pub(super) entry: Option<casefile_core::EntrySnapshot>,
    pub(super) log: Result<Option<ProgressFacts>, Vec<casefile_core::Diagnostic>>,
    references: BTreeSet<String>,
}
impl ProgressInput {
    pub(super) fn new(
        entry: Option<casefile_core::EntrySnapshot>,
        log: Result<Option<ProgressFacts>, Vec<casefile_core::Diagnostic>>,
    ) -> Self {
        let references = match log.as_ref().ok().and_then(Option::as_ref) {
            Some(ProgressFacts::Full(log)) => log
                .entries
                .iter()
                .map(|entry| entry.ticket_id())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            Some(ProgressFacts::Operations(operations)) => operations
                .iter()
                .map(|(_, ticket)| ticket.as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            None => BTreeSet::new(),
        };
        Self {
            entry,
            log,
            references,
        }
    }
}

impl Dependencies {
    pub(super) fn locks(&self, changes: &Overlay, applying: bool) -> BTreeMap<String, bool> {
        self.paths
            .iter()
            .map(|path| {
                (
                    format!("path:{path}"),
                    applying && changes.contains_key(path),
                )
            })
            .chain(self.identities.iter().map(|id| {
                (
                    format!("identity:{id}"),
                    applying && self.written_identities.contains(id),
                )
            }))
            .collect()
    }
}

pub(super) fn discover(
    root: &Path,
    changes: &Overlay,
    extra: &[String],
    initial_progress: Option<(String, ProgressInput)>,
    proposed_progress: Option<(&str, &casefile_core::ProgressLog)>,
) -> Result<Dependencies, StoreError> {
    let (_, active, _) = activation(root)?;
    let mut paths = changes
        .keys()
        .chain(extra)
        .cloned()
        .collect::<BTreeSet<_>>();
    paths.insert("casefile.toml".into());
    let scopes = paths
        .iter()
        .filter_map(|path| scope_for(path, &active))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut candidates = BTreeMap::<String, Header>::new();
    let index = crate::activation::ScopeIndex::new(&active);
    for base in &scopes {
        let mut directories = vec![
            format!("{base}/boards"),
            format!("{base}/decision-log"),
            format!("{base}/evidence"),
            format!("{base}/review"),
        ];
        for kind in ["tickets", "epics"] {
            for status in ["accepted", "provisional", "rejected"] {
                directories.push(format!("{base}/{kind}/{status}"));
            }
        }
        for directory in directories {
            for path in list(
                root,
                &directory,
                directory.ends_with("/review"),
                &index,
                Some(base),
            )? {
                let Some(kind) = kind_for_path(&path, &active) else {
                    continue;
                };
                let mut found = header(root, &path, kind)?;
                if matches!(kind, Kind::Ticket | Kind::Epic) {
                    found.id = Some(stem(&path).into());
                }
                candidates.insert(path, found);
            }
        }
    }
    let mut written_identities = BTreeSet::new();
    let mut written_targets = BTreeSet::new();
    for (path, bytes) in changes {
        written_targets.insert((
            path.clone(),
            candidates.get(path).and_then(|header| header.id.clone()),
        ));
        if let Some(old) = candidates.get(path).and_then(|h| h.id.as_ref()) {
            written_identities.insert(old.clone());
        }
        if let Some(bytes) = bytes {
            let parsed = from_bytes(bytes, kind_for_path(path, &active));
            if let Some(id) = &parsed.id {
                written_identities.insert(id.clone());
                written_targets.insert((path.clone(), Some(id.clone())));
            }
            // Preserve both old and proposed edges for introduced-diagnostic comparisons.
            let old = candidates.entry(path.clone()).or_default();
            old.decision_refs.extend(parsed.decision_refs);
            old.related_tickets.extend(parsed.related_tickets);
            old.supersedes.extend(parsed.supersedes);
            old.superseded_by.extend(parsed.superseded_by);
            old.refs.extend(parsed.refs);
            old.attachments.extend(parsed.attachments);
            if parsed.id.is_some() {
                old.id = parsed.id;
            }
        }
    }
    // Progress and binding validation needs the log and its accepted-ticket membership, not all
    // investigation bodies. A ticket change also checks the reverse progress reference.
    let full_progress = paths.iter().any(|path| {
        matches!(
            kind_for_path(path, &active),
            Some(Kind::Progress | Kind::StrategyBinding)
        )
    });
    let mut progress_inputs = initial_progress.into_iter().collect::<BTreeMap<_, _>>();
    let initial = paths.clone();
    for path in initial {
        let Some(base) = scope_for(&path, &active) else {
            continue;
        };
        match kind_for_path(&path, &active) {
            Some(Kind::Ticket | Kind::Epic) => {
                let log_path = format!("{base}/progress/log.toml");
                if !progress_inputs.contains_key(&log_path) {
                    progress_inputs.insert(
                        log_path.clone(),
                        progress_input(root, &log_path, full_progress)?,
                    );
                }
                let references = &progress_inputs.get(&log_path).unwrap().references;
                if references.iter().any(|id| written_identities.contains(id)) {
                    paths.insert(log_path.clone());
                }
                candidates
                    .entry(log_path.clone())
                    .or_default()
                    .refs
                    .extend(references.iter().cloned());
            }
            Some(Kind::StrategyBinding) => {
                paths.insert(format!("{base}/strategy/implementation.toml"));
                paths.insert(format!("{base}/progress/log.toml"));
            }
            Some(Kind::Strategy) => {
                if path.ends_with("/implementation.toml") {
                    paths.insert(format!("{base}/strategy/bindings.toml"));
                }
                if !root.join(&path).exists() {
                    let phase = path
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .trim_end_matches(".toml");
                    for history in list(
                        root,
                        &format!("{base}/strategy/transitions"),
                        false,
                        &index,
                        Some(base),
                    )? {
                        if header(root, &history, Kind::StrategyTransition)?
                            .phase
                            .as_deref()
                            == Some(phase)
                        {
                            paths.insert(history);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut accepted = BTreeMap::<(String, String), Vec<String>>::new();
    for (path, header) in &candidates {
        if let Some((base, filename)) = path.split_once("/tickets/accepted/") {
            if !filename.contains('/') {
                if let Some(id) = &header.id {
                    accepted
                        .entry((base.into(), id.clone()))
                        .or_default()
                        .push(path.clone());
                }
            }
        }
    }
    for path in paths.clone() {
        if kind_for_path(&path, &active) != Some(Kind::Progress) {
            continue;
        }
        if !progress_inputs.contains_key(&path) {
            progress_inputs.insert(path.clone(), progress_input(root, &path, full_progress)?);
        }
        let mut refs = progress_inputs.get(&path).unwrap().references.clone();
        if let Some((_, log)) =
            proposed_progress.filter(|(proposed_path, _)| *proposed_path == path)
        {
            refs.extend(log.entries.iter().map(|entry| entry.ticket_id().into()));
        } else if let Some(bytes) = changes.get(&path).and_then(Option::as_deref) {
            if let Ok(text) = std::str::from_utf8(bytes) {
                if let Ok(log) = casefile_core::parse_progress_log(&path, text) {
                    refs.extend(log.entries.iter().map(|entry| entry.ticket_id().into()));
                }
            }
        }
        candidates
            .entry(path.clone())
            .or_default()
            .refs
            .extend(refs.iter().cloned());
        let base = path
            .strip_suffix("/progress/log.toml")
            .expect("progress path");
        for id in &refs {
            if let Some(matching) = accepted.get(&(base.into(), id.clone())) {
                paths.extend(matching.iter().cloned());
            }
        }
    }
    let mut validation_paths = changes
        .keys()
        .chain(extra)
        .cloned()
        .collect::<BTreeSet<_>>();
    for (path, candidate) in &candidates {
        let source = index.resolve(path);
        let incoming = candidate.references().any(|reference| {
            written_targets.iter().any(|(target, id)| {
                crate::references::matches(
                    source,
                    reference,
                    target,
                    id.as_deref(),
                    index.resolve(target),
                )
            })
        });
        let duplicate = candidate.id.as_ref().is_some_and(|id| {
            written_targets.iter().any(|(target, written_id)| {
                written_id.as_deref() == Some(id.as_str())
                    && source.scope == index.resolve(target).scope
            })
        });
        if incoming || duplicate {
            paths.insert(path.clone());
            validation_paths.insert(path.clone());
        }
    }
    validation_paths.extend(
        paths
            .iter()
            .filter(|path| kind_for_path(path, &active) == Some(Kind::Progress))
            .cloned(),
    );
    let mut frontier = paths
        .iter()
        .map(|path| (path.clone(), true))
        .collect::<VecDeque<_>>();
    let mut visited = BTreeSet::new();
    while let Some((source, full)) = frontier.pop_front() {
        if !visited.insert((source.clone(), full)) {
            continue;
        }
        let Some(candidate) = candidates.get(&source) else {
            continue;
        };
        let references = if full {
            candidate.references().cloned().collect::<BTreeSet<_>>()
        } else {
            candidate.supersedes.iter().cloned().collect()
        };
        let mut targets = BTreeMap::new();
        for reference in references {
            let cycle = candidate.supersedes.contains(&reference);
            for (path, target) in &candidates {
                if crate::references::matches(
                    index.resolve(&source),
                    &reference,
                    path,
                    target.id.as_deref(),
                    index.resolve(path),
                ) {
                    targets.insert(path.clone(), cycle);
                }
            }
            if candidate.decision_refs.contains(&reference)
                || candidate.related_tickets.contains(&reference)
                || candidate.supersedes.contains(&reference)
                || candidate.superseded_by.contains(&reference)
            {
                for path in crate::references::candidates(
                    root,
                    &active,
                    &source,
                    &reference,
                    candidate.decision_refs.contains(&reference),
                )? {
                    targets.insert(path, cycle);
                }
            }
        }
        for (target, cycle) in targets {
            if !candidates.contains_key(&target) {
                if let Some(kind) = kind_for_path(&target, &active) {
                    candidates.insert(target.clone(), header(root, &target, kind)?);
                }
            }
            paths.insert(target.clone());
            if cycle {
                frontier.push_back((target, false));
            }
        }
    }
    let identity_key =
        |path: &str, id: &str| format!("{}:{id}", index.resolve(path).scope.unwrap_or("project"));
    let mut identities = paths
        .iter()
        .filter_map(|path| {
            candidates
                .get(path)
                .and_then(|header| header.id.as_deref())
                .map(|id| identity_key(path, id))
        })
        .collect::<BTreeSet<_>>();
    let written_identities = written_targets
        .iter()
        .filter_map(|(path, id)| id.as_deref().map(|id| identity_key(path, id)))
        .collect::<BTreeSet<_>>();
    identities.extend(written_identities.iter().cloned());
    let mut existence = BTreeSet::new();
    for path in paths.clone() {
        if let Some(header) = candidates.get(&path) {
            for attachment in &header.attachments {
                if crate::layout::safe_relative(attachment) {
                    if let Some((parent, _)) = path.rsplit_once('/') {
                        let target = format!("{parent}/{attachment}");
                        if !paths.contains(&target) {
                            existence.insert(target);
                        }
                    }
                }
            }
        }
    }
    paths.extend(existence.iter().cloned());
    Ok(Dependencies {
        paths,
        existence,
        validation_paths,
        progress_inputs,
        identities,
        written_identities,
    })
}

pub(super) fn accepted_paths(root: &Path, investigation: &str) -> Result<Vec<String>, StoreError> {
    let (_, active, _) = activation(root)?;
    list(
        root,
        &format!("{investigation}/tickets/accepted"),
        false,
        &crate::activation::ScopeIndex::new(&active),
        Some(investigation),
    )
}

fn progress_input(root: &Path, path: &str, full: bool) -> Result<ProgressInput, StoreError> {
    let entry = super::mutation::read_entry(root, path)?;
    let log = match &entry {
        None => Ok(None),
        Some(entry) => std::str::from_utf8(&entry.original_bytes)
            .map_err(|_| {
                vec![casefile_core::Diagnostic::new(
                    path,
                    "invalid_utf8",
                    "governed text must be UTF-8",
                )]
            })
            .and_then(|text| {
                if full {
                    casefile_core::parse_progress_log(path, text)
                        .map(|log| Some(ProgressFacts::Full(Arc::new(log))))
                } else {
                    casefile_core::parse_progress_operations(path, text)
                        .map(|operations| Some(ProgressFacts::Operations(operations)))
                }
            }),
    };
    Ok(ProgressInput::new(entry, log))
}
