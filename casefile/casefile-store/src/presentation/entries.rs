use super::*;

pub(super) struct PresentationIndexes<'a> {
    pub(super) snapshots: BTreeMap<&'a str, &'a EntrySnapshot>,
    pub(super) records: BTreeMap<&'a str, &'a crate::derived::DerivedRecord>,
    pub(super) diagnostics: BTreeMap<&'a str, Vec<&'a Diagnostic>>,
    pub(super) boards: BTreeMap<&'a crate::derived::ScopedIdentity, Vec<&'a DerivedBoard>>,
}

impl<'a> PresentationIndexes<'a> {
    pub(super) fn new(scan: &'a ScanResult, derived: &'a crate::derived::DerivedSnapshot) -> Self {
        let snapshots = scan
            .snapshot
            .entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        let records = derived
            .records
            .iter()
            .map(|record| (record.path.as_str(), record))
            .collect();
        let mut diagnostics: BTreeMap<&str, Vec<&Diagnostic>> = BTreeMap::new();
        for diagnostic in &scan.diagnostics {
            diagnostics
                .entry(diagnostic.path.as_str())
                .or_default()
                .push(diagnostic);
        }
        let mut boards = BTreeMap::new();
        for board in &derived.boards {
            boards
                .entry(&board.identity)
                .or_insert_with(Vec::new)
                .push(board);
        }
        Self {
            snapshots,
            records,
            diagnostics,
            boards,
        }
    }
}

pub(super) fn presentation_entry(
    descriptor: &Descriptor,
    indexes: &PresentationIndexes<'_>,
) -> PresentationEntry {
    let snapshot = indexes
        .snapshots
        .get(descriptor.path.as_str())
        .copied()
        .expect("descriptor has a presentation snapshot");
    let record = indexes.records.get(descriptor.path.as_str()).copied();
    let title = record.map(|record| record.title.clone());
    let identity = snapshot.identity.clone();
    let summary = snapshot.summary.clone().map(|record| PresentationSummary {
        title: title.unwrap_or_else(|| descriptor.path.clone()),
        record,
    });
    let diagnostics = indexes
        .diagnostics
        .get(descriptor.path.as_str())
        .into_iter()
        .flatten()
        .map(|diagnostic| (*diagnostic).clone())
        .collect();
    let progress = record.and_then(|record| record.progress.clone());
    let boards = record
        .and_then(|record| record.identity.as_ref())
        .map(|identity| {
            indexes
                .boards
                .get(identity)
                .into_iter()
                .flatten()
                .map(|board| (*board).clone())
                .collect()
        })
        .unwrap_or_default();
    let body =
        if descriptor.lazy || descriptor.metadata.public.kind != PresentationFileKind::Regular {
            PresentationFact::Unavailable
        } else {
            PresentationFact::Available(snapshot.original_bytes.clone())
        };
    PresentationEntry {
        path: descriptor.path.clone(),
        scope: descriptor.scope.clone(),
        kind: if descriptor.lazy {
            descriptor.kind
        } else {
            snapshot.kind
        },
        metadata: descriptor.metadata.public.clone(),
        content_handle: descriptor.handle.clone(),
        classification: PresentationFact::Available(snapshot.classification),
        identity: PresentationFact::Available(identity),
        summary: PresentationFact::Available(summary),
        diagnostics: PresentationFact::Available(diagnostics),
        progress: PresentationFact::Available(progress),
        relationships: if record
            .and_then(|record| record.work_item.as_ref())
            .is_some_and(|item| {
                !item.decision_refs.is_empty()
                    || !item.related_tickets.is_empty()
                    || !item.supersedes.is_empty()
                    || !item.superseded_by.is_empty()
            }) {
            PresentationFact::Unavailable
        } else {
            PresentationFact::Available(Vec::new())
        },
        boards: PresentationFact::Available(boards),
        body,
        derived: record.cloned(),
    }
}

pub(super) fn catalogue_entry(descriptor: &Descriptor) -> PresentationEntry {
    let governed = descriptor.kind.is_some();
    PresentationEntry {
        path: descriptor.path.clone(),
        scope: descriptor.scope.clone(),
        kind: descriptor.kind,
        metadata: descriptor.metadata.public.clone(),
        content_handle: descriptor.handle.clone(),
        classification: if governed {
            PresentationFact::Unavailable
        } else {
            PresentationFact::Available(
                if descriptor
                    .scope
                    .as_ref()
                    .is_some_and(|scope| scope.investigation.is_some())
                {
                    Classification::Raw
                } else {
                    Classification::Ungoverned
                },
            )
        },
        identity: PresentationFact::Available(None),
        summary: if governed {
            PresentationFact::Unavailable
        } else {
            PresentationFact::Available(None)
        },
        diagnostics: PresentationFact::Unavailable,
        progress: PresentationFact::Unavailable,
        relationships: PresentationFact::Available(Vec::new()),
        boards: PresentationFact::Unavailable,
        body: PresentationFact::Unavailable,
        derived: None,
    }
}
