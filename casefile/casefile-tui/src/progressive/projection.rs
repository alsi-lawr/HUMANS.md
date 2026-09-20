use super::*;

pub(super) fn build_projection(
    catalogue: Option<&PresentationCatalogue>,
    entries: &[&PresentationEntry],
    provisional: bool,
    relationships: &RelationshipState,
) -> UiProjection {
    let activation = catalogue
        .map(|catalogue| catalogue.activation)
        .unwrap_or(ActivationState::Unactivated);
    let investigation_roots = catalogue
        .map(|catalogue| {
            catalogue
                .projects
                .iter()
                .map(|project| {
                    (project.slug.clone(), {
                        let mut roots = project
                            .investigations
                            .iter()
                            .map(|investigation| investigation.identity.clone())
                            .collect::<Vec<_>>();
                        roots.sort();
                        roots.dedup();
                        roots
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut diagnostics = catalogue
        .map(|catalogue| catalogue.diagnostics.clone())
        .unwrap_or_default();
    for entry in entries {
        if let PresentationFact::Available(values) = &entry.diagnostics {
            diagnostics.extend(values.clone());
        }
    }
    diagnostics.sort_by(|left, right| {
        (&left.path, &left.code, &left.message).cmp(&(&right.path, &right.code, &right.message))
    });
    diagnostics.dedup();
    let snapshots = entries
        .iter()
        .map(|entry| snapshot_entry(entry))
        .collect::<Vec<_>>();
    let revision = Revision("presentation".into());
    let scan = ScanResult {
        activation,
        investigation_roots,
        snapshot: CasefileSnapshot {
            revision: revision.clone(),
            entries: snapshots,
        },
        diagnostics: diagnostics.clone(),
    };
    let derived = presentation_derived(&revision, entries, &diagnostics);
    let unavailable = entries
        .iter()
        .filter_map(|entry| {
            unavailable_fields(entry, relationships).map(|fields| (entry.path.clone(), fields))
        })
        .collect();
    UiProjection {
        relationship_updates: BTreeMap::new(),
        availability_changed: Vec::new(),
        removed: Vec::new(),
        incremental: false,
        scan,
        derived,
        provisional,
        unavailable,
    }
}

pub(super) fn unavailable_fields(
    entry: &PresentationEntry,
    relationships: &RelationshipState,
) -> Option<String> {
    let mut fields = Vec::new();
    if matches!(entry.classification, PresentationFact::Unavailable) {
        fields.push("classification");
    }
    if matches!(entry.summary, PresentationFact::Unavailable) {
        fields.push("summary");
    }
    if matches!(entry.diagnostics, PresentationFact::Unavailable) {
        fields.push("diagnostics");
    }
    if matches!(entry.progress, PresentationFact::Unavailable) {
        fields.push("progress");
    }
    if !relationships.is_available(entry) {
        fields.push("relationships");
    }
    if matches!(entry.boards, PresentationFact::Unavailable) {
        fields.push("boards");
    }
    if matches!(entry.body, PresentationFact::Unavailable) {
        fields.push("content");
    }
    (!fields.is_empty()).then(|| fields.join(", "))
}

fn snapshot_entry(entry: &PresentationEntry) -> EntrySnapshot {
    EntrySnapshot {
        path: entry.path.clone(),
        classification: match entry.classification {
            PresentationFact::Available(value) => value,
            PresentationFact::Unavailable => Classification::Raw,
        },
        kind: entry.kind,
        identity: match &entry.identity {
            PresentationFact::Available(value) => value.clone(),
            PresentationFact::Unavailable => None,
        },
        content_revision: entry.metadata.revision.clone(),
        summary: match &entry.summary {
            PresentationFact::Available(Some(summary)) => Some(summary.record.clone()),
            PresentationFact::Available(None) | PresentationFact::Unavailable => None,
        },
        original_bytes: match &entry.body {
            PresentationFact::Available(bytes) => bytes.clone(),
            PresentationFact::Unavailable => Vec::new(),
        },
    }
}

fn presentation_derived(
    revision: &Revision,
    entries: &[&PresentationEntry],
    diagnostics: &[casefile_core::Diagnostic],
) -> DerivedSnapshot {
    let records = entries
        .iter()
        .filter_map(|entry| {
            let mut record = entry.derived.clone()?;
            record.content = match &entry.body {
                PresentationFact::Available(bytes) => String::from_utf8(bytes.clone()).ok(),
                PresentationFact::Unavailable => None,
            };
            Some(record)
        })
        .collect();
    let mut boards = Vec::new();
    for entry in entries {
        if let PresentationFact::Available(values) = &entry.boards {
            boards.extend(values.iter().cloned());
        }
    }
    DerivedSnapshot {
        source_revision: revision.clone(),
        records,
        relationships: Vec::new(),
        boards,
        diagnostics: diagnostics.to_vec(),
    }
}
