use crate::{
    activation::{Activation, investigation_identity, project_for, scope_for},
    scanning::ScanResult,
};
use casefile_core::{
    BoardDraft, Classification, Diagnostic, EntrySnapshot, Kind, RecordDraft, RecordSummary,
    Revision, StrategyBinding, StrategyProjection, WorkItemDraft, parse_strategy_projection,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct RecordScope {
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investigation: Option<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ScopedIdentity {
    pub scope: RecordScope,
    pub identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedSnapshot {
    pub source_revision: Revision,
    pub records: Vec<DerivedRecord>,
    pub relationships: Vec<DerivedRelationship>,
    pub boards: Vec<DerivedBoard>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedRecord {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<RecordScope>,
    pub classification: Classification,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<ScopedIdentity>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_markdown: Option<String>,
    pub search_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemDraft>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardDraft>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<DerivedStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_binding: Option<DerivedStrategyBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedStrategy {
    #[serde(flatten)]
    pub matrix: StrategyProjection,
    pub binding: Option<StrategyBindingState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedStrategyBinding {
    #[serde(flatten)]
    pub binding: StrategyBinding,
    pub state: StrategyBindingState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StrategyBindingState {
    Absent { effective: EffectiveWriterBinding },
    Pending,
    Resolved { effective: EffectiveWriterBinding },
    Unresolved,
    Invalid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectiveWriterBinding {
    pub model: String,
    pub reasoning_effort: String,
    pub source: WriterBindingSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriterBindingSource {
    Matrix,
    Binding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Decision,
    Related,
    Supersedes,
    SupersededBy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedRelationship {
    pub source: ScopedIdentity,
    pub target: ScopedIdentity,
    pub kind: RelationshipKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedBoard {
    pub identity: ScopedIdentity,
    pub title: String,
    pub filter_statuses: Option<Vec<String>>,
    pub filter_kinds: Option<Vec<String>>,
    pub columns: Vec<DerivedBoardColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedBoardColumn {
    pub name: String,
    pub statuses: Vec<String>,
    pub cards: Vec<DerivedCard>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedCard {
    pub identity: ScopedIdentity,
    pub kind: Kind,
    pub title: String,
    pub status: String,
    pub rank: Option<u64>,
}

#[derive(Default)]
struct StrategyMetadata<'a> {
    binding_selected: bool,
    binding: Option<&'a StrategyBinding>,
    binding_invalid: bool,
    implementation_selected: bool,
    implementation_projection: Option<(&'a str, StrategyProjection)>,
}

fn strategy_metadata_by_scope<'a>(
    entries: impl IntoIterator<Item = (&'a EntrySnapshot, Option<RecordScope>)>,
) -> BTreeMap<Option<RecordScope>, StrategyMetadata<'a>> {
    let mut metadata_by_scope: BTreeMap<Option<RecordScope>, StrategyMetadata<'a>> =
        BTreeMap::new();
    for (entry, scope) in entries {
        let metadata = metadata_by_scope.entry(scope).or_default();
        if !metadata.binding_selected && entry.kind == Some(Kind::StrategyBinding) {
            metadata.binding_selected = true;
            metadata.binding = match &entry.summary {
                Some(RecordSummary::StrategyBinding { binding }) => Some(binding),
                _ => None,
            };
            metadata.binding_invalid = entry.classification == Classification::Invalid;
        }
        if !metadata.implementation_selected
            && entry.classification == Classification::Governed
            && matches!(&entry.summary, Some(RecordSummary::Strategy { phase, .. }) if phase == "implementation")
        {
            metadata.implementation_selected = true;
            metadata.implementation_projection = (|| {
                let Some(RecordSummary::Strategy { adapter, .. }) = &entry.summary else {
                    return None;
                };
                let text = std::str::from_utf8(&entry.original_bytes).ok()?;
                parse_strategy_projection(&entry.path, text)
                    .ok()
                    .flatten()
                    .map(|matrix| (adapter.as_str(), matrix))
            })();
        }
    }
    metadata_by_scope
}

pub(super) fn derive_snapshot(scan: &ScanResult, active: &Activation) -> DerivedSnapshot {
    let scopes = scan
        .snapshot
        .entries
        .iter()
        .map(|entry| record_scope(&entry.path, active))
        .collect::<Vec<_>>();
    let strategy_metadata =
        strategy_metadata_by_scope(scan.snapshot.entries.iter().zip(scopes.iter().cloned()));
    let records = scan
        .snapshot
        .entries
        .iter()
        .zip(scopes)
        .map(|(entry, scope)| {
            let content = String::from_utf8(entry.original_bytes.clone()).ok();
            let rendered_markdown = content
                .as_deref()
                .filter(|_| entry.path.ends_with(".md"))
                .map(casefile_core::render_markdown_html);
            let title = entry.summary.as_ref().map_or_else(
                || entry.identity.clone().unwrap_or_else(|| entry.path.clone()),
                summary_title,
            );
            let draft = content.as_deref().and_then(|text| match entry.kind {
                Some(kind)
                    if kind.is_writable() && entry.classification == Classification::Governed =>
                {
                    casefile_core::parse_draft(
                        &entry.path,
                        entry.kind.expect("work item kind"),
                        text,
                    )
                    .ok()
                }
                _ => None,
            });
            let (work_item, board) = match draft {
                Some(RecordDraft::Ticket(item) | RecordDraft::Epic(item)) => (Some(item), None),
                Some(RecordDraft::Board(board)) => (None, Some(board)),
                None => (None, None),
            };
            let metadata = strategy_metadata
                .get(&scope)
                .expect("strategy metadata exists for every record scope");
            let strategy = match (&entry.summary, content.as_deref()) {
                (Some(RecordSummary::Strategy { phase, adapter, .. }), Some(text)) => {
                    parse_strategy_projection(&entry.path, text)
                        .ok()
                        .flatten()
                        .map(|matrix| DerivedStrategy {
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
                title: title.clone(),
                search_text: format!("{title}\n{}", content.as_deref().unwrap_or_default()),
                content,
                rendered_markdown,
                work_item,
                board,
                strategy,
                strategy_binding,
            }
        })
        .collect::<Vec<_>>();
    let relationships = derive_relationships(&records);
    let boards = derive_boards(&records, active);
    DerivedSnapshot {
        source_revision: scan.snapshot.revision.clone(),
        records,
        relationships,
        boards,
        diagnostics: scan.diagnostics.clone(),
    }
}

fn summary_title(summary: &RecordSummary) -> String {
    match summary {
        RecordSummary::Markdown { title }
        | RecordSummary::WorkItem { title, .. }
        | RecordSummary::Board { title, .. } => title.clone(),
        RecordSummary::Strategy { strategy_id, .. } => strategy_id.clone(),
        RecordSummary::StrategyBinding { binding } => format!("{} writer binding", binding.adapter),
        RecordSummary::Activation { .. } => "Casefile activation".into(),
        RecordSummary::ProjectMap { .. } => "Project map".into(),
    }
}

fn resolve_binding(
    phase: &str,
    adapter: &str,
    matrix: &StrategyProjection,
    binding: Option<&StrategyBinding>,
    binding_invalid: bool,
) -> StrategyBindingState {
    if phase != "implementation" {
        return StrategyBindingState::Pending;
    }
    if binding_invalid {
        return StrategyBindingState::Invalid;
    }
    match binding {
        Some(binding) => binding_state(binding, true, Some((adapter, matrix))),
        None => matrix_default(matrix),
    }
}

fn matrix_default(matrix: &StrategyProjection) -> StrategyBindingState {
    let writers = matrix
        .workers
        .iter()
        .filter(|worker| worker.role == "implementation-writer")
        .collect::<Vec<_>>();
    if writers.len() != 1 {
        return StrategyBindingState::Unresolved;
    }
    let writer = writers[0];
    match (&writer.model, &writer.reasoning_effort) {
        (Some(model), Some(reasoning_effort)) => StrategyBindingState::Absent {
            effective: EffectiveWriterBinding {
                model: model.clone(),
                reasoning_effort: reasoning_effort.clone(),
                source: WriterBindingSource::Matrix,
            },
        },
        _ => StrategyBindingState::Unresolved,
    }
}

fn binding_state(
    binding: &StrategyBinding,
    implementation_selected: bool,
    implementation: Option<(&str, &StrategyProjection)>,
) -> StrategyBindingState {
    if !implementation_selected {
        return StrategyBindingState::Pending;
    }
    let Some((adapter, matrix)) = implementation else {
        return StrategyBindingState::Unresolved;
    };
    let writers = matrix
        .workers
        .iter()
        .filter(|worker| worker.role == "implementation-writer")
        .collect::<Vec<_>>();
    if binding.adapter != adapter || writers.len() != 1 {
        return StrategyBindingState::Unresolved;
    }
    StrategyBindingState::Resolved {
        effective: EffectiveWriterBinding {
            model: binding.model.clone(),
            reasoning_effort: binding.reasoning_effort.clone(),
            source: WriterBindingSource::Binding,
        },
    }
}

fn record_scope(path: &str, active: &Activation) -> Option<RecordScope> {
    let project = project_for(path, active)?;
    Some(RecordScope {
        project: project.into(),
        investigation: scope_for(path, active)
            .and_then(|value| investigation_identity(project, value))
            .map(Into::into),
    })
}

fn derive_relationships(records: &[DerivedRecord]) -> Vec<DerivedRelationship> {
    let mut result = Vec::new();
    for record in records {
        let (Some(source), Some(item)) = (&record.identity, &record.work_item) else {
            continue;
        };
        for (references, kind) in [
            (&item.decision_refs, RelationshipKind::Decision),
            (&item.related_tickets, RelationshipKind::Related),
            (&item.supersedes, RelationshipKind::Supersedes),
            (&item.superseded_by, RelationshipKind::SupersededBy),
        ] {
            for reference in references {
                let targets = records
                    .iter()
                    .filter(|target| {
                        target.identity.as_ref().is_some_and(|identity| {
                            identity.identity == *reference
                                && match kind {
                                    RelationshipKind::Decision => {
                                        target.kind == Some(Kind::Decision)
                                            && identity.scope.project == source.scope.project
                                            && (identity.scope.investigation.is_none()
                                                || identity.scope.investigation
                                                    == source.scope.investigation)
                                    }
                                    _ => {
                                        matches!(target.kind, Some(Kind::Ticket | Kind::Epic))
                                            && identity.scope == source.scope
                                    }
                                }
                        })
                    })
                    .filter_map(|target| target.identity.clone())
                    .collect::<Vec<_>>();
                if targets.len() == 1 {
                    result.push(DerivedRelationship {
                        source: source.clone(),
                        target: targets[0].clone(),
                        kind,
                    });
                }
            }
        }
    }
    result.sort_by(|left, right| {
        (&left.source, left.kind as u8, &left.target).cmp(&(
            &right.source,
            right.kind as u8,
            &right.target,
        ))
    });
    result.dedup();
    result
}

fn derive_boards(records: &[DerivedRecord], active: &Activation) -> Vec<DerivedBoard> {
    let mut boards = Vec::new();
    for record in records.iter().filter(|record| {
        record.kind == Some(Kind::Board) && record.classification == Classification::Governed
    }) {
        let Some(text) = record.content.as_deref() else {
            continue;
        };
        let Ok(RecordDraft::Board(board)) =
            casefile_core::parse_draft(&record.path, Kind::Board, text)
        else {
            continue;
        };
        let Some(scope) = record_scope(&record.path, active) else {
            continue;
        };
        let identity = ScopedIdentity {
            scope,
            identity: board.id.clone(),
        };
        let mut columns = board
            .columns
            .into_iter()
            .map(|column| DerivedBoardColumn {
                name: column.name,
                statuses: column.statuses,
                cards: Vec::new(),
            })
            .collect::<Vec<_>>();
        for candidate in records.iter().filter(|candidate| {
            candidate
                .identity
                .as_ref()
                .is_some_and(|item| item.scope == identity.scope)
        }) {
            let (Some(card_id), Some(item), Some(kind)) =
                (&candidate.identity, &candidate.work_item, candidate.kind)
            else {
                continue;
            };
            if board
                .filter_statuses
                .as_ref()
                .is_some_and(|values| !values.contains(&item.status))
                || board.filter_kinds.as_ref().is_some_and(|values| {
                    !values.iter().any(|value| {
                        value
                            == if kind == Kind::Ticket {
                                "ticket"
                            } else {
                                "epic"
                            }
                    })
                })
            {
                continue;
            }
            if let Some(column) = columns
                .iter_mut()
                .find(|column| column.statuses.contains(&item.status))
            {
                column.cards.push(DerivedCard {
                    identity: card_id.clone(),
                    kind,
                    title: item.title.clone(),
                    status: item.status.clone(),
                    rank: item.rank,
                });
            }
        }
        for column in &mut columns {
            column.cards.sort_by(|left, right| {
                (left.rank.unwrap_or(u64::MAX), &left.identity.identity)
                    .cmp(&(right.rank.unwrap_or(u64::MAX), &right.identity.identity))
            });
        }
        boards.push(DerivedBoard {
            identity,
            title: board.title,
            filter_statuses: board.filter_statuses,
            filter_kinds: board.filter_kinds,
            columns,
        });
    }
    boards.sort_by(|left, right| left.identity.cmp(&right.identity));
    boards
}
