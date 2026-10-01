use super::*;

pub(super) struct LoadedScope {
    pub(super) descriptors: Vec<Descriptor>,
    pub(super) files: BTreeMap<String, Arc<LoadedFile>>,
    pub(super) entries: Vec<Arc<PresentationEntry>>,
    pub(super) facts: ScopeFacts,
    pub(super) reusable: bool,
    pub(super) project_context: Vec<String>,
}

pub(super) fn project_context(active: &Activation, descriptors: &[Descriptor]) -> Vec<String> {
    if descriptors
        .iter()
        .any(|descriptor| descriptor.kind == Some(Kind::ProjectMap))
    {
        active.projects.keys().cloned().collect()
    } else {
        Vec::new()
    }
}

pub(super) fn load_scope(
    inner: &SessionInner,
    active: &Activation,
    descriptors: &[Descriptor],
    previous: Option<&LoadedScope>,
    cancelled: &AtomicBool,
) -> Result<LoadedScope, StoreError> {
    let project_context = project_context(active, descriptors);
    let old_descriptors = previous
        .into_iter()
        .flat_map(|scope| scope.descriptors.iter())
        .map(|descriptor| (descriptor.path.as_str(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let old_entries = previous
        .into_iter()
        .flat_map(|scope| scope.entries.iter())
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut files = BTreeMap::new();
    let mut lazy = BTreeMap::new();
    let mut kept = Vec::new();
    let mut reusable = true;
    for descriptor in descriptors {
        check_cancelled(cancelled)?;
        let old = old_descriptors.get(descriptor.path.as_str()).filter(|old| {
            old.metadata == descriptor.metadata
                && old.kind == descriptor.kind
                && old.scope == descriptor.scope
                && (descriptor.kind != Some(Kind::ProjectMap)
                    || previous.is_some_and(|scope| scope.project_context == project_context))
        });
        if descriptor.lazy {
            let cached = old.and_then(|_| old_entries.get(descriptor.path.as_str()).copied());
            lazy.insert(
                descriptor.path.clone(),
                cached
                    .cloned()
                    .unwrap_or_else(|| Arc::new(catalogue_entry(descriptor))),
            );
            kept.push(descriptor.clone());
            continue;
        }
        let cached = previous.and_then(|scope| scope.files.get(&descriptor.path));
        if old.is_some() && cached.is_some_and(|file| !file.retry) {
            files.insert(descriptor.path.clone(), cached.unwrap().clone());
            kept.push(descriptor.clone());
            continue;
        }
        let mut descriptor = descriptor.clone();
        let file = if descriptor.metadata.public.kind == PresentationFileKind::Symlink {
            parsed_file(
                &descriptor,
                Vec::new(),
                crate::scanning::classification::Classified {
                    classification: (
                        Classification::Invalid,
                        descriptor.kind,
                        None,
                        None,
                        vec![Diagnostic::new(
                            &descriptor.path,
                            "unsafe_path",
                            "governed paths cannot be symlinks",
                        )],
                    ),
                    facts: Default::default(),
                },
                false,
            )
        } else {
            match inner.reader.read(&descriptor.path, cancelled) {
                Ok(bytes) => {
                    check_cancelled(cancelled)?;
                    let classified =
                        classify_facts(&descriptor.path, &bytes, active, descriptor.kind);
                    let mut raced = false;
                    if let Ok(metadata) = inner.reader.metadata(&descriptor.path) {
                        raced = metadata != descriptor.metadata;
                        descriptor.metadata = metadata;
                    }
                    reusable &= !raced;
                    parsed_file(&descriptor, bytes, classified, raced)
                }
                Err(StoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    check_cancelled(cancelled)?;
                    reusable = false;
                    let diagnostic =
                        Diagnostic::new(&descriptor.path, "presentation_read", error.to_string());
                    let mut file = if let Some(cached) = cached {
                        LoadedFile {
                            snapshot: cached.snapshot.clone(),
                            entry: cached.entry.clone(),
                            local: cached.local.clone(),
                            diagnostics: cached.diagnostics.clone(),
                            progress: cached.progress.clone(),
                            retry: true,
                        }
                    } else {
                        parsed_file(
                            &descriptor,
                            Vec::new(),
                            crate::scanning::classification::Classified {
                                classification: (
                                    Classification::Invalid,
                                    descriptor.kind,
                                    None,
                                    None,
                                    Vec::new(),
                                ),
                                facts: Default::default(),
                            },
                            true,
                        )
                    };
                    file.diagnostics.push(diagnostic);
                    file
                }
            }
        };
        files.insert(descriptor.path.clone(), Arc::new(file));
        kept.push(descriptor);
    }
    check_cancelled(cancelled)?;
    let facts = project_scope(active, &mut files, previous, cancelled)?;
    check_cancelled(cancelled)?;
    let entries = kept
        .iter()
        .map(|descriptor| {
            lazy.get(&descriptor.path)
                .cloned()
                .unwrap_or_else(|| files[&descriptor.path].entry.clone())
        })
        .collect();
    Ok(LoadedScope {
        descriptors: kept,
        files,
        entries,
        facts,
        reusable,
        project_context,
    })
}
