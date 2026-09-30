use crate::scanning::ScanResult;
use casefile_core::{
    BoardDraft, BoardStatusSource, Classification, Diagnostic, EntrySnapshot, Kind, ProgressEntry,
    ProgressNoteCategory, ProgressStatus, RecordDraft, RecordSummary, Revision, StrategyBinding,
    StrategyProjection, WorkItemDraft, parse_progress_log, parse_strategy_projection,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
    pub work_item: Option<DerivedWorkItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<DerivedTicketProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardDraft>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<DerivedStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_binding: Option<DerivedStrategyBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedWorkItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub rank: Option<u64>,
    pub decision_refs: Vec<String>,
    pub related_tickets: Vec<String>,
    pub supersedes: Vec<String>,
    pub superseded_by: Vec<String>,
}

impl From<WorkItemDraft> for DerivedWorkItem {
    fn from(item: WorkItemDraft) -> Self {
        Self {
            id: item.id,
            title: item.title,
            status: item.status,
            rank: item.rank,
            decision_refs: item.decision_refs,
            related_tickets: item.related_tickets,
            supersedes: item.supersedes,
            superseded_by: item.superseded_by,
        }
    }
}

impl DerivedRecord {
    pub fn rendered_markdown(&self) -> Option<String> {
        self.content
            .as_deref()
            .filter(|_| self.path.ends_with(".md"))
            .map(casefile_core::render_markdown_html)
    }

    pub fn search_text(&self) -> String {
        format!(
            "{}\n{}",
            self.title,
            self.content.as_deref().unwrap_or_default()
        )
    }
}

mod boards;
mod progress;
mod records;
pub(super) use boards::{derive_boards, scoped_boards};
pub(super) use progress::fold_progress;
pub(super) use records::{local_record, project_binding, ticket_progress};

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
    #[serde(default)]
    pub status_source: BoardStatusSource,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedTicketProgress {
    pub status: ProgressStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition: Option<DerivedProgressTransition>,
    pub notes: Vec<DerivedProgressNote>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedProgressTransition {
    pub id: String,
    pub recorded_at: String,
    pub recorded_by: String,
    pub from: ProgressStatus,
    pub to: ProgressStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DerivedProgressNote {
    pub id: String,
    pub recorded_at: String,
    pub recorded_by: String,
    pub category: ProgressNoteCategory,
    pub message: String,
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
    facts: &BTreeMap<String, crate::scanning::classification::ParsedFacts>,
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
                facts
                    .get(&entry.path)?
                    .strategy
                    .clone()
                    .map(|matrix| (adapter.as_str(), matrix))
            })();
        }
    }
    metadata_by_scope
}

pub(super) fn derive_snapshot(scan: &ScanResult) -> DerivedSnapshot {
    records::derive(scan, true, fallback_facts(scan))
}

pub(super) fn derive_snapshot_from_facts(
    scan: &ScanResult,
    facts: BTreeMap<String, crate::scanning::classification::ParsedFacts>,
) -> DerivedSnapshot {
    records::derive(scan, true, facts)
}

fn fallback_facts(
    scan: &ScanResult,
) -> BTreeMap<String, crate::scanning::classification::ParsedFacts> {
    scan.snapshot
        .entries
        .iter()
        .filter(|entry| entry.classification == Classification::Governed)
        .map(|entry| {
            let mut facts = crate::scanning::classification::ParsedFacts::default();
            let source = std::str::from_utf8(&entry.original_bytes).ok();
            match (entry.kind, source) {
                (Some(kind), Some(text)) if kind.is_writable() => {
                    facts.draft = casefile_core::parse_draft(&entry.path, kind, text).ok()
                }
                (Some(Kind::Progress), Some(text)) => {
                    facts.progress = parse_progress_log(&entry.path, text).ok()
                }
                (Some(Kind::Strategy), Some(text)) => {
                    facts.strategy = parse_strategy_projection(&entry.path, text).ok().flatten()
                }
                _ => {}
            }
            (entry.path.clone(), facts)
        })
        .collect()
}

fn summary_title(summary: &RecordSummary) -> String {
    match summary {
        RecordSummary::Markdown { title }
        | RecordSummary::WorkItem { title, .. }
        | RecordSummary::Board { title, .. } => title.clone(),
        RecordSummary::Strategy { strategy_id, .. } => strategy_id.clone(),
        RecordSummary::StrategyBinding { binding } => format!("{} writer binding", binding.adapter),
        RecordSummary::StrategyTransition { record } => {
            format!("{} strategy transition", record.selected_strategy_id)
        }
        RecordSummary::Activation { .. } => "Casefile activation".into(),
        RecordSummary::ProjectMap { .. } => "Project map".into(),
        RecordSummary::Progress => "Ticket progress".into(),
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

fn record_scope(path: &str, scan: &ScanResult) -> Option<RecordScope> {
    let (project, investigation) = scan.scope_for_path(path)?;
    Some(RecordScope {
        project: project.into(),
        investigation: investigation.map(Into::into),
    })
}

/// Resolves references from already-derived records using project-wide target uniqueness.
pub fn derive_relationships<'a>(
    records: impl IntoIterator<Item = &'a DerivedRecord>,
) -> Vec<DerivedRelationship> {
    let records = records.into_iter().collect::<Vec<_>>();
    let mut decisions: BTreeMap<(&str, &str), Vec<&ScopedIdentity>> = BTreeMap::new();
    let mut work_items: BTreeMap<(&str, &str), Vec<&ScopedIdentity>> = BTreeMap::new();
    for record in &records {
        let Some(identity) = record.identity.as_ref() else {
            continue;
        };
        let key = (identity.scope.project.as_str(), identity.identity.as_str());
        if record.kind == Some(Kind::Decision) {
            decisions.entry(key).or_default().push(identity);
        } else if matches!(record.kind, Some(Kind::Ticket | Kind::Epic)) {
            work_items.entry(key).or_default().push(identity);
        }
    }
    let mut result = Vec::new();
    for record in &records {
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
                let key = (source.scope.project.as_str(), reference.as_str());
                let targets = if kind == RelationshipKind::Decision {
                    decisions.get(&key)
                } else {
                    work_items.get(&key)
                };
                let targets = targets.map(Vec::as_slice).unwrap_or_default();
                if targets.len() == 1 {
                    result.push(DerivedRelationship {
                        source: source.clone(),
                        target: (*targets[0]).clone(),
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
