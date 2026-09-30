use super::{
    InventoryEntry, classification::ParsedFacts, inventory_target, scoped::selected_entry,
};
use crate::{
    activation::{Activation, ScopeIndex},
    derived::{StrategyBindingState, resolve_binding},
    layout::checked_path,
    read_context::read_activation,
    store::StoreError,
};
use casefile_core::{Classification, EntrySnapshot, Kind, RecordSummary};
use serde::Serialize;
use std::{path::Path, sync::Arc};

#[derive(Serialize)]
pub struct WriterBindingProjection {
    pub strategy_id: String,
    pub adapter: String,
    pub binding: StrategyBindingState,
}

struct SelectedRead<'a> {
    root: &'a Path,
    active: Arc<Activation>,
    config: InventoryEntry,
    files: Vec<(String, Option<InventoryEntry>)>,
}

impl<'a> SelectedRead<'a> {
    fn begin(root: &'a Path) -> Result<Self, StoreError> {
        let (active, config) = read_activation(root)?;
        Ok(Self {
            root,
            active,
            config,
            files: Vec::new(),
        })
    }

    fn entry(
        &mut self,
        path: &str,
        kind: Kind,
    ) -> Result<Option<(EntrySnapshot, ParsedFacts)>, StoreError> {
        let file = inventory_target(self.root, path)?;
        let result = file
            .as_ref()
            .map(|file| {
                selected_entry(self.root, path, file, &self.active, Some(kind))
                    .map(|(entry, parsed, _)| (entry, parsed))
            })
            .transpose()?;
        self.files.push((path.into(), file));
        Ok(result)
    }

    fn verify(&self) -> Result<(), StoreError> {
        for (path, original) in &self.files {
            let current = inventory_target(self.root, path)?;
            if current.as_ref().map(|entry| &entry.revision)
                != original.as_ref().map(|entry| &entry.revision)
            {
                return Err(StoreError::StaleTargetRevision);
            }
        }
        let config = inventory_target(self.root, "casefile.toml")?;
        if config.as_ref().map(|entry| &entry.revision) != Some(&self.config.revision) {
            let (active, _) = read_activation(self.root)?;
            let before = ScopeIndex::new(&self.active);
            let after = ScopeIndex::new(&active);
            for (path, _) in &self.files {
                let old = before.resolve(path);
                let new = after.resolve(path);
                if (old.project, old.scope, old.kind) != (new.project, new.scope, new.kind)
                    || old
                        .project
                        .and_then(|p| self.active.projects.get(p))
                        .map(|p| &p.prefix)
                        != new
                            .project
                            .and_then(|p| active.projects.get(p))
                            .map(|p| &p.prefix)
                {
                    return Err(StoreError::StaleTargetRevision);
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn read_editable_entry(
    root: &Path,
    path: &str,
    kind: Kind,
) -> Result<Option<EntrySnapshot>, StoreError> {
    let path = checked_path(path)?;
    let mut read = SelectedRead::begin(root)?;
    if !kind.is_writable() || ScopeIndex::new(&read.active).resolve(&path).kind != Some(kind) {
        return Err(StoreError::Invalid(
            "selected record is not an editable governed ticket, epic, or board".into(),
        ));
    }
    let entry = read.entry(&path, kind)?.map(|(entry, _)| entry);
    if entry
        .as_ref()
        .is_some_and(|entry| entry.classification != Classification::Governed)
    {
        return Err(StoreError::Invalid(format!(
            "{path}: selected editable record is invalid or unsafe"
        )));
    }
    read.verify()?;
    Ok(entry)
}

pub(crate) fn project_writer_binding(
    root: &Path,
    investigation: &str,
    strategy_id: &str,
) -> Result<WriterBindingProjection, StoreError> {
    let investigation = checked_path(investigation)?;
    let mut read = SelectedRead::begin(root)?;
    let scopes = ScopeIndex::new(&read.active);
    if scopes.resolve(&investigation).scope != Some(investigation.as_str()) {
        return Err(StoreError::Invalid(
            "investigation must be an exact activated path".into(),
        ));
    }
    let implementation_path = format!("{investigation}/strategy/implementation.toml");
    let binding_path = format!("{investigation}/strategy/bindings.toml");
    if scopes.resolve(&implementation_path).scope != Some(investigation.as_str())
        || scopes.resolve(&binding_path).scope != Some(investigation.as_str())
    {
        return Err(StoreError::Invalid(
            "selected strategy is outside the activated investigation".into(),
        ));
    }
    let (implementation, parsed) = read
        .entry(&implementation_path, Kind::Strategy)?
        .ok_or_else(|| StoreError::Invalid("selected implementation strategy is missing".into()))?;
    let Some(RecordSummary::Strategy {
        strategy_id: selected_id,
        phase,
        adapter,
    }) = &implementation.summary
    else {
        return Err(StoreError::Invalid(
            "selected implementation strategy is invalid or ungraphable".into(),
        ));
    };
    if implementation.classification != Classification::Governed || parsed.strategy.is_none() {
        return Err(StoreError::Invalid(
            "selected implementation strategy is invalid or ungraphable".into(),
        ));
    }
    if phase != "implementation" || selected_id != strategy_id || adapter != "codex" {
        return Err(StoreError::Invalid(
            "requested Codex implementation strategy is not selected".into(),
        ));
    }
    let binding = read.entry(&binding_path, Kind::StrategyBinding)?;
    let invalid = binding
        .as_ref()
        .is_some_and(|(entry, _)| entry.classification != Classification::Governed);
    let value = binding
        .as_ref()
        .and_then(|(entry, _)| match &entry.summary {
            Some(RecordSummary::StrategyBinding { binding }) => Some(binding),
            _ => None,
        });
    let projected = resolve_binding(
        phase,
        adapter,
        parsed.strategy.as_ref().unwrap(),
        value,
        invalid,
    );
    read.verify()?;
    Ok(WriterBindingProjection {
        strategy_id: selected_id.clone(),
        adapter: adapter.clone(),
        binding: projected,
    })
}
