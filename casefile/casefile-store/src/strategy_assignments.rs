use crate::{layout::checked_path, store::StoreError};
use casefile_core::{
    AssignmentPhase, Classification, EntrySnapshot, RecordSummary, Revision, RoleAssignment,
    StrategyBinding, StrategyProjection, is_strategy_id,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyAssignmentTarget {
    pub investigation: String,
    pub phase: AssignmentPhase,
    pub strategy_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyAssignmentRevisions {
    pub matrix: Revision,
    pub assignments: Option<Revision>,
    pub legacy_writer_binding: Option<Revision>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyAssignmentRequest {
    pub target: StrategyAssignmentTarget,
    pub expected_revisions: StrategyAssignmentRevisions,
    pub roles: Vec<RoleAssignment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrategyAssignmentProjection {
    pub target: StrategyAssignmentTarget,
    pub adapter: String,
    pub revisions: StrategyAssignmentRevisions,
    pub roles: BTreeMap<String, ProjectedRoleAssignment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectedRoleAssignment {
    pub platform_profiles: BTreeSet<String>,
    pub selection: Option<SelectedRoleAssignment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectedRoleAssignment {
    pub model: String,
    pub reasoning_effort: String,
    pub source: RoleAssignmentSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleAssignmentSource {
    StrategyAssignment,
    LegacyWriterBinding,
}

impl StrategyAssignmentTarget {
    pub(super) fn canonical(mut self) -> Result<Self, StoreError> {
        self.investigation = checked_path(&self.investigation)?;
        if !is_strategy_id(&self.strategy_id) {
            return Err(StoreError::Invalid("invalid assignment strategy ID".into()));
        }
        checked_path(&self.assignments_path())?;
        Ok(self)
    }

    pub(super) fn matrix_path(&self) -> String {
        format!("{}/strategy/{}.toml", self.investigation, self.phase)
    }

    pub(super) fn assignments_path(&self) -> String {
        format!(
            "{}/strategy/assignments/{}/{}.toml",
            self.investigation, self.phase, self.strategy_id
        )
    }

    pub(super) fn legacy_path(&self) -> Option<String> {
        (self.phase == AssignmentPhase::Implementation)
            .then(|| format!("{}/strategy/bindings.toml", self.investigation))
    }

    pub(super) fn matrix<'a>(
        &self,
        entry: &'a EntrySnapshot,
        matrix: Option<&'a StrategyProjection>,
    ) -> Result<(&'a str, &'a StrategyProjection), StoreError> {
        if let Some(RecordSummary::Strategy {
            strategy_id,
            adapter,
            ..
        }) = &entry.summary
            && entry.classification == Classification::Governed
            && strategy_id == &self.strategy_id
            && let Some(matrix) = matrix
        {
            return Ok((adapter, matrix));
        }
        Err(StoreError::Invalid(
            "the exact requested strategy must be selected with a complete matrix".into(),
        ))
    }

    pub(super) fn project_roles(
        &self,
        matrix: &StrategyProjection,
        assignments: Option<&EntrySnapshot>,
        legacy: Option<&EntrySnapshot>,
    ) -> Result<BTreeMap<String, ProjectedRoleAssignment>, StoreError> {
        let mut roles = BTreeMap::new();
        for worker in &matrix.workers {
            let role =
                roles
                    .entry(worker.role.clone())
                    .or_insert_with(|| ProjectedRoleAssignment {
                        platform_profiles: BTreeSet::new(),
                        selection: None,
                    });
            role.platform_profiles
                .insert(worker.platform_profile.clone());
        }
        if let Some(entry) = assignments {
            let Some(RecordSummary::StrategyAssignments { assignments }) = &entry.summary else {
                return Err(StoreError::Invalid(
                    "saved strategy assignments are invalid".into(),
                ));
            };
            for row in &assignments.roles {
                if let Some(role) = roles.get_mut(&row.role) {
                    role.selection = Some(SelectedRoleAssignment {
                        model: row.model.clone(),
                        reasoning_effort: row.reasoning_effort.clone(),
                        source: RoleAssignmentSource::StrategyAssignment,
                    });
                }
            }
        } else if let Some(RecordSummary::StrategyBinding { binding }) =
            legacy.and_then(|entry| entry.summary.as_ref())
            && matrix
                .workers
                .iter()
                .filter(|worker| worker.role == binding.role)
                .count()
                == 1
            && self.legacy_applies(binding)
            && let Some(role) = roles.get_mut(&binding.role)
        {
            role.selection = Some(SelectedRoleAssignment {
                model: binding.model.clone(),
                reasoning_effort: binding.reasoning_effort.clone(),
                source: RoleAssignmentSource::LegacyWriterBinding,
            });
        }
        Ok(roles)
    }

    fn legacy_applies(&self, binding: &StrategyBinding) -> bool {
        self.phase == AssignmentPhase::Implementation
            && binding.adapter == "codex"
            && (!matches!(
                binding.resolution.mode.as_str(),
                "named_profile" | "runtime_override"
            ) || binding.resolution.value.split(';').any(|route| {
                route.split_once('=').is_some_and(|(strategy, profile)| {
                    strategy == self.strategy_id && !profile.is_empty()
                })
            }))
    }
}
