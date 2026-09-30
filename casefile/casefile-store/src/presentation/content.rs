use super::*;

#[derive(Clone)]
pub(super) struct EmittedContent {
    pub(super) order: u64,
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
    let result = fetch_entry(&inner, &selected, &cancelled);
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
    cancelled: &AtomicBool,
) -> Result<PresentationEntry, StoreError> {
    let mut descriptor = emitted.descriptor.clone();
    let (_, active, _) = inner.reader.activation()?;
    descriptor.metadata = inner.reader.metadata(&descriptor.path)?;
    if descriptor.metadata.public.kind != PresentationFileKind::Regular {
        return Err(StoreError::Invalid(
            "selected content must remain a regular non-symlink file".into(),
        ));
    }
    let bytes = inner.reader.read(&descriptor.path, cancelled)?;
    check_cancelled(cancelled)?;
    descriptor.lazy = false;
    descriptor.kind = ScopeIndex::new(&active).resolve(&descriptor.path).kind;
    let classified = classify_facts(&descriptor.path, &bytes, &active, descriptor.kind);
    check_cancelled(cancelled)?;
    let file = parsed_file(&descriptor, bytes, classified, false);
    let mut entry = Arc::try_unwrap(file.entry).expect("requested content has one owner");
    entry.progress = PresentationFact::Unavailable;
    entry.boards = PresentationFact::Unavailable;
    Ok(entry)
}

pub(super) fn register_handles(
    state: &mut SessionLoadedState,
    order: u64,
    target: &PresentationTarget,
    entries: &[Arc<PresentationEntry>],
) {
    let handles = &mut state.handles;
    for entry in entries {
        let Some(handle) = &entry.content_handle else {
            continue;
        };
        let key = (target.clone(), entry.path.clone());
        if let Some(emitted) = handles.get_mut(&key) {
            if emitted.order > order {
                continue;
            }
            if emitted.descriptor.handle.as_ref() == Some(handle) {
                emitted.order = order;
                continue;
            }
        }
        handles.insert(
            key,
            EmittedContent {
                order,
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
        .state
        .lock()
        .expect("presentation state")
        .handles
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
