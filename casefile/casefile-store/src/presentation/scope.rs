use super::*;

pub(super) struct LoadedScope {
    pub(super) descriptors: Vec<Descriptor>,
    pub(super) snapshots: Vec<EntrySnapshot>,
    pub(super) entries: Vec<Arc<PresentationEntry>>,
    pub(super) diagnostics: Vec<Diagnostic>,
    pub(super) reusable: bool,
}

pub(super) fn load_scope(
    inner: &SessionInner,
    active: &Activation,
    descriptors: &[Descriptor],
    previous: Option<&LoadedScope>,
    cancelled: &AtomicBool,
) -> Result<LoadedScope, StoreError> {
    let previous_entries = previous
        .into_iter()
        .flat_map(|scope| scope.snapshots.iter())
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut snapshots = Vec::new();
    let mut kept = Vec::new();
    let mut diagnostics = Vec::new();
    let mut reusable = true;
    for descriptor in descriptors {
        if cancelled.load(Ordering::Acquire) {
            return Err(StoreError::Invalid("presentation load cancelled".into()));
        }
        let mut descriptor = descriptor.clone();
        if descriptor.lazy {
            kept.push(descriptor);
            continue;
        }
        let mut snapshot = EntrySnapshot {
            path: descriptor.path.clone(),
            classification: Classification::Raw,
            kind: descriptor.kind,
            identity: None,
            content_revision: descriptor.metadata.public.revision.clone(),
            summary: None,
            original_bytes: Vec::new(),
        };
        if descriptor.metadata.public.kind == PresentationFileKind::Symlink {
            snapshot.classification = Classification::Invalid;
            diagnostics.push(Diagnostic::new(
                &descriptor.path,
                "unsafe_path",
                "governed paths cannot be symlinks",
            ));
        } else if let Some(cached) =
            previous_entries
                .get(descriptor.path.as_str())
                .filter(|cached| {
                    cached.content_revision == descriptor.metadata.public.revision
                        && !previous.is_some_and(|scope| {
                            scope.diagnostics.iter().any(|diagnostic| {
                                diagnostic.path == descriptor.path
                                    && diagnostic.code == "presentation_read"
                            })
                        })
                })
        {
            snapshot = (*cached).clone();
            diagnostics.extend(
                previous
                    .into_iter()
                    .flat_map(|scope| scope.diagnostics.iter())
                    .filter(|d| d.path == descriptor.path)
                    .cloned(),
            );
        } else {
            match inner.reader.read(&descriptor.path) {
                Ok(bytes) => {
                    let (classification, kind, identity, summary, local) =
                        classify(&descriptor.path, &bytes, active);
                    snapshot.classification = classification;
                    snapshot.kind = kind;
                    snapshot.identity = identity;
                    snapshot.summary = summary;
                    snapshot.original_bytes = bytes;
                    diagnostics.extend(local);
                    // A save during the read is reconciled again on the next refresh, never rejected globally.
                    if let Ok(metadata) = inner.reader.metadata(&descriptor.path) {
                        if metadata == descriptor.metadata {
                            snapshot.content_revision = metadata.public.revision.clone();
                        } else {
                            reusable = false;
                            snapshot.content_revision = Revision("display-read-raced".into());
                        }
                        descriptor.metadata = metadata;
                    }
                }
                Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    reusable = false;
                    if let Some(cached) = previous_entries.get(descriptor.path.as_str()) {
                        snapshot = (*cached).clone();
                    } else {
                        snapshot.classification = Classification::Invalid;
                    }
                    diagnostics.push(Diagnostic::new(
                        &descriptor.path,
                        "presentation_read",
                        error.to_string(),
                    ));
                }
            }
        }
        snapshots.push(snapshot);
        kept.push(descriptor);
    }
    // These facts depend only on this complete scope, not on other loaded investigations.
    let local_diagnostics = diagnostics.clone();
    diagnostics.extend(binding_diagnostics(&snapshots));
    diagnostics.extend(crate::validation::progress_diagnostics(&snapshots, active));
    let scan = ScanResult {
        activation: ActivationState::Active,
        investigation_roots: investigation_roots(active),
        snapshot: CasefileSnapshot {
            revision: Revision("presentation".into()),
            entries: snapshots,
        },
        diagnostics: stable(diagnostics),
    };
    let derived = derive_presentation_snapshot(&scan);
    let indexes = PresentationIndexes::new(&scan, &derived);
    let entries = kept
        .iter()
        .map(|descriptor| {
            Arc::new(if descriptor.lazy {
                catalogue_entry(descriptor)
            } else {
                presentation_entry(descriptor, &indexes)
            })
        })
        .collect();
    Ok(LoadedScope {
        descriptors: kept,
        snapshots: scan.snapshot.entries,
        entries,
        diagnostics: local_diagnostics,
        reusable,
    })
}
