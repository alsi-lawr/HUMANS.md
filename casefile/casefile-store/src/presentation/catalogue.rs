use super::*;

#[derive(Clone)]
pub(super) struct Descriptor {
    pub(super) path: String,
    pub(super) metadata: ReaderMetadata,
    pub(super) scope: Option<PresentationScope>,
    pub(super) kind: Option<Kind>,
    pub(super) lazy: bool,
    pub(super) handle: Option<PresentationContentHandle>,
}

pub(super) fn catalogue(
    state: ActivationState,
    activation: &Activation,
    diagnostics: Vec<Diagnostic>,
) -> PresentationCatalogue {
    let projects = if state == ActivationState::Active {
        activation
            .projects
            .iter()
            .map(|(slug, project)| PresentationProject {
                slug: slug.clone(),
                prefix: project.prefix.clone(),
                investigations: project
                    .investigations
                    .iter()
                    .filter_map(|path| {
                        investigation_identity(slug, path).map(|identity| {
                            PresentationInvestigation {
                                identity: identity.into(),
                                path: path.clone(),
                            }
                        })
                    })
                    .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };
    PresentationCatalogue {
        activation: state,
        projects,
        diagnostics,
    }
}

pub(super) fn validate_target(
    target: &PresentationTarget,
    active: &Activation,
) -> Result<(), StoreError> {
    let valid = match target {
        PresentationTarget::Store => true,
        PresentationTarget::Project { project } => active.projects.contains_key(project),
        PresentationTarget::Investigation { project, path } => active
            .projects
            .get(project)
            .is_some_and(|value| value.investigations.contains(path)),
    };
    valid.then_some(()).ok_or_else(|| {
        StoreError::Invalid("presentation target is not present in the activation catalogue".into())
    })
}

pub(super) fn collect_descriptors(
    inner: &SessionInner,
    target: &PresentationTarget,
    active: &Activation,
    cancelled: &AtomicBool,
) -> Result<Vec<Descriptor>, StoreError> {
    let scopes = ScopeIndex::new(active);
    let previous_scopes = inner
        .state
        .lock()
        .expect("presentation state")
        .scopes
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let previous = previous_scopes
        .iter()
        .flat_map(|scope| scope.descriptors.iter())
        .map(|descriptor| (descriptor.path.as_str(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let start = target_root(target);
    let mut pending = vec![start];
    let mut descriptors = Vec::new();
    while let Some(directory) = pending.pop() {
        check_cancelled(cancelled)?;
        let mut children = inner.reader.read_dir(&directory, cancelled)?;
        children.sort();
        for path in children.into_iter().rev() {
            check_cancelled(cancelled)?;
            if is_store_path_excluded(Path::new(&path)) {
                continue;
            }
            let metadata = match inner.reader.metadata(&path) {
                Ok(metadata) => metadata,
                Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if metadata.public.kind == PresentationFileKind::Directory {
                pending.push(path);
                continue;
            }
            if metadata.public.kind == PresentationFileKind::Other {
                continue;
            }
            let resolved = scopes.resolve(&path);
            let kind = resolved.kind;
            let scope = resolved.project.map(|project| PresentationScope {
                project: project.into(),
                investigation: resolved
                    .scope
                    .and_then(|base| investigation_identity(project, base))
                    .map(Into::into),
            });
            let lazy = metadata.public.kind == PresentationFileKind::Regular
                && (kind == Some(Kind::Evidence) || kind.is_none());
            let safe = normalize_planning_relative(&path).is_ok_and(|canonical| canonical == path);
            let handle = (lazy && safe).then(|| {
                previous
                    .get(path.as_str())
                    .filter(|old| {
                        old.metadata == metadata
                            && old.kind == kind
                            && old.scope == scope
                            && old.lazy
                    })
                    .and_then(|old| old.handle.clone())
                    .unwrap_or_else(|| PresentationContentHandle {
                        session: inner.session_id,
                        id: inner.next_handle.fetch_add(1, Ordering::Relaxed),
                        path: path.clone(),
                    })
            });
            descriptors.push(Descriptor {
                scope,
                path,
                metadata,
                kind,
                lazy,
                handle,
            });
        }
    }
    descriptors.sort_by(|left, right| (&left.scope, &left.path).cmp(&(&right.scope, &right.path)));
    Ok(descriptors)
}

pub(super) fn target_root(target: &PresentationTarget) -> String {
    match target {
        PresentationTarget::Store => String::new(),
        PresentationTarget::Project { project } => format!("projects/{project}"),
        PresentationTarget::Investigation { path, .. } => path.clone(),
    }
}

pub(super) fn target_contains(target: &PresentationTarget, path: &str) -> bool {
    match target {
        PresentationTarget::Store => true,
        PresentationTarget::Project { project } => path
            .strip_prefix("projects/")
            .and_then(|rest| rest.strip_prefix(project))
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/')),
        PresentationTarget::Investigation { path: root, .. } => path
            .strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/')),
    }
}
