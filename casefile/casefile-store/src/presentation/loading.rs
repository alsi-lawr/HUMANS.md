use super::*;

pub(super) fn run_load(
    inner: Arc<SessionInner>,
    request: PresentationLoadRequest,
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
            let loaded = if let Some(previous) = previous.as_ref().filter(|previous| {
                previous.reusable
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
            inner
                .state
                .lock()
                .expect("presentation state")
                .scopes
                .insert(scope[0].scope.clone(), loaded.clone());
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
        inner
            .state
            .lock()
            .expect("presentation state")
            .scopes
            .retain(|scope, loaded| {
                current_scopes.contains(scope)
                    || !loaded
                        .descriptors
                        .iter()
                        .any(|descriptor| target_contains(&request.target, &descriptor.path))
            });
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
