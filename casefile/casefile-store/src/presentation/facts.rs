use super::*;
use casefile_core::stable;
use std::borrow::Cow;

pub(super) struct LoadedFile {
    pub(super) snapshot: EntrySnapshot,
    pub(super) entry: Arc<PresentationEntry>,
    pub(super) local: Arc<DerivedRecord>,
    pub(super) diagnostics: Vec<Diagnostic>,
    pub(super) progress: Option<Arc<ProgressFacts>>,
    pub(super) retry: bool,
}

pub(super) struct ProgressFacts {
    pub(super) operations: Vec<(String, String)>,
    pub(super) folded: BTreeMap<String, DerivedTicketProgress>,
}

#[derive(Default)]
pub(super) struct ScopeFacts {
    progress_diagnostics: Arc<Vec<Diagnostic>>,
    binding_diagnostics: Arc<Vec<Diagnostic>>,
    boards: Arc<Vec<DerivedBoard>>,
}

fn ticket_membership(file: &LoadedFile) -> (Classification, &Option<String>, Option<&str>) {
    (
        file.snapshot.classification,
        &file.snapshot.identity,
        file.local
            .work_item
            .as_ref()
            .map(|item| item.status.as_str()),
    )
}

fn implementation(file: &LoadedFile) -> bool {
    file.snapshot.classification == Classification::Governed
        && matches!(&file.snapshot.summary, Some(RecordSummary::Strategy { phase, .. }) if phase == "implementation")
}

pub(super) fn project_scope(
    active: &Activation,
    files: &mut BTreeMap<String, Arc<LoadedFile>>,
    previous: Option<&LoadedScope>,
    cancelled: &AtomicBool,
) -> Result<ScopeFacts, StoreError> {
    check_cancelled(cancelled)?;
    let changed = |path: &str, file: &Arc<LoadedFile>| {
        previous
            .and_then(|scope| scope.files.get(path))
            .is_none_or(|old| !Arc::ptr_eq(old, file))
    };
    let removed = previous
        .into_iter()
        .flat_map(|scope| scope.files.iter())
        .filter(|(path, _)| !files.contains_key(*path));
    let membership_changed = files.iter().any(|(path, file)| {
        changed(path, file)
            && (file.snapshot.kind == Some(Kind::Progress)
                || file.snapshot.kind == Some(Kind::Ticket)
                    && previous
                        .and_then(|scope| scope.files.get(path))
                        .is_none_or(|old| ticket_membership(old) != ticket_membership(file)))
    }) || removed
        .clone()
        .any(|(_, file)| matches!(file.snapshot.kind, Some(Kind::Progress | Kind::Ticket)));
    let binding_changed = files.iter().any(|(path, file)| {
        changed(path, file)
            && (implementation(file)
                || file.snapshot.kind == Some(Kind::StrategyBinding)
                || previous
                    .and_then(|scope| scope.files.get(path))
                    .is_some_and(|old| implementation(old)))
    }) || removed
        .clone()
        .any(|(_, file)| implementation(file) || file.snapshot.kind == Some(Kind::StrategyBinding));
    let progress_diagnostics = if let Some(previous) = previous.filter(|_| !membership_changed) {
        previous.facts.progress_diagnostics.clone()
    } else {
        Arc::new(crate::validation::progress_diagnostics(
            files.values().map(|file| &file.snapshot),
            active,
            files.values().filter_map(|file| {
                file.progress
                    .as_ref()
                    .map(|facts| (file.snapshot.path.as_str(), facts.operations.as_slice()))
            }),
        ))
    };
    let binding_diagnostics = if let Some(previous) = previous.filter(|_| !binding_changed) {
        previous.facts.binding_diagnostics.clone()
    } else {
        let mut validation = crate::validation::ValidationFacts::default();
        let snapshots = files
            .values()
            .map(|file| {
                if file.snapshot.kind == Some(Kind::Strategy) {
                    validation.strategies.insert(
                        file.snapshot.path.clone(),
                        file.local
                            .strategy
                            .as_ref()
                            .map(|strategy| strategy.matrix.clone()),
                    );
                }
                file.snapshot.clone()
            })
            .collect::<Vec<_>>();
        Arc::new(binding_diagnostics_facts(&snapshots, &validation))
    };
    let log = files
        .values()
        .find(|file| file.snapshot.kind == Some(Kind::Progress));
    let invalid = log.is_some_and(|file| {
        file.snapshot.classification != Classification::Governed
            || !file.diagnostics.is_empty()
            || progress_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path == file.snapshot.path)
    });
    let empty = BTreeMap::new();
    let folded = log
        .and_then(|file| file.progress.as_ref())
        .map(|facts| &facts.folded)
        .unwrap_or(&empty);
    let progress_changed =
        membership_changed || log.is_some_and(|file| changed(&file.snapshot.path, file));
    let selected = files.values().find(|file| implementation(file));
    let binding = files
        .values()
        .find(|file| file.snapshot.kind == Some(Kind::StrategyBinding));
    let mut records = BTreeMap::new();
    for (path, file) in files.iter() {
        check_cancelled(cancelled)?;
        let body_changed = changed(path, file);
        let binding_affected = binding_changed
            && matches!(
                file.snapshot.kind,
                Some(Kind::Strategy | Kind::StrategyBinding)
            );
        let old = file.entry.derived.as_ref().unwrap();
        let wanted_progress = ticket_progress(&file.local, folded, invalid);
        if !body_changed
            && !binding_affected
            && (!progress_changed || old.progress.as_ref() == wanted_progress.as_deref())
        {
            records.insert(path.clone(), Cow::Borrowed(old));
            continue;
        }
        let mut record = (*file.local).clone();
        record.progress = wanted_progress.map(Cow::into_owned);
        project_binding(
            &mut record,
            &file.snapshot.summary,
            selected.map(|file| (&file.snapshot, file.local.as_ref())),
            binding.map(|file| (&file.snapshot, file.local.as_ref())),
        );
        records.insert(path.clone(), Cow::Owned(record));
    }
    check_cancelled(cancelled)?;
    let boards_changed = previous.is_none()
        || removed
            .clone()
            .any(|(_, file)| file.local.board.is_some() || file.local.work_item.is_some())
        || records.iter().any(|(path, record)| {
            if record.board.is_none()
                && record.work_item.is_none()
                && previous
                    .and_then(|scope| scope.files.get(path))
                    .is_none_or(|old| old.local.board.is_none() && old.local.work_item.is_none())
            {
                return false;
            }
            previous
                .and_then(|scope| scope.files.get(path))
                .is_none_or(|old| {
                    let old = old.entry.derived.as_ref().unwrap();
                    if old.board.is_none()
                        && record.board.is_none()
                        && old.work_item.is_none()
                        && record.work_item.is_none()
                    {
                        return false;
                    }
                    old.board != record.board
                        || old.classification != record.classification
                        || old.identity != record.identity
                        || old
                            .work_item
                            .as_ref()
                            .map(|item| (&item.id, &item.title, &item.status, item.rank))
                            != record
                                .work_item
                                .as_ref()
                                .map(|item| (&item.id, &item.title, &item.status, item.rank))
                        || old.progress.as_ref().map(|progress| progress.status)
                            != record.progress.as_ref().map(|progress| progress.status)
                })
        });
    let boards = if boards_changed {
        Arc::new(derive_boards(records.values().map(Cow::as_ref)))
    } else {
        previous.unwrap().facts.boards.clone()
    };
    let mut diagnostics = BTreeMap::<&str, Vec<&Diagnostic>>::new();
    for diagnostic in progress_diagnostics
        .iter()
        .chain(binding_diagnostics.iter())
    {
        diagnostics
            .entry(&diagnostic.path)
            .or_default()
            .push(diagnostic);
    }
    let mut board_index = BTreeMap::<&crate::derived::ScopedIdentity, Vec<&DerivedBoard>>::new();
    for board in boards.iter() {
        board_index.entry(&board.identity).or_default().push(board);
    }
    let dependencies_unchanged = previous.is_some_and(|old| {
        Arc::ptr_eq(&old.facts.progress_diagnostics, &progress_diagnostics)
            && Arc::ptr_eq(&old.facts.binding_diagnostics, &binding_diagnostics)
            && Arc::ptr_eq(&old.facts.boards, &boards)
    });
    check_cancelled(cancelled)?;
    let mut updates = Vec::new();
    for (path, record) in records {
        check_cancelled(cancelled)?;
        if dependencies_unchanged && matches!(record, Cow::Borrowed(_)) {
            continue;
        }
        let file = &files[&path];
        let local_diagnostics = file.diagnostics.iter().chain(
            diagnostics
                .get(path.as_str())
                .into_iter()
                .flatten()
                .copied(),
        );
        let local_boards = record.identity.as_ref().and_then(|id| board_index.get(id));
        let diagnostics_equal = matches!(&file.entry.diagnostics,
            PresentationFact::Available(old) if old.iter().eq(local_diagnostics.clone()));
        let boards_equal = matches!(&file.entry.boards,
            PresentationFact::Available(old) if old.iter().eq(
                local_boards.into_iter().flatten().copied()));
        if file.entry.derived.as_ref() == Some(record.as_ref()) && diagnostics_equal && boards_equal
        {
            continue;
        }
        updates.push((
            path.clone(),
            record.into_owned(),
            stable(local_diagnostics.cloned().collect()),
            local_boards
                .into_iter()
                .flatten()
                .map(|board| (*board).clone())
                .collect(),
        ));
    }
    for (path, record, diagnostics, boards) in updates {
        check_cancelled(cancelled)?;
        let file = files.get_mut(&path).unwrap();
        if Arc::get_mut(file).is_none() {
            *file = Arc::new(LoadedFile {
                snapshot: file.snapshot.clone(),
                entry: file.entry.clone(),
                local: file.local.clone(),
                diagnostics: file.diagnostics.clone(),
                progress: file.progress.clone(),
                retry: file.retry,
            });
        }
        let file = Arc::get_mut(file).unwrap();
        if Arc::get_mut(&mut file.entry).is_none() {
            file.entry = Arc::new((*file.entry).clone());
        }
        let entry = Arc::get_mut(&mut file.entry).unwrap();
        entry.progress = PresentationFact::Available(record.progress.clone());
        entry.derived = Some(record);
        entry.diagnostics = PresentationFact::Available(diagnostics);
        entry.boards = PresentationFact::Available(boards);
    }
    Ok(ScopeFacts {
        progress_diagnostics,
        binding_diagnostics,
        boards,
    })
}
