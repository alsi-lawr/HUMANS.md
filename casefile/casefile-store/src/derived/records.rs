use super::*;

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
    let records = scan
        .snapshot
        .entries
        .iter()
        .zip(scopes)
        .map(|(entry, scope)| {
            let source = std::str::from_utf8(&entry.original_bytes).ok();
            let content = retain_display_payload
                .then(|| source.map(str::to_owned))
                .flatten();
            let title = entry.summary.as_ref().map_or_else(
                || entry.identity.clone().unwrap_or_else(|| entry.path.clone()),
                summary_title,
            );
            let parsed = parsed_facts.remove(&entry.path).unwrap_or_default();
            let draft = parsed.draft;
            let (work_item, board) = match draft {
                Some(RecordDraft::Ticket(item) | RecordDraft::Epic(item)) => {
                    (Some(DerivedWorkItem::from(item)), None)
                }
                Some(RecordDraft::Board(board)) => (None, Some(board)),
                None => (None, None),
            };
            let progress_identity = identity_for_progress(&entry.path, &work_item, scan);
            let ticket_progress = progress_identity
                .as_ref()
                .and_then(|(scope, ticket)| {
                    (!invalid_progress_scopes.contains(scope)).then_some((scope, ticket))
                })
                .and_then(|(scope, ticket)| {
                    progress.get(scope).and_then(|values| values.get(*ticket))
                })
                .cloned()
                .or_else(|| {
                    progress_identity
                        .as_ref()
                        .filter(|(scope, _)| !invalid_progress_scopes.contains(scope))
                        .and_then(|_| {
                            work_item.as_ref().filter(|item| {
                                item.status == "accepted" && entry.kind == Some(Kind::Ticket)
                            })
                        })
                        .map(|_| DerivedTicketProgress {
                            status: ProgressStatus::Unknown,
                            last_transition: None,
                            notes: Vec::new(),
                        })
                });
            let metadata = strategy_metadata
                .get(&scope)
                .expect("strategy metadata exists for every record scope");
            let strategy = match (&entry.summary, source) {
                (Some(RecordSummary::Strategy { phase, adapter, .. }), Some(_)) => {
                    parsed.strategy.map(|matrix| DerivedStrategy {
                        binding: (phase == "implementation").then(|| {
                            resolve_binding(
                                phase,
                                adapter,
                                &matrix,
                                metadata.binding,
                                metadata.binding_invalid,
                            )
                        }),
                        matrix,
                    })
                }
                _ => None,
            };
            let strategy_binding = match &entry.summary {
                Some(RecordSummary::StrategyBinding { binding }) => Some(DerivedStrategyBinding {
                    binding: binding.clone(),
                    state: binding_state(
                        binding,
                        metadata.implementation_selected,
                        metadata
                            .implementation_projection
                            .as_ref()
                            .map(|(adapter, matrix)| (*adapter, matrix)),
                    ),
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
                content,
                work_item,
                progress: ticket_progress,
                board,
                strategy,
                strategy_binding,
            }
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
