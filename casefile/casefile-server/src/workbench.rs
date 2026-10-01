use anyhow::{Result, bail};
use casefile_core::{ChangeRequest, Diagnostic};
use casefile_store::{
    DerivedBoard, DerivedIndex, DerivedRelationship, Indexed, Provider, ProviderApplyOutcome,
    ProviderPreview, ProviderRecordApplyResult, ProviderSnapshot, RecordScope, ScopedIdentity,
};
use casefile_store_sqlite::SqliteIndex;
use serde::Serialize;

#[path = "workbench/display.rs"]
mod display;
#[path = "workbench/workspace.rs"]
pub(crate) mod workspace;

pub(crate) struct Workbench {
    provider: Provider<SqliteIndex>,
}

#[derive(Serialize)]
pub(crate) struct ApplyReply {
    #[serde(flatten)]
    outcome: ProviderApplyOutcome<ProviderRecordApplyResult>,
    workspace: Option<workspace::WorkspaceResponse>,
}

fn current<T>(result: Indexed<T>) -> Result<T> {
    match result {
        Indexed::Current { value, .. } => Ok(value),
        Indexed::Missing => bail!("current index is missing"),
        Indexed::Stale { .. } => bail!("index changed during projection"),
    }
}

impl Workbench {
    pub(crate) fn new(provider: Provider<SqliteIndex>) -> Self {
        Self { provider }
    }

    fn read<T>(
        &self,
        project: impl FnOnce(&SqliteIndex, &casefile_store::WorkspaceReadToken) -> Result<T>,
    ) -> Result<Indexed<T>> {
        let (token, value) = self.provider.read_full_index(project)?;
        Ok(Indexed::Current {
            source_revision: token.source_revision,
            value,
        })
    }

    pub(crate) fn records(
        &self,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<display::DisplayRecord>>> {
        self.read(|index, token| {
            current(index.records(&token.source_revision, scope, search)?)?
                .into_iter()
                .map(display::DisplayRecord::from_record)
                .collect()
        })
    }

    pub(crate) fn snapshot(&self) -> Result<ProviderSnapshot> {
        Ok(self.provider.snapshot()?)
    }

    pub(crate) fn relationships(
        &self,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Vec<DerivedRelationship>>> {
        self.read(|index, token| current(index.relationships(&token.source_revision, identity)?))
    }

    pub(crate) fn boards(&self, scope: &RecordScope) -> Result<Indexed<Vec<DerivedBoard>>> {
        self.read(|index, token| current(index.boards(&token.source_revision, scope)?))
    }

    pub(crate) fn diagnostics(&self) -> Result<Indexed<Vec<Diagnostic>>> {
        self.read(|index, token| current(index.diagnostics(&token.source_revision)?))
    }

    pub(crate) fn workspace(
        &self,
        context: &workspace::WorkspaceContext,
    ) -> Result<workspace::WorkspaceResponse> {
        let (token, value) = self
            .provider
            .read_full_index(|index, token| workspace::project(index, token, context))?;
        Ok(workspace::WorkspaceResponse::from_projection(token, value))
    }

    pub(crate) fn preview(
        &self,
        request: ChangeRequest,
    ) -> Result<ProviderPreview, casefile_store::ProviderError> {
        self.provider.preview_record(request)
    }

    pub(crate) fn apply(
        &self,
        preview_id: &str,
        context: Option<&workspace::WorkspaceContext>,
    ) -> Result<ApplyReply, casefile_store::ProviderError> {
        let Some(context) = context else {
            return Ok(ApplyReply {
                outcome: self.provider.apply_record(preview_id)?,
                workspace: None,
            });
        };
        let (outcome, projection) = self
            .provider
            .apply_record_with_index(preview_id, |index, token| {
                workspace::project(index, token, context)
            })?;
        Ok(ApplyReply {
            outcome,
            workspace: projection
                .map(|(token, value)| workspace::WorkspaceResponse::from_projection(token, value)),
        })
    }
}
