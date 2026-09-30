use super::*;
use rayon::prelude::*;

static READ_POOL: std::sync::LazyLock<Result<rayon::ThreadPool, rayon::ThreadPoolBuildError>> =
    std::sync::LazyLock::new(|| rayon::ThreadPoolBuilder::new().num_threads(2).build());

#[derive(Clone, Copy)]
pub(crate) enum ScopedRead {
    RecordIndex,
    Boards,
    StrategyTransitions,
}

pub(crate) struct ScopedReadResult {
    pub(crate) freshness: crate::ScopeReadToken,
    pub(crate) drafts: BTreeMap<String, RecordDraft>,
    pub(crate) project: String,
    pub(crate) investigation: String,
    pub(crate) path: String,
    pub(crate) entries: Vec<EntrySnapshot>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(crate) fn scoped_scan(
    root: &Path,
    project: &str,
    investigation: &str,
    read: ScopedRead,
) -> Result<ScopedReadResult, StoreError> {
    let scope = crate::InvestigationScope {
        project: project.into(),
        investigation: investigation.into(),
    };
    let target = match read {
        ScopedRead::RecordIndex => crate::ScopeReadTarget::RecordIndex { scope },
        ScopedRead::Boards => crate::ScopeReadTarget::Boards { scope },
        ScopedRead::StrategyTransitions => crate::ScopeReadTarget::StrategyTransitions { scope },
    };
    let mut observation = crate::read_context::ScopeObservation::begin(root, target)?;
    if matches!(read, ScopedRead::RecordIndex | ScopedRead::Boards) {
        observation.observe_progress(root)?;
    }
    scoped_entries(root, observation)
}

pub(crate) fn scoped_detail_scan(
    root: &Path,
    project: &str,
    investigation: &str,
    identity: &str,
) -> Result<ScopedReadResult, StoreError> {
    let target = crate::ScopeReadTarget::RecordDetail {
        identity: crate::InvestigationScopedIdentity {
            scope: crate::InvestigationScope {
                project: project.into(),
                investigation: investigation.into(),
            },
            identity: identity.into(),
        },
    };
    let mut observation = crate::read_context::ScopeObservation::begin(root, target)?;
    let mut entries = Vec::new();
    let mut drafts = BTreeMap::new();
    for (relative, file) in &observation.entries {
        let (mut entry, draft, found) = selected_entry(
            root,
            relative,
            file,
            &observation.active,
            crate::layout::kind_in_scope(relative, &observation.path),
        )?;
        if entry.classification != Classification::Governed {
            let message = found
                .first()
                .map(|diagnostic| {
                    format!(
                        "{}: {}: {}",
                        diagnostic.path, diagnostic.code, diagnostic.message
                    )
                })
                .unwrap_or_else(|| format!("{relative}: requested record is invalid"));
            return Err(StoreError::Invalid(message));
        }
        if let Some(draft) = draft {
            drafts.insert(relative.clone(), draft);
        }
        entry.original_bytes = Vec::new();
        entries.push(entry);
    }
    if entries.len() == 1
        && entries[0].kind == Some(Kind::Ticket)
        && matches!(&entries[0].summary, Some(RecordSummary::WorkItem {status, ..}) if status == "accepted")
    {
        observation.observe_progress(root)?;
    }
    let mut diagnostics = Vec::new();
    append_progress(root, &observation, &mut entries, &mut diagnostics)?;
    let freshness = observation.verify(root)?;
    let scope = observation.target.scope();
    Ok(ScopedReadResult {
        freshness,
        drafts,
        project: scope.project.clone(),
        investigation: scope.investigation.clone(),
        path: observation.path,
        entries,
        diagnostics: stable(diagnostics),
    })
}

fn scoped_entries(
    root: &Path,
    observation: crate::read_context::ScopeObservation,
) -> Result<ScopedReadResult, StoreError> {
    let mut entries = Vec::new();
    let drafts = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let process = |(relative, file): (&String, &InventoryEntry)| {
        let kind = crate::layout::kind_in_scope(relative, &observation.path);
        let (mut entry, draft, found) =
            selected_entry(root, relative, file, &observation.active, kind)?;
        if matches!(entry.kind, Some(Kind::Ticket | Kind::Epic)) {
            entry.original_bytes = Vec::new();
        }
        drop(draft);
        Ok((entry, found))
    };
    let selected: Vec<Result<_, StoreError>> = if observation.entries.len() < 2 {
        observation
            .entries
            .iter()
            .map(|(path, entry)| process((path, entry)))
            .collect()
    } else {
        let pool = READ_POOL.as_ref().map_err(|error| {
            StoreError::Invalid(format!("read worker initialization failed: {error}"))
        })?;
        pool.install(|| {
            observation
                .entries
                .par_iter()
                .map(|(path, entry)| process((path, entry)))
                .collect()
        })
    };
    for result in selected {
        let (entry, mut found) = result?;
        entries.push(entry);
        diagnostics.append(&mut found);
    }
    append_progress(root, &observation, &mut entries, &mut diagnostics)?;
    let freshness = observation.verify(root)?;
    let scope = observation.target.scope();
    Ok(ScopedReadResult {
        freshness,
        drafts,
        project: scope.project.clone(),
        investigation: scope.investigation.clone(),
        path: observation.path,
        entries,
        diagnostics: stable(diagnostics),
    })
}

fn append_progress(
    root: &Path,
    observation: &crate::read_context::ScopeObservation,
    entries: &mut Vec<EntrySnapshot>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), StoreError> {
    if let Some((path, Some(file))) = &observation.progress {
        let (entry, _, mut found) =
            selected_entry(root, path, file, &observation.active, Some(Kind::Progress))?;
        let position = entries.partition_point(|entry| entry.path.as_str() < path.as_str());
        entries.insert(position, entry);
        diagnostics.append(&mut found);
    }
    Ok(())
}

fn selected_entry(
    root: &Path,
    path: &str,
    file: &InventoryEntry,
    active: &Activation,
    kind: Option<Kind>,
) -> Result<(EntrySnapshot, Option<RecordDraft>, Vec<Diagnostic>), StoreError> {
    let bytes = if file.kind == InventoryKind::Regular {
        read_observed_entry(root, path, file).map_err(|error| match error {
            StoreError::Invalid(message) => StoreError::Invalid(format!("{path}: {message}")),
            error => error,
        })?
    } else {
        Vec::new()
    };
    let parsed = if file.kind == InventoryKind::Regular {
        classify_facts(path, &bytes, active, kind)
    } else {
        classification::Classified {
            classification: invalid(
                path,
                kind,
                "unsafe_path",
                "governed paths must be regular non-symlink files",
            ),
            facts: classification::ParsedFacts::default(),
        }
    };
    let (classification, kind, identity, summary, diagnostics) = parsed.classification;
    Ok((
        EntrySnapshot {
            path: path.into(),
            classification,
            kind,
            identity,
            summary,
            content_revision: file.revision.clone(),
            original_bytes: bytes,
        },
        parsed.facts.draft,
        diagnostics,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_scope_read_reports_first_path_failure_and_retries_without_partial_results() {
        let root = tempfile::tempdir().unwrap();
        let path = "projects/demo/investigations/sample";
        fs::write(
            root.path().join("casefile.toml"),
            format!(
                "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['{path}']\n"
            ),
        )
        .unwrap();
        let tickets = root.path().join(format!("{path}/tickets/accepted"));
        fs::create_dir_all(&tickets).unwrap();
        for id in ["HMD-001", "HMD-002"] {
            fs::write(tickets.join(format!("{id}.md")), "original observation").unwrap();
        }
        let target = crate::ScopeReadTarget::RecordIndex {
            scope: crate::InvestigationScope {
                project: "demo".into(),
                investigation: "sample".into(),
            },
        };
        let observation =
            crate::read_context::ScopeObservation::begin(root.path(), target).unwrap();
        for id in ["HMD-002", "HMD-001"] {
            fs::write(
                tickets.join(format!("{id}.md")),
                "changed requested descriptor before guarded read",
            )
            .unwrap();
        }
        let error = scoped_entries(root.path(), observation)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("HMD-001.md"), "{error}");
        let retry = scoped_scan(root.path(), "demo", "sample", ScopedRead::RecordIndex).unwrap();
        assert_eq!(retry.entries.len(), 2);
        assert!(
            retry
                .entries
                .iter()
                .all(|entry| entry.classification == Classification::Invalid)
        );
        assert!(
            retry
                .entries
                .windows(2)
                .all(|entries| entries[0].path < entries[1].path)
        );
        assert_eq!(retry.diagnostics.len(), 2);
    }
}
