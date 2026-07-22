use crate::{
    activation::{Activation, investigation_identity, project_for, scope_for},
    scanning::ScanResult,
};
use casefile_core::{
    BoardDraft, Classification, Diagnostic, Kind, RecordDraft, RecordSummary, Revision,
    WorkItemDraft,
};
use serde::{Deserialize, Serialize};

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

pub(super) fn derive_snapshot(scan: &ScanResult, active: &Activation) -> DerivedSnapshot {
    let records = scan
        .snapshot
        .entries
        .iter()
        .map(|entry| {
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
            let scope = record_scope(&entry.path, active);
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
        RecordSummary::Activation { .. } => "Casefile activation".into(),
        RecordSummary::ProjectMap { .. } => "Project map".into(),
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
