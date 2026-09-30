use super::*;

pub(super) fn progress_by_scope(
    scan: &ScanResult,
    facts: &mut BTreeMap<String, crate::scanning::classification::ParsedFacts>,
) -> (
    BTreeMap<RecordScope, BTreeMap<String, DerivedTicketProgress>>,
    BTreeSet<RecordScope>,
) {
    let diagnostic_paths = scan
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    let mut invalid = BTreeSet::new();
    for entry in scan
        .snapshot
        .entries
        .iter()
        .filter(|entry| entry.kind == Some(Kind::Progress))
    {
        let Some(scope) = record_scope(&entry.path, scan) else {
            continue;
        };
        if entry.classification != Classification::Governed
            || diagnostic_paths.contains(entry.path.as_str())
        {
            invalid.insert(scope);
            continue;
        }
        let Some(log) = facts
            .get_mut(&entry.path)
            .and_then(|facts| facts.progress.take())
        else {
            invalid.insert(scope);
            continue;
        };
        result.insert(scope, fold_progress(log.entries));
    }
    (result, invalid)
}

pub(crate) fn fold_progress(
    entries: Vec<ProgressEntry>,
) -> BTreeMap<String, DerivedTicketProgress> {
    let mut values = BTreeMap::new();
    for entry in entries {
        match entry {
            ProgressEntry::Transition {
                id,
                recorded_at,
                recorded_by,
                ticket_id,
                from,
                to,
            } => {
                let value = values
                    .entry(ticket_id)
                    .or_insert_with(|| DerivedTicketProgress {
                        status: ProgressStatus::Unknown,
                        last_transition: None,
                        notes: Vec::new(),
                    });
                value.status = to;
                value.last_transition = Some(DerivedProgressTransition {
                    id,
                    recorded_at,
                    recorded_by,
                    from,
                    to,
                });
            }
            ProgressEntry::Note {
                id,
                recorded_at,
                recorded_by,
                ticket_id,
                category,
                message,
            } => {
                values
                    .entry(ticket_id)
                    .or_insert_with(|| DerivedTicketProgress {
                        status: ProgressStatus::Unknown,
                        last_transition: None,
                        notes: Vec::new(),
                    })
                    .notes
                    .push(DerivedProgressNote {
                        id,
                        recorded_at,
                        recorded_by,
                        category,
                        message,
                    });
            }
        }
    }
    values
}
