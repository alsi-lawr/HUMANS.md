use super::mutation_metadata::{Header, from_bytes, header, list, stem};
use crate::{
    activation::{activation, scope_for},
    layout::kind_for_path,
    mutation::Overlay,
    store::StoreError,
};
use casefile_core::Kind;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub(super) struct Dependencies {
    pub(super) paths: BTreeSet<String>,
    pub(super) existence: BTreeSet<String>,
    pub(super) validation_paths: BTreeSet<String>,
    identities: BTreeSet<String>,
    written_identities: BTreeSet<String>,
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
            if projects.contains(project) {
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
    let initial = paths.clone();
    for path in initial {
        let Some(base) = scope_for(&path, &active) else {
            continue;
        };
        match kind_for_path(&path, &active) {
            Some(Kind::Ticket | Kind::Epic) => {
                let log_path = format!("{base}/progress/log.toml");
                let mut log_header = Header::default();
                if let Some(entry) = super::mutation::read_entry(root, &log_path)? {
                    if let Ok(text) = std::str::from_utf8(&entry.original_bytes) {
                        if let Ok(log) = casefile_core::parse_progress_log(&log_path, text) {
                            log_header
                                .refs
                                .extend(log.entries.iter().map(|e| e.ticket_id().to_owned()));
                        }
                    }
                }
                if log_header
                    .refs
                    .iter()
                    .any(|id| written_identities.contains(id))
                {
                    paths.insert(log_path.clone());
                }
                candidates.insert(log_path, log_header);
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
    for path in paths.clone() {
        if kind_for_path(&path, &active) != Some(Kind::Progress) {
            continue;
        }
        let existing = fs::read(root.join(&path)).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(Vec::new())
            } else {
                Err(e)
            }
        })?;
        let mut refs = BTreeSet::new();
        for bytes in [
            Some(existing.as_slice()),
            changes.get(&path).and_then(Option::as_deref),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(text) = std::str::from_utf8(bytes) {
                if let Ok(log) = casefile_core::parse_progress_log(&path, text) {
                    refs.extend(log.entries.iter().map(|e| e.ticket_id().to_owned()));
                }
            }
        }
        candidates
            .entry(path.clone())
            .or_default()
            .refs
            .extend(refs.iter().cloned());
        let base = path
            .strip_suffix("progress/log.toml")
            .expect("progress path");
        for (candidate, header) in &candidates {
            if candidate.starts_with(&format!("{base}tickets/accepted/"))
                && header.id.as_ref().is_some_and(|id| refs.contains(id))
            {
                paths.insert(candidate.clone());
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
    loop {
        let count = (paths.len(), identities.len());
        for (path, header) in &candidates {
            let selected = header.id.as_ref().is_some_and(|id| identities.contains(id))
                || (kind_for_path(path, &active) == Some(Kind::Decision)
                    && identities
                        .iter()
                        .any(|id| stem(path).starts_with(&format!("{id}-"))));
            if selected {
                paths.insert(path.clone());
                // Only supersession reachability is transitive. Related and decision references
                // on an unchanged supporting record do not expand another record's read set.
                if header.id.as_ref().is_some_and(|id| cycle_ids.contains(id)) {
                    cycle_ids.extend(header.supersedes.iter().cloned());
                    identities.extend(header.supersedes.iter().cloned());
                }
            }
        }
        if count == (paths.len(), identities.len()) {
            break;
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
        identities,
        written_identities,
    })
}

pub(super) fn accepted_paths(root: &Path, investigation: &str) -> Result<Vec<String>, StoreError> {
    list(root, &format!("{investigation}/tickets/accepted"), false)
}
