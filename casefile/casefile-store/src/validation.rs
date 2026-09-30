use crate::{
    activation::{Activation, PathFacts, ScopeIndex},
    layout::safe_relative,
};
use casefile_core::{Classification, Diagnostic, EntrySnapshot, Kind, RecordDraft, RecordSummary};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct ValidationFacts<'a> {
    pub(super) resolved: BTreeMap<String, PathFacts<'a>>,
    pub(super) strategies: BTreeMap<String, Option<casefile_core::StrategyProjection>>,
    work: BTreeMap<String, WorkReferences>,
    metadata: BTreeMap<String, (Vec<String>, Vec<String>)>,
    progress: BTreeMap<String, Vec<(String, String)>>,
}

struct WorkReferences {
    decision_refs: Vec<String>,
    related_tickets: Vec<String>,
    supersedes: Vec<String>,
    superseded_by: Vec<String>,
}

impl<'a> ValidationFacts<'a> {
    pub(super) fn attachment_targets(&self) -> impl Iterator<Item = String> + '_ {
        self.metadata.iter().flat_map(|(path, (_, attachments))| {
            attachments
                .iter()
                .filter_map(move |attachment| attachment_target(path, attachment))
        })
    }

    pub(super) fn insert(
        &mut self,
        entry: &EntrySnapshot,
        resolved: PathFacts<'a>,
        parsed: crate::scanning::classification::ParsedFacts,
    ) {
        self.resolved.insert(entry.path.clone(), resolved);
        if let Some(RecordDraft::Ticket(item) | RecordDraft::Epic(item)) = parsed.draft {
            self.work.insert(
                entry.path.clone(),
                WorkReferences {
                    decision_refs: item.decision_refs,
                    related_tickets: item.related_tickets,
                    supersedes: item.supersedes,
                    superseded_by: item.superseded_by,
                },
            );
        }
        if let Some(metadata) = parsed.metadata {
            self.metadata.insert(entry.path.clone(), metadata);
        }
        if let Some(log) = parsed.progress {
            self.progress.insert(
                entry.path.clone(),
                log.entries
                    .iter()
                    .map(|entry| (entry.id().into(), entry.ticket_id().into()))
                    .collect(),
            );
        }
        if entry.kind == Some(Kind::Strategy) {
            self.strategies.insert(entry.path.clone(), parsed.strategy);
        }
    }
}

pub(super) fn cross_validate(entries: &[EntrySnapshot], active: &Activation) -> Vec<Diagnostic> {
    let mut facts = ValidationFacts::default();
    let scopes = ScopeIndex::new(active);
    for entry in entries {
        let resolved = scopes.resolve(&entry.path);
        let parsed = crate::scanning::classify_facts(
            &entry.path,
            &entry.original_bytes,
            active,
            resolved.kind,
        )
        .facts;
        facts.insert(entry, resolved, parsed);
    }
    cross_validate_facts(entries, active, &facts)
}

pub(super) fn cross_validate_facts(
    entries: &[EntrySnapshot],
    active: &Activation,
    facts: &ValidationFacts,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut identities: BTreeMap<&str, &EntrySnapshot> = BTreeMap::new();
    let paths: BTreeSet<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
    let mut supersedes = BTreeMap::new();
    let scopes = &facts.resolved;
    for entry in entries
        .iter()
        .filter(|entry| entry.classification == Classification::Governed)
    {
        if let Some(identity) = entry.identity.as_deref() {
            if let Some(previous) = identities.insert(identity, entry) {
                diagnostics.push(Diagnostic::new(
                    &entry.path,
                    "duplicate_identity",
                    format!("identity also appears at {}", previous.path),
                ));
            }
        }
        if entry.kind.is_some_and(Kind::is_writable) {
            if let Some(project) = facts.resolved[&entry.path]
                .project
                .and_then(|project| active.projects.get(project))
            {
                if !entry.identity.as_deref().is_some_and(|id| {
                    id.strip_prefix(&project.prefix)
                        .is_some_and(|rest| rest.starts_with('-'))
                }) {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "project_prefix",
                        "record identity must use the configured project prefix",
                    ));
                }
            }
        }
    }
    diagnostics.extend(progress_diagnostics_facts(entries, active, facts));
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.summary, Some(RecordSummary::WorkItem { .. })))
    {
        let RecordSummary::WorkItem { id, .. } =
            entry.summary.as_ref().expect("filtered work item")
        else {
            unreachable!()
        };
        if let Some(item) = facts.work.get(&entry.path) {
            let project = scopes[entry.path.as_str()].project;
            for reference in &item.decision_refs {
                let resolves = identities.get(reference.as_str()).is_some_and(|target| {
                    target.kind == Some(Kind::Decision)
                        && scopes[target.path.as_str()].project == project
                });
                if reference == id || !resolves {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "unresolved_reference",
                        "decision references must resolve within the governed project",
                    ));
                }
            }
            for reference in item
                .related_tickets
                .iter()
                .chain(item.supersedes.iter())
                .chain(item.superseded_by.iter())
            {
                if reference == id
                    || identities.get(reference.as_str()).is_none_or(|target| {
                        !matches!(target.kind, Some(Kind::Ticket | Kind::Epic))
                            || scopes[target.path.as_str()].project != project
                    })
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "unresolved_reference",
                        "references must resolve to work items within the governed project",
                    ));
                }
            }
            supersedes.insert(id.clone(), item.supersedes.clone());
        }
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, Some(Kind::Evidence | Kind::Review)))
    {
        if let Some((refs, attachments)) = facts.metadata.get(&entry.path) {
            let scope = scopes[entry.path.as_str()].scope;
            for reference in refs {
                if identities
                    .get(reference.as_str())
                    .is_none_or(|target| scopes[target.path.as_str()].scope != scope)
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "unresolved_reference",
                        "references must resolve within the governed project/investigation scope",
                    ));
                }
            }
            for attachment in attachments {
                let target = attachment_target(&entry.path, attachment);
                if !target
                    .as_deref()
                    .is_some_and(|path| safe_relative(path) && paths.contains(path))
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "missing_attachment",
                        "attachments must be contained regular files",
                    ));
                }
            }
        }
    }
    for start in cycle_reachable(&supersedes) {
        diagnostics.push(Diagnostic::new(
            identities[start].path.clone(),
            "supersession_cycle",
            "supersession references must not form a cycle",
        ));
    }
    diagnostics
}

pub(super) fn progress_diagnostics(
    entries: &[EntrySnapshot],
    active: &Activation,
) -> Vec<Diagnostic> {
    let mut facts = ValidationFacts::default();
    let scopes = ScopeIndex::new(active);
    for entry in entries {
        let resolved = scopes.resolve(&entry.path);
        let parsed = crate::scanning::classify_facts(
            &entry.path,
            &entry.original_bytes,
            active,
            resolved.kind,
        )
        .facts;
        facts.insert(entry, resolved, parsed);
    }
    progress_diagnostics_facts(entries, active, &facts)
}

fn progress_diagnostics_facts(
    entries: &[EntrySnapshot],
    _active: &Activation,
    facts: &ValidationFacts,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let scopes = &facts.resolved;
    let mut observed_tickets = BTreeMap::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.kind == Some(Kind::Ticket))
    {
        let scope = scopes[entry.path.as_str()].scope;
        if let Some(identity) = entry.identity.as_deref() {
            observed_tickets.entry((scope, identity)).or_insert(entry);
        }
        if let Some(name) = entry
            .path
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".md"))
        {
            observed_tickets.entry((scope, name)).or_insert(entry);
        }
    }
    let accepted = entries
        .iter()
        .filter_map(|entry| {
            if entry.kind == Some(Kind::Ticket) && entry.classification == Classification::Governed
            {
                if let Some(RecordSummary::WorkItem { id, status, .. }) = &entry.summary {
                    if status == "accepted" {
                        return Some((scopes[entry.path.as_str()].scope, id.as_str()));
                    }
                }
            }
            None
        })
        .collect::<BTreeSet<_>>();
    for entry in entries.iter().filter(|entry| {
        entry.kind == Some(Kind::Progress) && entry.classification == Classification::Governed
    }) {
        let Some(log) = facts.progress.get(&entry.path) else {
            continue;
        };
        let scope = scopes[entry.path.as_str()].scope;
        for (operation_id, ticket_id) in log {
            let accepted_ticket = accepted.contains(&(scope, ticket_id.as_str()));
            if !accepted_ticket {
                let observed = observed_tickets.get(&(scope, ticket_id.as_str())).copied();
                let status = observed.and_then(|entry| match &entry.summary {
                    Some(RecordSummary::WorkItem { status, .. }) => Some(status.clone()),
                    _ => None,
                });
                let mut diagnostic = Diagnostic::new(
                    &entry.path,
                    "invalid_progress_ticket",
                    "progress entry must target a governed accepted ticket in this investigation",
                );
                diagnostic.progress_ticket = Some(casefile_core::ProgressTicketDiagnostic {
                    ticket_id: ticket_id.clone(),
                    operation_id: operation_id.clone(),
                    reason: match observed {
                        None => "missing_in_investigation".into(),
                        Some(entry) if entry.classification != Classification::Governed => {
                            "invalid_ticket".into()
                        }
                        Some(_) => "non_accepted_ticket".into(),
                    },
                    investigation: scope.unwrap_or_default().into(),
                    classification: observed.map(|entry| entry.classification),
                    status,
                    next_query: casefile_core::ProgressTicketQuery {
                        query: "record_index".into(),
                        scope: casefile_core::ProgressTicketScope {
                            project: scopes[entry.path.as_str()]
                                .project
                                .unwrap_or_default()
                                .into(),
                            investigation: scope
                                .and_then(|path| {
                                    crate::activation::investigation_identity(
                                        scopes[entry.path.as_str()].project.unwrap_or_default(),
                                        path,
                                    )
                                })
                                .unwrap_or_default()
                                .into(),
                        },
                    },
                });
                diagnostics.push(diagnostic);
            }
        }
    }
    diagnostics
}

fn attachment_target(entry_path: &str, attachment: &str) -> Option<String> {
    if !safe_relative(attachment) {
        return None;
    }
    let (parent, _) = entry_path.rsplit_once('/')?;
    let target = format!("{parent}/{attachment}");
    safe_relative(&target).then_some(target)
}

fn cycle_reachable(graph: &BTreeMap<String, Vec<String>>) -> BTreeSet<&str> {
    let mut remaining = BTreeMap::new();
    let mut predecessors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (node, next) in graph {
        let mut count = 0;
        for target in next.iter().filter(|target| graph.contains_key(*target)) {
            count += 1;
            predecessors.entry(target).or_default().push(node);
        }
        remaining.insert(node.as_str(), count);
    }
    let mut leaves = remaining
        .iter()
        .filter_map(|(node, count)| (*count == 0).then_some(*node))
        .collect::<Vec<_>>();
    while let Some(node) = leaves.pop() {
        if let Some(parents) = predecessors.get(node) {
            for parent in parents {
                let count = remaining.get_mut(parent).expect("graph predecessor");
                *count -= 1;
                if *count == 0 {
                    leaves.push(parent);
                }
            }
        }
    }
    remaining
        .into_iter()
        .filter_map(|(node, count)| (count > 0).then_some(node))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::attachment_target;

    #[test]
    fn attachment_targets_keep_canonical_store_separators() {
        let evidence = "projects/demo/investigations/sample/evidence/observation.md";
        assert_eq!(
            attachment_target(evidence, "attachment.txt").as_deref(),
            Some("projects/demo/investigations/sample/evidence/attachment.txt")
        );
        assert_eq!(
            attachment_target(evidence, "nested/attachment.txt").as_deref(),
            Some("projects/demo/investigations/sample/evidence/nested/attachment.txt")
        );
        for unsafe_attachment in ["../attachment.txt", "/attachment.txt"] {
            assert_eq!(attachment_target(evidence, unsafe_attachment), None);
        }
    }
}
