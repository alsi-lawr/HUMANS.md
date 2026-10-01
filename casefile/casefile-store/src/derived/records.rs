use super::*;

pub(crate) fn local_record(
    entry: &EntrySnapshot,
    scope: Option<RecordScope>,
    parsed: crate::scanning::classification::ParsedFacts,
    retain_display_payload: bool,
) -> DerivedRecord {
    let title = entry.summary.as_ref().map_or_else(
        || entry.identity.clone().unwrap_or_else(|| entry.path.clone()),
        summary_title,
    );
    let (work_item, board) = match parsed.draft {
        Some(RecordDraft::Ticket(item) | RecordDraft::Epic(item)) => {
            (Some(DerivedWorkItem::from(item)), None)
        }
        Some(RecordDraft::Board(board)) => (None, Some(board)),
        None => (None, None),
    };
    let strategy = match &entry.summary {
        Some(RecordSummary::Strategy { .. }) => parsed.strategy.map(|matrix| DerivedStrategy {
            matrix,
            binding: None,
        }),
        _ => None,
    };
    let strategy_binding = match &entry.summary {
        Some(RecordSummary::StrategyBinding { binding }) => Some(DerivedStrategyBinding {
            binding: binding.clone(),
            state: StrategyBindingState::Pending,
        }),
        _ => None,
    };
    let identity = entry
        .identity
        .as_ref()
        .zip(scope.clone())
        .map(|(id, scope)| ScopedIdentity {
            scope,
            identity: id.into(),
        });
    DerivedRecord {
        path: entry.path.clone(),
        scope,
        classification: entry.classification,
        kind: entry.kind,
        identity,
        title,
        content: retain_display_payload
            .then(|| {
                std::str::from_utf8(&entry.original_bytes)
                    .ok()
                    .map(str::to_owned)
            })
            .flatten(),
        work_item,
        progress: None,
        board,
        strategy,
        strategy_binding,
    }
}

pub(crate) fn ticket_progress<'a>(
    record: &DerivedRecord,
    progress: &'a BTreeMap<String, DerivedTicketProgress>,
    invalid: bool,
) -> Option<std::borrow::Cow<'a, DerivedTicketProgress>> {
    let item = record
        .work_item
        .as_ref()
        .filter(|item| item.status == "accepted")?;
    if invalid || record.scope.is_none() {
        return None;
    }
    progress
        .get(&item.id)
        .map(std::borrow::Cow::Borrowed)
        .or_else(|| {
            (record.kind == Some(Kind::Ticket)).then(|| {
                std::borrow::Cow::Owned(DerivedTicketProgress {
                    status: ProgressStatus::Unknown,
                    last_transition: None,
                    notes: Vec::new(),
                })
            })
        })
}

pub(crate) fn project_binding(
    record: &mut DerivedRecord,
    summary: &Option<RecordSummary>,
    implementation: Option<(&EntrySnapshot, &DerivedRecord)>,
    binding: Option<(&EntrySnapshot, &DerivedRecord)>,
) {
    let implementation_projection = implementation.and_then(|(entry, record)| {
        let Some(RecordSummary::Strategy { adapter, .. }) = &entry.summary else {
            return None;
        };
        record
            .strategy
            .as_ref()
            .map(|strategy| (adapter.as_str(), &strategy.matrix))
    });
    let binding_value = binding
        .and_then(|(_, record)| record.strategy_binding.as_ref())
        .map(|value| &value.binding);
    if let Some(strategy) = &mut record.strategy {
        if let Some(RecordSummary::Strategy { phase, adapter, .. }) = summary {
            strategy.binding = (phase == "implementation").then(|| {
                resolve_binding(
                    phase,
                    adapter,
                    &strategy.matrix,
                    binding_value,
                    binding
                        .is_some_and(|(entry, _)| entry.classification == Classification::Invalid),
                )
            });
        }
    }
    if let Some(binding) = &mut record.strategy_binding {
        binding.state = binding_state(
            &binding.binding,
            implementation.is_some(),
            implementation_projection,
        );
    }
}

pub(super) fn derive(
    scan: &ScanResult,
    retain_display_payload: bool,
    mut parsed_facts: BTreeMap<String, crate::scanning::classification::ParsedFacts>,
) -> DerivedSnapshot {
    let scopes = scan
        .snapshot
        .entries
        .iter()
        .map(|entry| record_scope(&entry.path, scan))
        .collect::<Vec<_>>();
    let strategy_metadata = strategy_metadata_by_scope(
        scan.snapshot.entries.iter().zip(scopes.iter().cloned()),
        &parsed_facts,
    );
    let (progress, invalid_progress_scopes) =
        super::progress::progress_by_scope(scan, &mut parsed_facts);
    let empty = BTreeMap::new();
    let records = scan
        .snapshot
        .entries
        .iter()
        .zip(scopes)
        .map(|(entry, scope)| {
            let metadata = strategy_metadata
                .get(&scope)
                .expect("strategy metadata exists for every record scope");
            let mut record = local_record(
                entry,
                scope.clone(),
                parsed_facts.remove(&entry.path).unwrap_or_default(),
                retain_display_payload,
            );
            record.progress = ticket_progress(
                &record,
                scope
                    .as_ref()
                    .and_then(|scope| progress.get(scope))
                    .unwrap_or(&empty),
                scope
                    .as_ref()
                    .is_some_and(|scope| invalid_progress_scopes.contains(scope)),
            )
            .map(std::borrow::Cow::into_owned);
            if let Some(strategy) = &mut record.strategy {
                if let Some(RecordSummary::Strategy { phase, adapter, .. }) = &entry.summary {
                    strategy.binding = (phase == "implementation").then(|| {
                        resolve_binding(
                            phase,
                            adapter,
                            &strategy.matrix,
                            metadata.binding,
                            metadata.binding_invalid,
                        )
                    });
                }
            }
            if let Some(binding) = &mut record.strategy_binding {
                binding.state = binding_state(
                    &binding.binding,
                    metadata.implementation_selected,
                    metadata
                        .implementation_projection
                        .as_ref()
                        .map(|(adapter, matrix)| (*adapter, matrix)),
                );
            }
            record
        })
        .collect::<Vec<_>>();
    let relationships = derive_relationships(records.iter());
    let boards = super::boards::derive_boards(&records);
    DerivedSnapshot {
        source_revision: scan.snapshot.revision.clone(),
        records,
        relationships,
        boards,
        diagnostics: scan.diagnostics.clone(),
    }
}
