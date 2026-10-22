use crate::{
    StoreError,
    activation::{Activation, PathFacts, ScopeIndex},
    layout::safe_relative,
    mutation_metadata::{from_bytes, list, stem},
    scanning::{InventoryEntry, InventoryKind, inventory_target, read_observed_entry},
};
use casefile_core::Kind;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
};

pub(super) fn matches(
    source: PathFacts<'_>,
    reference: &str,
    target_path: &str,
    target_id: Option<&str>,
    target: PathFacts<'_>,
) -> bool {
    if source.project.is_none() || source.project != target.project {
        return false;
    }
    if reference.contains('/') {
        return safe_relative(reference) && reference == target_path;
    }
    target_id == Some(reference)
        && (source.scope == target.scope
            || target.scope.is_none() && target.kind == Some(Kind::Decision))
}

pub(super) fn candidates(
    root: &Path,
    active: &Activation,
    source: &str,
    reference: &str,
    decision: bool,
) -> Result<Vec<String>, StoreError> {
    let scopes = ScopeIndex::new(active);
    let from = scopes.resolve(source);
    if reference.contains('/') {
        let target = scopes.resolve(reference);
        return Ok(
            if safe_relative(reference)
                && from.project.is_some()
                && from.project == target.project
                && matches!(
                    target.kind,
                    Some(Kind::Ticket | Kind::Epic | Kind::Decision)
                )
            {
                vec![reference.into()]
            } else {
                Vec::new()
            },
        );
    }
    if !safe_relative(reference) {
        return Ok(Vec::new());
    }
    let Some(scope) = from.scope else {
        return Ok(Vec::new());
    };
    if !decision {
        return Ok(["tickets", "epics"]
            .into_iter()
            .flat_map(|kind| {
                ["accepted", "provisional", "rejected"]
                    .into_iter()
                    .map(move |status| format!("{scope}/{kind}/{status}/{reference}.md"))
            })
            .filter(|path| scopes.resolve(path).scope == from.scope)
            .collect());
    }
    let mut paths = Vec::new();
    for (directory, scope) in [
        (format!("{scope}/decision-log"), Some(scope)),
        (
            format!("projects/{}/decision-log", from.project.unwrap()),
            None,
        ),
    ] {
        for path in list(root, &directory, false, &scopes, scope)? {
            let name = stem(&path);
            if (name == reference
                || name
                    .strip_prefix(reference)
                    .is_some_and(|suffix| suffix.starts_with('-')))
                && scopes.resolve(&path).kind == Some(Kind::Decision)
            {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

pub(super) fn observe_dependencies(
    root: &Path,
    active: &Activation,
    entries: &[(String, InventoryEntry)],
) -> Result<Vec<(String, InventoryEntry)>, StoreError> {
    let scopes = ScopeIndex::new(active);
    let mut seen = entries
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    let mut pending = VecDeque::new();
    for (path, entry) in entries {
        if entry.kind == InventoryKind::Regular
            && matches!(scopes.resolve(path).kind, Some(Kind::Ticket | Kind::Epic))
        {
            pending.push_back((
                path.clone(),
                from_bytes(
                    &read_observed_entry(root, path, entry)?,
                    scopes.resolve(path).kind,
                ),
            ));
        }
    }
    let mut support = BTreeMap::new();
    while let Some((source, header)) = pending.pop_front() {
        for (reference, decision) in header
            .decision_refs
            .iter()
            .map(|reference| (reference, true))
            .chain(
                header
                    .related_tickets
                    .iter()
                    .chain(&header.supersedes)
                    .chain(&header.superseded_by)
                    .map(|reference| (reference, false)),
            )
        {
            for path in candidates(root, active, &source, reference, decision)? {
                if !seen.insert(path.clone()) {
                    continue;
                }
                let Some(entry) = inventory_target(root, &path)? else {
                    continue;
                };
                let kind = scopes.resolve(&path).kind;
                if entry.kind == InventoryKind::Regular
                    && matches!(kind, Some(Kind::Ticket | Kind::Epic))
                {
                    pending.push_back((
                        path.clone(),
                        from_bytes(&read_observed_entry(root, &path, &entry)?, kind),
                    ));
                }
                support.insert(path, entry);
            }
        }
    }
    Ok(support.into_iter().collect())
}
