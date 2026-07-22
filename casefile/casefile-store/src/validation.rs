use crate::{
    activation::{Activation, project_for, scope_for},
    layout::safe_relative,
};
use casefile_core::{
    Classification, Diagnostic, EntrySnapshot, Kind, RecordDraft, RecordSummary,
    parse_metadata_arrays,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn cross_validate(entries: &[EntrySnapshot], active: &Activation) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut identities: BTreeMap<&str, &EntrySnapshot> = BTreeMap::new();
    let paths: BTreeSet<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
    let mut supersedes = BTreeMap::new();
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
            if let Some(project) = active.projects.iter().find(|(_, project)| {
                project
                    .investigations
                    .iter()
                    .any(|base| entry.path.starts_with(&(base.to_owned() + "/")))
            }) {
                if !entry
                    .identity
                    .as_deref()
                    .is_some_and(|id| id.starts_with(&(project.1.prefix.clone() + "-")))
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "project_prefix",
                        "record identity must use the configured project prefix",
                    ));
                }
            }
        }
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.summary, Some(RecordSummary::WorkItem { .. })))
    {
        let RecordSummary::WorkItem { id, .. } =
            entry.summary.as_ref().expect("filtered work item")
        else {
            unreachable!()
        };
        let text = std::str::from_utf8(&entry.original_bytes).unwrap_or_default();
        if let Ok(draft) =
            casefile_core::parse_draft(&entry.path, entry.kind.expect("work kind"), text)
        {
            let item = match draft {
                RecordDraft::Ticket(item) | RecordDraft::Epic(item) => item,
                _ => unreachable!(),
            };
            let scope = scope_for(&entry.path, active);
            for reference in &item.decision_refs {
                let resolves = identities.get(reference.as_str()).is_some_and(|target| {
                    target.kind == Some(Kind::Decision)
                        && project_for(&target.path, active) == project_for(&entry.path, active)
                        && match scope_for(&target.path, active) {
                            None => true,
                            Some(target_scope) => Some(target_scope) == scope,
                        }
                });
                if reference == id || !resolves {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "unresolved_reference",
                        "decision references must resolve in the same project or investigation",
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
                    || identities
                        .get(reference.as_str())
                        .is_none_or(|target| scope_for(&target.path, active) != scope)
                {
                    diagnostics.push(Diagnostic::new(
                        &entry.path,
                        "unresolved_reference",
                        "references must resolve within the governed project/investigation scope",
                    ));
                }
            }
            supersedes.insert(id.clone(), item.supersedes);
        }
    }
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, Some(Kind::Evidence | Kind::Review)))
    {
        if let Ok(text) = std::str::from_utf8(&entry.original_bytes) {
            if let Ok((refs, attachments)) = parse_metadata_arrays(&entry.path, text) {
                let scope = scope_for(&entry.path, active);
                for reference in refs {
                    if identities
                        .get(reference.as_str())
                        .is_none_or(|target| scope_for(&target.path, active) != scope)
                    {
                        diagnostics.push(Diagnostic::new(&entry.path, "unresolved_reference", "references must resolve within the governed project/investigation scope"));
                    }
                }
                for attachment in attachments {
                    let target = Path::new(&entry.path)
                        .parent()
                        .map(|parent| parent.join(&attachment))
                        .and_then(|path| path.to_str().map(str::to_owned));
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
    }
    for start in supersedes.keys() {
        if has_cycle(
            start,
            &supersedes,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        ) {
            diagnostics.push(Diagnostic::new(
                identities[start.as_str()].path.clone(),
                "supersession_cycle",
                "supersession references must not form a cycle",
            ));
        }
    }
    diagnostics
}

fn has_cycle(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    checked: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(node.into()) {
        return true;
    }
    if checked.contains(node) {
        visiting.remove(node);
        return false;
    }
    let result = graph.get(node).is_some_and(|next| {
        next.iter()
            .any(|id| has_cycle(id, graph, visiting, checked))
    });
    visiting.remove(node);
    checked.insert(node.into());
    result
}
