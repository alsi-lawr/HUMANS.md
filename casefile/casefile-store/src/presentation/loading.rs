use super::*;

pub(super) fn run_load(
    inner: Arc<SessionInner>,
    request: PresentationLoadRequest,
    order: u64,
    sender: SyncSender<PresentationEvent>,
    cancelled: Arc<AtomicBool>,
) {
    let mut catalogue_emitted = false;
    let result = (|| -> Result<(), StoreError> {
        let (state, activation, diagnostics) = inner.reader.activation()?;
        let catalogue = catalogue(state, &activation, diagnostics);
        if !send_bounded(
            &sender,
            &cancelled,
            PresentationEvent::Catalogue {
                generation: request.generation,
                target: request.target.clone(),
                coverage: coverage(PresentationCoverageState::Pending),
                progress: PresentationProgress {
                    completed: 0,
                    total: None,
                },
                catalogue,
            },
        ) {
            return Ok(());
        }
        catalogue_emitted = true;
        if state != ActivationState::Active {
            send_bounded(
                &sender,
                &cancelled,
                PresentationEvent::Complete {
                    generation: request.generation,
                    target: request.target.clone(),
                    coverage: coverage(PresentationCoverageState::Complete),
                    progress: PresentationProgress {
                        completed: 0,
                        total: Some(0),
                    },
                },
            );
            return Ok(());
        }
        validate_target(&request.target, &activation)?;
        let descriptors = collect_descriptors(&inner, &request.target, &activation, &cancelled)?;
        let total = descriptors.len();
        let current_scopes = descriptors
            .iter()
            .map(|descriptor| descriptor.scope.clone())
            .collect::<BTreeSet<_>>();
        let mut completed = 0;
        let mut pending = BTreeMap::new();
        for scope in descriptors.chunk_by(|left, right| left.scope == right.scope) {
            if cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            let previous = inner
                .state
                .lock()
                .expect("presentation state")
                .scopes
                .get(&scope[0].scope)
                .cloned();
            let project_context = project_context(&activation, scope);
            let loaded = if let Some(previous) = previous.as_ref().filter(|previous| {
                previous.reusable
                    && previous.project_context == project_context
                    && previous.descriptors.len() == scope.len()
                    && previous.descriptors.iter().zip(scope).all(|(a, b)| {
                        a.path == b.path && a.metadata == b.metadata && a.kind == b.kind
                    })
            }) {
                previous.clone()
            } else {
                Arc::new(load_scope(
                    &inner,
                    &activation,
                    scope,
                    previous.as_deref(),
                    &cancelled,
                )?)
            };
            if cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            pending.insert(scope[0].scope.clone(), loaded.clone());
            for chunk in loaded.entries.chunks(PRESENTATION_BATCH_LIMIT) {
                completed += chunk.len();
                if !send_bounded(
                    &sender,
                    &cancelled,
                    PresentationEvent::Entries {
                        generation: request.generation,
                        target: request.target.clone(),
                        coverage: coverage(PresentationCoverageState::Partial),
                        progress: PresentationProgress {
                            completed,
                            total: Some(total),
                        },
                        entries: chunk.to_vec(),
                    },
                ) {
                    return Ok(());
                }
            }
        }
        check_cancelled(&cancelled)?;
        let current_handles = pending
            .values()
            .flat_map(|scope| &scope.descriptors)
            .map(|descriptor| (descriptor.path.as_str(), &descriptor.handle))
            .collect::<BTreeMap<_, _>>();
        {
            let mut state = inner.state.lock().expect("presentation state");
            if !state.current(&request.target, order) {
                return Ok(());
            }
            state.scopes.retain(|scope, loaded| {
                current_scopes.contains(scope)
                    || !loaded
                        .descriptors
                        .iter()
                        .any(|descriptor| target_contains(&request.target, &descriptor.path))
            });
            // A completed scope refresh invalidates only changed/deleted lazy descriptors, across views.
            state.handles.retain(|(_, path), emitted| {
                !target_contains(&request.target, path)
                    || current_handles
                        .get(path.as_str())
                        .is_some_and(|handle| **handle == emitted.descriptor.handle)
            });
            drop(current_handles);
            state.scopes.extend(pending);
        }
        send_bounded(
            &sender,
            &cancelled,
            PresentationEvent::Complete {
                generation: request.generation,
                target: request.target.clone(),
                coverage: coverage(PresentationCoverageState::Complete),
                progress: PresentationProgress {
                    completed,
                    total: Some(completed),
                },
            },
        );
        Ok(())
    })();
    if let Err(error) = result {
        send_bounded(
            &sender,
            &cancelled,
            PresentationEvent::Failure {
                generation: request.generation,
                target: request.target,
                coverage: PresentationCoverage {
                    catalogue: if catalogue_emitted {
                        PresentationCoverageState::Complete
                    } else {
                        PresentationCoverageState::Pending
                    },
                    payload: PresentationCoverageState::Pending,
                    facts: PresentationCoverageState::Pending,
                },
                progress: PresentationProgress {
                    completed: 0,
                    total: None,
                },
                message: error.to_string(),
            },
        );
    }
}

pub(super) fn coverage(payload: PresentationCoverageState) -> PresentationCoverage {
    PresentationCoverage {
        catalogue: PresentationCoverageState::Complete,
        payload,
        facts: payload,
    }
}
