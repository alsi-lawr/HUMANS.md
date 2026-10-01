use crate::{
    InvestigationScope, InvestigationScopedIdentity, StoreError,
    activation::{
        Activation, ActivationState, activation_content, contains_path, investigation_identity,
    },
    revision::store_revision,
    scanning::{
        InventoryEntry, InventoryKind, collect_selected, inventory_target, read_inventory_entry,
    },
};
use casefile_core::Revision;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

mod tokens;
pub use tokens::{
    AttachmentState, CatalogueToken, CheckFreshness, ReadDependency, ScopeReadTarget,
    ScopeReadToken,
};

pub(super) fn observe_attachment(root: &Path, path: &str) -> Result<AttachmentState, StoreError> {
    match crate::store::require_safe_target_parent(
        root,
        Path::new(path).parent().unwrap_or_else(|| Path::new("")),
        path,
    ) {
        Ok(()) => {}
        Err(StoreError::Invalid(_)) => return Ok(AttachmentState::Unsafe),
        Err(error) => return Err(error),
    }
    match std::fs::symlink_metadata(root.join(path)) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(AttachmentState::Regular)
        }
        Ok(_) => Ok(AttachmentState::Unsafe),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AttachmentState::Missing),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read_activation(
    root: &Path,
) -> Result<(std::sync::Arc<Activation>, InventoryEntry), StoreError> {
    let entry = inventory_target(root, "casefile.toml")?.ok_or_else(|| {
        StoreError::Invalid("investigation reads require active Casefile configuration".into())
    })?;
    if entry.kind != InventoryKind::Regular {
        return Err(StoreError::Invalid(
            "casefile.toml must be a regular non-symlink file".into(),
        ));
    }
    let config = crate::scanning::read_observed_entry(root, "casefile.toml", &entry)?;
    let (state, active, _) = activation_content(Some(&config));
    if state != ActivationState::Active {
        return Err(StoreError::Invalid(
            "investigation reads require active Casefile configuration".into(),
        ));
    }
    Ok((std::sync::Arc::new(active), entry))
}

pub(super) struct ScopeObservation {
    pub active: std::sync::Arc<Activation>,
    config_revision: Revision,
    pub path: String,
    pub entries: Vec<(String, InventoryEntry)>,
    pub support: Vec<(String, InventoryEntry)>,
    pub progress: Option<(String, Option<InventoryEntry>)>,
    pub target: ScopeReadTarget,
    activation_revision: Revision,
    mapping_revision: Option<Revision>,
    pub mapping_diagnostics: Vec<casefile_core::Diagnostic>,
    pub attachments: BTreeMap<String, AttachmentState>,
}

impl ScopeObservation {
    pub fn begin(root: &Path, target: ScopeReadTarget) -> Result<Self, StoreError> {
        let (active, entry) = read_activation(root)?;
        Self::from_active(root, target, active, entry.revision)
    }

    fn from_active(
        root: &Path,
        target: ScopeReadTarget,
        active: std::sync::Arc<Activation>,
        config_revision: Revision,
    ) -> Result<Self, StoreError> {
        let scope = target.scope();
        let mut matches = active
            .projects
            .get(&scope.project)
            .into_iter()
            .flat_map(|project| &project.investigations)
            .filter(|path| {
                investigation_identity(&scope.project, path) == Some(scope.investigation.as_str())
            });
        let path = matches
            .next()
            .filter(|_| matches.next().is_none())
            .ok_or_else(|| {
                StoreError::Invalid(
                    "investigation scope must resolve to exactly one activated path".into(),
                )
            })?
            .clone();
        let project = &active.projects[&scope.project];
        let mut activation_fields = vec![
            ("prefix".to_owned(), Revision(project.prefix.clone())),
            ("path".to_owned(), Revision(path.clone())),
        ];
        for nested in project
            .investigations
            .iter()
            .filter(|nested| *nested != &path && contains_path(&path, nested))
        {
            activation_fields.push((nested.clone(), Revision("activated".into())));
        }
        activation_fields.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let activation_revision = digest(&activation_fields);
        let mut entries = Vec::new();
        let mut support = Vec::new();
        if let ScopeReadTarget::RecordDetail { identity } = &target {
            if crate::layout::checked_path(&identity.identity)? != identity.identity
                || identity.identity.contains('/')
            {
                return Err(StoreError::Invalid(
                    "record identity must be one contained filename component".into(),
                ));
            }
            let scopes = crate::activation::ScopeIndex::new(&active);
            for kind in ["tickets", "epics"] {
                for disposition in ["accepted", "provisional", "rejected"] {
                    let relative = format!("{path}/{kind}/{disposition}/{}.md", identity.identity);
                    let resolved = scopes.resolve(&relative);
                    if resolved.scope != Some(path.as_str()) {
                        continue;
                    }
                    if let Some(entry) = inventory_target(root, &relative)? {
                        entries.push((relative, entry));
                    }
                }
            }
        } else {
            collect_selected(root, &root.join(&path), &mut entries, |relative, is_dir| {
                let selected = relative.strip_prefix(&path).expect("selected traversal");
                if project.investigations.iter().any(|nested| {
                    nested != &path && contains_path(&path, nested) && relative.starts_with(nested)
                }) {
                    return false;
                }
                let first = selected.components().next();
                let included = match first {
                    None => true,
                    Some(Component::Normal(first)) => match &target {
                        ScopeReadTarget::RecordIndex { .. } => {
                            first == "tickets" || first == "epics"
                        }
                        ScopeReadTarget::Boards { .. } => {
                            first == "tickets" || first == "epics" || first == "boards"
                        }
                        ScopeReadTarget::StrategyTransitions { .. } => first == "strategy",
                        ScopeReadTarget::Diagnostics { .. } => [
                            "request.md",
                            "final-disposition.md",
                            "implementation-plan",
                            "strategy",
                            "decision-log",
                            "evidence",
                            "review",
                            "tickets",
                            "epics",
                            "boards",
                            "progress",
                        ]
                        .iter()
                        .any(|group| first == *group),
                        _ => false,
                    },
                    _ => false,
                };
                included
                    && (is_dir
                        || selected.to_str().is_none_or(|local| {
                            crate::layout::scope_container_native_relative_path(local)
                                || crate::layout::kind_in_native_relative_path(local).is_some()
                        }))
            })?;
            require_directory_containers(&entries, &path)?;
            entries.retain(|(relative, _)| {
                let kind = crate::layout::kind_in_scope(relative, &path);
                match target {
                    ScopeReadTarget::RecordIndex { .. } => matches!(
                        kind,
                        Some(casefile_core::Kind::Ticket | casefile_core::Kind::Epic)
                    ),
                    ScopeReadTarget::Boards { .. } => matches!(
                        kind,
                        Some(
                            casefile_core::Kind::Ticket
                                | casefile_core::Kind::Epic
                                | casefile_core::Kind::Board
                        )
                    ),
                    ScopeReadTarget::StrategyTransitions { .. } => {
                        kind == Some(casefile_core::Kind::StrategyTransition)
                    }
                    ScopeReadTarget::Diagnostics { .. } => kind.is_some(),
                    _ => false,
                }
            });
        }
        entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut mapping_diagnostics = Vec::new();
        let mapping_revision = if matches!(target, ScopeReadTarget::Diagnostics { .. }) {
            for base in &project.investigations {
                collect_selected(root, &root.join(base), &mut support, |relative, is_dir| {
                    let local = relative.strip_prefix(base).expect("support traversal");
                    let included = local.components().next().is_none_or(|component| matches!(component, Component::Normal(value) if value == "tickets" || value == "epics" || value == "decision-log"));
                    included
                        && (is_dir
                            || local.to_str().is_none_or(|local| {
                                crate::layout::scope_container_native_relative_path(local)
                                    || matches!(
                                        crate::layout::kind_in_native_relative_path(local),
                                        Some(
                                            casefile_core::Kind::Ticket
                                                | casefile_core::Kind::Epic
                                                | casefile_core::Kind::Decision
                                        )
                                    )
                            }))
                })?;
                require_directory_containers(&support, base)?;
            }
            let decision_path = format!("projects/{}/decision-log", scope.project);
            collect_selected(
                root,
                &root.join(&decision_path),
                &mut support,
                |relative, is_dir| {
                    let local = relative
                        .strip_prefix(&decision_path)
                        .expect("decision traversal");
                    !is_dir
                        && local
                            .to_str()
                            .is_none_or(|name| name.ends_with(".md") && name.contains('-'))
                },
            )?;
            let index = crate::activation::ScopeIndex::new(&active);
            support.sort_by(|left, right| left.0.cmp(&right.0));
            support.dedup_by(|later, previous| {
                if later.0 == previous.0 {
                    std::mem::swap(later, previous);
                    true
                } else {
                    false
                }
            });
            support.retain(|(relative, _)| {
                entries
                    .binary_search_by(|entry| entry.0.cmp(relative))
                    .is_err()
                    && matches!(
                        index.resolve(relative).kind,
                        Some(
                            casefile_core::Kind::Ticket
                                | casefile_core::Kind::Epic
                                | casefile_core::Kind::Decision
                        )
                    )
            });
            let bytes = read_optional(root, "projects.toml")?;
            let value = bytes
                .as_deref()
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(|text| toml::from_str::<toml::Value>(text).ok());
            let selected = value
                .as_ref()
                .and_then(|value| value.get("projects"))
                .and_then(|projects| projects.get(&scope.project))
                .and_then(toml::Value::as_str)
                .filter(|value| Path::new(value).is_absolute());
            if selected.is_none() {
                mapping_diagnostics.push(
                    casefile_core::Diagnostic::new(
                        "projects.toml",
                        "invalid_project_map",
                        "selected project requires a platform-absolute string source root",
                    )
                    .field(&scope.project),
                );
            }
            Some(digest(&[(
                scope.project.clone(),
                Revision(selected.unwrap_or("invalid").into()),
            )]))
        } else {
            None
        };
        Ok(Self {
            active,
            config_revision,
            path,
            entries,
            support,
            progress: None,
            target,
            activation_revision,
            mapping_revision,
            mapping_diagnostics,
            attachments: BTreeMap::new(),
        })
    }

    pub fn observe_progress(&mut self, root: &Path) -> Result<(), StoreError> {
        let path = format!("{}/progress/log.toml", self.path);
        let resolved = crate::activation::ScopeIndex::new(&self.active).resolve(&path);
        if resolved.scope == Some(self.path.as_str())
            && resolved.kind == Some(casefile_core::Kind::Progress)
        {
            let entry = inventory_target(root, &path)?;
            self.progress = Some((path, entry));
        }
        Ok(())
    }

    pub fn token(&self) -> ScopeReadToken {
        let records = inventory_digest(&self.entries);
        let mut dependencies = vec![
            ReadDependency::ActivationSelection {
                revision: self.activation_revision.clone(),
            },
            ReadDependency::SelectedRecords {
                revision: records.clone(),
            },
        ];
        let domain = match &self.target {
            ScopeReadTarget::RecordIndex { .. } => "record_index",
            ScopeReadTarget::RecordDetail { .. } => "record_detail",
            ScopeReadTarget::Boards { .. } => "boards",
            ScopeReadTarget::StrategyTransitions { .. } => "strategy_transitions",
            ScopeReadTarget::Diagnostics { .. } => "diagnostics",
        };
        let scope = self.target.scope();
        let mut fields = vec![
            ("query".into(), Revision(domain.into())),
            ("project".into(), Revision(scope.project.clone())),
            (
                "investigation".into(),
                Revision(scope.investigation.clone()),
            ),
            ("activation".into(), self.activation_revision.clone()),
            ("records".into(), records),
        ];
        if let ScopeReadTarget::RecordDetail { identity } = &self.target {
            fields.push(("identity".into(), Revision(identity.identity.clone())));
        }
        if let Some((path, entry)) = &self.progress {
            let revision = entry.as_ref().map(|entry| entry.revision.clone());
            fields.push((
                path.clone(),
                revision.clone().unwrap_or(Revision("absent".into())),
            ));
            dependencies.push(ReadDependency::Progress {
                path: path.clone(),
                revision,
            });
        }
        if let Some(mapping) = &self.mapping_revision {
            let support = inventory_digest(&self.support);
            fields.push(("support".into(), support.clone()));
            fields.push(("mapping".into(), mapping.clone()));
            dependencies.push(ReadDependency::ProjectSupport { revision: support });
            dependencies.push(ReadDependency::ProjectMapping {
                project: self.target.scope().project.clone(),
                revision: mapping.clone(),
            });
        }
        if !self.attachments.is_empty() {
            for (path, state) in &self.attachments {
                fields.push((path.clone(), Revision(format!("attachment:{state:?}"))));
            }
            dependencies.push(ReadDependency::Attachments {
                targets: self.attachments.clone(),
            });
        }
        ScopeReadToken {
            target: self.target.clone(),
            dependencies,
            revision: digest(&fields),
        }
    }

    pub fn verify(&self, root: &Path) -> Result<ScopeReadToken, StoreError> {
        let config = inventory_target(root, "casefile.toml")?;
        let mut current = if config.as_ref().is_some_and(|entry| {
            entry.kind == InventoryKind::Regular && entry.revision == self.config_revision
        }) {
            Self::from_active(
                root,
                self.target.clone(),
                self.active.clone(),
                self.config_revision.clone(),
            )?
        } else {
            Self::begin(root, self.target.clone())?
        };
        if self.progress.is_some() {
            current.observe_progress(root)?;
        }
        for path in self.attachments.keys() {
            current
                .attachments
                .insert(path.clone(), observe_attachment(root, path)?);
        }
        let token = self.token();
        if current.token() != token {
            return Err(StoreError::Invalid(
                "requested scope or read dependencies changed; retry the exact target".into(),
            ));
        }
        Ok(token)
    }
}

fn digest(fields: &[(String, Revision)]) -> Revision {
    store_revision(
        fields
            .iter()
            .map(|(field, revision)| (field.as_str(), revision)),
        true,
    )
}
fn inventory_digest(entries: &[(String, InventoryEntry)]) -> Revision {
    store_revision(
        entries
            .iter()
            .map(|(path, entry)| (path.as_str(), &entry.revision)),
        false,
    )
}
fn read_optional(root: &Path, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
    match inventory_target(root, path)? {
        None => Ok(None),
        Some(entry) if entry.kind == InventoryKind::Regular => {
            read_inventory_entry(&entry).map(Some)
        }
        Some(_) => Err(StoreError::Invalid(format!(
            "{path} must be a regular non-symlink file"
        ))),
    }
}

fn require_directory_containers(
    entries: &[(String, InventoryEntry)],
    scope: &str,
) -> Result<(), StoreError> {
    if let Some(path) = entries
        .iter()
        .map(|(path, _)| path)
        .filter(|path| crate::layout::scope_container(path, scope))
        .min()
    {
        return Err(StoreError::Invalid(format!(
            "{path}: governed container must be a non-symlink directory"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_observation_rejects_affecting_changes_but_admits_unrelated_edits() {
        let root = tempfile::tempdir().unwrap();
        let scope = "projects/demo/investigations/sample";
        std::fs::write(root.path().join("casefile.toml"), format!("schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['{scope}']\n")).unwrap();
        let tickets = root.path().join(format!("{scope}/tickets/accepted"));
        std::fs::create_dir_all(&tickets).unwrap();
        let ticket = tickets.join("HMD-001.md");
        std::fs::write(&ticket, "requested descriptor").unwrap();
        let target = ScopeReadTarget::RecordIndex {
            scope: InvestigationScope {
                project: "demo".into(),
                investigation: "sample".into(),
            },
        };
        let observed = ScopeObservation::begin(root.path(), target.clone()).unwrap();
        std::fs::write(root.path().join("unrelated.txt"), "external edit").unwrap();
        observed.verify(root.path()).unwrap();
        std::fs::write(&ticket, "changed requested descriptor").unwrap();
        assert!(observed.verify(root.path()).is_err());
        let observed = ScopeObservation::begin(root.path(), target.clone()).unwrap();
        std::fs::write(tickets.join("HMD-002.md"), "new candidate").unwrap();
        assert!(observed.verify(root.path()).is_err());
        let observed = ScopeObservation::begin(root.path(), target).unwrap();
        std::fs::write(root.path().join("casefile.toml"), format!("schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['{scope}', '{scope}/nested']\n")).unwrap();
        assert!(observed.verify(root.path()).is_err());
    }
}
