use crate::{
    activation::{Activation, ActivationState, ScopeIndex, investigation_identity},
    mutation::Overlay,
    revision::{store_revision, synthetic_revision},
    scanning::{
        ScanResult, binding_diagnostics_facts, classification::ParsedFacts, classify_facts,
    },
    validation::{ValidationFacts, cross_validate_facts},
};
use casefile_core::{CasefileSnapshot, EntrySnapshot, stable};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Projection<'a> {
    pub(super) active: &'a Activation,
    pub(super) validation_paths: &'a BTreeSet<String>,
    pub(super) parsed: &'a BTreeMap<String, ParsedFacts>,
    pub(super) progress_operations: &'a BTreeMap<String, Vec<(String, String)>>,
    pub(super) local_diagnostics: &'a BTreeMap<String, Vec<casefile_core::Diagnostic>>,
}

impl Projection<'_> {
    pub(super) fn run<'a>(
        &self,
        files: impl Iterator<Item = (&'a str, Option<&'a EntrySnapshot>)>,
        changes: &Overlay,
        refreshed: &BTreeSet<String>,
        selected: Option<(&str, &casefile_core::SelectedStrategyMatrix)>,
        progress: Option<(&str, &casefile_core::ProgressLog)>,
    ) -> ScanResult {
        let Self {
            active,
            validation_paths,
            parsed,
            local_diagnostics,
            progress_operations,
        } = *self;
        let mut inputs = files.collect::<BTreeMap<_, _>>();
        for path in changes.keys() {
            inputs.entry(path).or_insert(None);
        }
        let scopes = ScopeIndex::new(active);
        let mut validation = ValidationFacts::default();
        let mut diagnostics = Vec::new();
        let mut entries = Vec::with_capacity(inputs.len());
        for (path, input) in inputs {
            let proposed = changes.get(path);
            if proposed.is_some_and(Option::is_none) || (proposed.is_none() && input.is_none()) {
                continue;
            }
            let resolved = scopes.resolve(path);
            let changed_body = proposed
                .and_then(Option::as_deref)
                .is_some_and(|bytes| input.is_none_or(|entry| entry.original_bytes != bytes));
            let classification = if changed_body || refreshed.contains(path) {
                let bytes = proposed
                    .and_then(Option::as_deref)
                    .unwrap_or_else(|| &input.unwrap().original_bytes);
                Some(
                    if progress
                        .is_some_and(|(progress_path, _)| progress_path == path && changed_body)
                    {
                        crate::scanning::classification::Classified {
                            classification: (
                                casefile_core::Classification::Governed,
                                Some(casefile_core::Kind::Progress),
                                None,
                                Some(casefile_core::RecordSummary::Progress),
                                Vec::new(),
                            ),
                            facts: ParsedFacts::default(),
                        }
                    } else {
                        match selected.filter(|(selected_path, _)| {
                            *selected_path == path && proposed.is_some()
                        }) {
                            Some((_, selected)) => crate::scanning::classification::Classified {
                                classification: (
                                    casefile_core::Classification::Governed,
                                    Some(casefile_core::Kind::Strategy),
                                    None,
                                    Some(casefile_core::RecordSummary::Strategy {
                                        strategy_id: selected.strategy_id.clone(),
                                        phase: selected.phase.clone(),
                                        adapter: selected.adapter.clone(),
                                    }),
                                    Vec::new(),
                                ),
                                facts: ParsedFacts {
                                    strategy: Some(selected.projection.clone()),
                                    ..ParsedFacts::default()
                                },
                            },
                            None => classify_facts(path, bytes, active, resolved.kind),
                        }
                    },
                )
            } else {
                None
            };
            let (class, kind, identity, summary) = match &classification {
                Some(classified) => {
                    diagnostics.extend(classified.classification.4.iter().cloned());
                    (
                        classified.classification.0,
                        classified.classification.1,
                        classified.classification.2.clone(),
                        classified.classification.3.clone(),
                    )
                }
                None => {
                    let entry = input.unwrap();
                    diagnostics.extend(local_diagnostics.get(path).into_iter().flatten().cloned());
                    (
                        entry.classification,
                        entry.kind,
                        entry.identity.clone(),
                        entry.summary.clone(),
                    )
                }
            };
            let entry = EntrySnapshot {
                path: path.into(),
                classification: class,
                kind,
                identity,
                content_revision: if proposed.is_some() {
                    synthetic_revision(path, true)
                } else {
                    input.unwrap().content_revision.clone()
                },
                summary,
                original_bytes: Vec::new(),
            };
            if let Some((_, log)) =
                progress.filter(|(progress_path, _)| *progress_path == path && changed_body)
            {
                validation.insert_progress(&entry, resolved, log);
            } else if let Some(classified) = &classification {
                validation.insert(&entry, resolved, &classified.facts);
            } else if let Some(facts) = parsed.get(path) {
                validation.insert(&entry, resolved, facts);
                if let Some(operations) = progress_operations.get(path) {
                    validation.insert_progress_operations(path, operations);
                }
            }
            entries.push(entry);
        }
        diagnostics.extend(
            cross_validate_facts(&entries, active, &validation, |_| true)
                .into_iter()
                .filter(|diagnostic| {
                    validation_paths.contains(&diagnostic.path)
                        || diagnostic.code == "duplicate_identity"
                }),
        );
        diagnostics.extend(binding_diagnostics_facts(&entries, &validation));
        // This internal projection token never leaves the mutation context as a Store revision.
        let revision = store_revision(
            entries
                .iter()
                .map(|e| (e.path.as_str(), &e.content_revision)),
            true,
        );
        ScanResult {
            activation: ActivationState::Active,
            investigation_roots: active
                .projects
                .iter()
                .map(|(slug, p)| {
                    (
                        slug.clone(),
                        p.investigations
                            .iter()
                            .filter_map(|i| investigation_identity(slug, i).map(str::to_owned))
                            .collect(),
                    )
                })
                .collect(),
            snapshot: CasefileSnapshot { revision, entries },
            diagnostics: stable(diagnostics),
        }
    }
}
