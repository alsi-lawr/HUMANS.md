use anyhow::{Result, bail};
use casefile_core::{ChangeRequest, Diagnostic, Revision};
use casefile_store::{
    DerivedBoard, DerivedIndex, DerivedRecord, DerivedRelationship, Indexed, Provider,
    ProviderApplyOutcome, ProviderPreview, ProviderRecordApplyResult, ProviderSnapshot,
    RecordScope, ScopedIdentity,
};
use casefile_store_sqlite::SqliteIndex;

pub(crate) struct Workbench {
    provider: Provider<SqliteIndex>,
    index: SqliteIndex,
}

impl Workbench {
    pub(crate) fn new(provider: Provider<SqliteIndex>, index: SqliteIndex) -> Self {
        Self { provider, index }
    }

    pub(crate) fn records(
        &self,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<DerivedRecord>>> {
        let revision = self.refresh()?;
        Ok(self.index.records(&revision, scope, search)?)
    }

    pub(crate) fn snapshot(&self) -> Result<ProviderSnapshot> {
        Ok(self.provider.snapshot()?)
    }

    pub(crate) fn relationships(
        &self,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Vec<DerivedRelationship>>> {
        let revision = self.refresh()?;
        Ok(self.index.relationships(&revision, identity)?)
    }

    pub(crate) fn boards(&self, scope: &RecordScope) -> Result<Indexed<Vec<DerivedBoard>>> {
        let revision = self.refresh()?;
        Ok(self.index.boards(&revision, scope)?)
    }

    pub(crate) fn diagnostics(&self) -> Result<Indexed<Vec<Diagnostic>>> {
        let revision = self.refresh()?;
        Ok(self.index.diagnostics(&revision)?)
    }

    pub(crate) fn preview(
        &self,
        request: ChangeRequest,
    ) -> Result<ProviderPreview, casefile_store::ProviderError> {
        self.provider.preview_record(request)
    }

    pub(crate) fn apply(
        &self,
        preview: ProviderPreview,
    ) -> Result<ProviderApplyOutcome<ProviderRecordApplyResult>, casefile_store::ProviderError>
    {
        self.provider.apply_record(preview)
    }

    fn refresh(&self) -> Result<Revision> {
        let state = self.provider.refresh_full_cache()?;
        let casefile_store::CacheState::Current {
            source_revision: revision,
        } = state
        else {
            bail!("provider cache did not publish the canonical revision")
        };
        if !matches!(self.index.state(&revision)?, Indexed::Current { .. }) {
            bail!("provider cache did not publish the canonical revision");
        }
        Ok(revision)
    }
}
