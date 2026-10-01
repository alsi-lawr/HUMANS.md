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
    let projects = paths
        .iter()
        .filter_map(|p| {
            p.strip_prefix("projects/")?
                .split_once('/')
                .map(|(p, _)| p.to_owned())
        })
        .collect::<BTreeSet<_>>();
    let scopes = paths
        .iter()
        .filter_map(|path| scope_for(path, &active))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut candidates = BTreeMap::<String, Header>::new();
    for (project, config) in &active.projects {
        let mut directories = vec![format!("projects/{project}/decision-log")];
        for base in &config.investigations {
            directories.push(format!("{base}/boards"));
            directories.push(format!("{base}/decision-log"));
            for kind in ["tickets", "epics"] {
                for status in ["accepted", "provisional", "rejected"] {
                    directories.push(format!("{base}/{kind}/{status}"));
                }
            }
            if scopes.contains(base) {
                directories.extend([format!("{base}/evidence"), format!("{base}/review")]);
            }
        }
        for directory in directories {
            for path in list(root, &directory, directory.ends_with("/review"))? {
                let Some(kind) = kind_for_path(&path, &active) else {
                    continue;
                };
                let mut header = match kind {
                    Kind::Board => header(root, &path, kind)?,
                    Kind::Ticket | Kind::Epic | Kind::Evidence | Kind::Review
                        if projects.contains(project) =>
                    {
                        header(root, &path, kind)?
                    }
                    _ => Header::default(),
                };
                if matches!(kind, Kind::Ticket | Kind::Epic) {
                    header.id = Some(stem(&path).into());
                }
                if kind == Kind::Decision {
                    // The canonical parser chooses a filename prefix using the H1; all prefixes
                    // are candidates, and only selected candidates are canonically parsed.
                    header.id = Some(stem(&path).into());
                }
                candidates.insert(path, header);
            }
        }
    }
    let mut written_identities = BTreeSet::new();
    for (path, bytes) in changes {
        if let Some(old) = candidates.get(path).and_then(|h| h.id.as_ref()) {
            written_identities.insert(old.clone());
        }
        if let Some(bytes) = bytes {
            let parsed = from_bytes(bytes, kind_for_path(path, &active));
            if let Some(id) = &parsed.id {
                written_identities.insert(id.clone());
            }
            // Preserve both old and proposed edges for introduced-diagnostic comparisons.
            let old = candidates.entry(path.clone()).or_default();
            old.refs.extend(parsed.references().cloned());
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
                    for history in list(root, &format!("{base}/strategy/transitions"), false)? {
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
    let mut identities = written_identities.clone();
    let mut validation_paths = changes
        .keys()
        .chain(extra)
        .cloned()
        .collect::<BTreeSet<_>>();
    for (path, header) in &candidates {
        if header
            .references()
            .any(|id| written_identities.contains(id))
        {
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
    let mut cycle_ids = BTreeSet::new();
    for path in &paths {
        if let Some(header) = candidates.get(path) {
            identities.extend(header.id.iter().cloned());
            if validation_paths.contains(path)
                || kind_for_path(path, &active) == Some(Kind::Progress)
            {
                identities.extend(header.references().cloned());
                cycle_ids.extend(header.id.iter().cloned());
                cycle_ids.extend(header.supersedes.iter().cloned());
            }
        }
    }
    let mut by_identity = BTreeMap::<&str, Vec<&str>>::new();
    for (path, header) in &candidates {
        if let Some(id) = &header.id {
            by_identity.entry(id).or_default().push(path);
        }
        if kind_for_path(path, &active) == Some(Kind::Decision) {
            let stem = stem(path);
            for (index, _) in stem.match_indices('-') {
                by_identity.entry(&stem[..index]).or_default().push(path);
            }
        }
    }
    let mut frontier = identities
        .iter()
        .map(|id| (id.clone(), cycle_ids.contains(id)))
        .collect::<VecDeque<_>>();
    let mut visited = BTreeSet::new();
    while let Some((id, cycle)) = frontier.pop_front() {
        if !visited.insert((id.clone(), cycle)) {
            continue;
        }
        if let Some(matching) = by_identity.get(id.as_str()) {
            for path in matching {
                paths.insert((*path).into());
                let header = &candidates[*path];
                if cycle && header.id.as_ref().is_some_and(|id| cycle_ids.contains(id)) {
                    for next in &header.supersedes {
                        identities.insert(next.clone());
                        cycle_ids.insert(next.clone());
                        frontier.push_back((next.clone(), true));
                    }
                }
            }
        }
    }
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
    list(root, &format!("{investigation}/tickets/accepted"), false)
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
