use super::*;
use casefile_core::stable;

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

pub(super) fn parsed_file(
    descriptor: &Descriptor,
    bytes: Vec<u8>,
    classified: crate::scanning::classification::Classified,
    retry: bool,
) -> LoadedFile {
    let (classification, kind, identity, summary, diagnostics) = classified.classification;
    let diagnostics = stable(diagnostics);
    let mut parsed = classified.facts;
    let progress = parsed.progress.take().map(|log| {
        let operations = log
            .entries
            .iter()
            .map(|entry| (entry.id().into(), entry.ticket_id().into()))
            .collect();
        Arc::new(ProgressFacts {
            operations,
            folded: fold_progress(log.entries),
        })
    });
    let snapshot = EntrySnapshot {
        path: descriptor.path.clone(),
        classification,
        kind,
        identity,
        summary,
        content_revision: descriptor.metadata.public.revision.clone(),
        original_bytes: bytes,
    };
    let local = Arc::new(local_record(
        &snapshot,
        descriptor
            .scope
            .as_ref()
            .map(|scope| crate::derived::RecordScope {
                project: scope.project.clone(),
                investigation: scope.investigation.clone(),
            }),
        parsed,
        false,
    ));
    let mut entry = catalogue_entry(descriptor);
    entry.kind = snapshot.kind;
    entry.classification = PresentationFact::Available(snapshot.classification);
    entry.identity = PresentationFact::Available(snapshot.identity.clone());
    entry.summary =
        PresentationFact::Available(snapshot.summary.clone().map(|record| PresentationSummary {
            title: local.title.clone(),
            record,
        }));
    entry.diagnostics = PresentationFact::Available(diagnostics.clone());
    entry.progress = PresentationFact::Available(None);
    entry.boards = PresentationFact::Available(Vec::new());
    entry.relationships = if local.work_item.as_ref().is_some_and(|item| {
        !item.decision_refs.is_empty()
            || !item.related_tickets.is_empty()
            || !item.supersedes.is_empty()
            || !item.superseded_by.is_empty()
    }) {
        PresentationFact::Unavailable
    } else {
        PresentationFact::Available(Vec::new())
    };
    let mut snapshot = snapshot;
    entry.body = if descriptor.metadata.public.kind == PresentationFileKind::Regular {
        PresentationFact::Available(std::mem::take(&mut snapshot.original_bytes))
    } else {
        PresentationFact::Unavailable
    };
    entry.derived = Some((*local).clone());
    LoadedFile {
        snapshot,
        entry: Arc::new(entry),
        local,
        diagnostics,
        progress,
        retry,
    }
}
