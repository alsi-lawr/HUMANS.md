use super::*;

#[derive(Clone)]
pub(super) struct EmittedContent {
    pub(super) generation: u64,
    pub(super) descriptor: Descriptor,
    pub(super) target: PresentationTarget,
}

pub(super) fn run_content(
    inner: Arc<SessionInner>,
    request: PresentationContentRequest,
    sender: SyncSender<PresentationContentEvent>,
    cancelled: Arc<AtomicBool>,
) {
    let selected = select_emitted(&inner, &request.target, &request.selector);
    let selected = match selected {
        Ok(value) if value.target == request.target => value,
        Ok(_) => {
            send_content_failure(
                &sender,
                &cancelled,
                &request,
                None,
                "content target does not match the emitted entry",
            );
            return;
        }
        Err((path, message)) => {
            send_content_failure(&sender, &cancelled, &request, path, &message);
            return;
        }
    };
    if !send_bounded(
        &sender,
        &cancelled,
        PresentationContentEvent::Pending {
            generation: request.generation,
            target: request.target.clone(),
            path: selected.descriptor.path.clone(),
        },
    ) {
        return;
    }
    let result = fetch_entry(&inner, &selected);
    let event = match result {
        Ok(entry) => PresentationContentEvent::Loaded {
            generation: request.generation,
            target: request.target,
            entry: Box::new(entry),
        },
        Err(error) => PresentationContentEvent::Failure {
            generation: request.generation,
            target: request.target,
            path: Some(selected.descriptor.path),
            message: error.to_string(),
        },
    };
    send_bounded(&sender, &cancelled, event);
}

pub(super) fn fetch_entry(
    inner: &SessionInner,
    emitted: &EmittedContent,
) -> Result<PresentationEntry, StoreError> {
    let mut descriptor = emitted.descriptor.clone();
    let (_, active, _) = inner.reader.activation()?;
    descriptor.metadata = inner.reader.metadata(&descriptor.path)?;
    if descriptor.metadata.public.kind != PresentationFileKind::Regular {
        return Err(StoreError::Invalid(
            "selected content must remain a regular non-symlink file".into(),
        ));
    }
    let bytes = inner.reader.read(&descriptor.path)?;
    descriptor.lazy = false;
    let (classification, kind, identity, summary, diagnostics) =
        classify(&descriptor.path, &bytes, &active);
    let scan = ScanResult {
        activation: ActivationState::Active,
        investigation_roots: investigation_roots(&active),
        snapshot: CasefileSnapshot {
            revision: Revision("presentation".into()),
            entries: vec![EntrySnapshot {
                path: descriptor.path.clone(),
                classification,
                kind,
                identity,
                summary,
                content_revision: descriptor.metadata.public.revision.clone(),
                original_bytes: bytes,
            }],
        },
        diagnostics,
    };
    let derived = derive_presentation_snapshot(&scan);
    let indexes = PresentationIndexes::new(&scan, &derived);
    let mut entry = presentation_entry(&descriptor, &indexes);
    entry.progress = PresentationFact::Unavailable;
    entry.boards = PresentationFact::Unavailable;
    Ok(entry)
}

pub(super) fn register_handles(
    inner: &SessionInner,
    generation: u64,
    target: &PresentationTarget,
    entries: &[Arc<PresentationEntry>],
) {
    let mut handles = inner.handles.lock().expect("presentation handles");
    for entry in entries {
        let Some(handle) = &entry.content_handle else {
            continue;
        };
        let key = (target.clone(), entry.path.clone());
        if let Some(emitted) = handles.get_mut(&key) {
            if emitted.generation > generation {
                continue;
            }
            if emitted.descriptor.handle.as_ref() == Some(handle) {
                emitted.generation = generation;
                continue;
            }
        }
        handles.insert(
            key,
            EmittedContent {
                generation,
                target: target.clone(),
                descriptor: Descriptor {
                    path: entry.path.clone(),
                    metadata: ReaderMetadata {
                        public: entry.metadata.clone(),
                    },
                    scope: entry.scope.clone(),
                    kind: entry.kind,
                    lazy: true,
                    handle: Some(handle.clone()),
                },
            },
        );
    }
}

pub(super) fn select_emitted(
    inner: &SessionInner,
    target: &PresentationTarget,
    selector: &PresentationContentSelector,
) -> Result<EmittedContent, (Option<String>, String)> {
    let path = match selector {
        PresentationContentSelector::Handle { handle } => {
            if handle.session != inner.session_id {
                return Err((
                    Some(handle.path.clone()),
                    "content handle was not emitted by this session".into(),
                ));
            }
            handle.path.as_str()
        }
        PresentationContentSelector::Path { path } => {
            let canonical = normalize_planning_relative(path)
                .map_err(|message| (Some(path.clone()), message.into()))?;
            if canonical != *path || is_store_path_excluded(Path::new(path)) {
                return Err((
                    Some(path.clone()),
                    "content path must be canonical, contained, and included".into(),
                ));
            }
            path.as_str()
        }
    };
    let emitted = inner
        .handles
        .lock()
        .expect("presentation handles")
        .get(&(target.clone(), path.into()))
        .cloned()
        .ok_or_else(|| {
            (
                Some(path.into()),
                "content path was not emitted by this session".into(),
            )
        })?;
    if let PresentationContentSelector::Handle { handle } = selector {
        if emitted.descriptor.handle.as_ref() != Some(handle) {
            return Err((
                Some(path.into()),
                "content handle was superseded by a newer entry".into(),
            ));
        }
    }
    Ok(emitted)
}

pub(super) fn send_content_failure(
    sender: &SyncSender<PresentationContentEvent>,
    cancelled: &AtomicBool,
    request: &PresentationContentRequest,
    path: Option<String>,
    message: &str,
) {
    send_bounded(
        sender,
        cancelled,
        PresentationContentEvent::Failure {
            generation: request.generation,
            target: request.target.clone(),
            path,
            message: message.into(),
        },
    );
}
