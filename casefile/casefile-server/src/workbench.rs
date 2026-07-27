use anyhow::{Result, bail};
use casefile_core::{ChangeRequest, Diagnostic, Revision};
use casefile_store::{
    DerivedBoard, DerivedIndex, DerivedRecord, DerivedRelationship, Indexed, Provider,
    ProviderApplyOutcome, ProviderPreview, ProviderQuery, ProviderQueryResult,
    ProviderRecordApplyResult, ProviderSnapshot, RecordScope, ScopedIdentity,
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

    pub(crate) fn provider_query(&self, query: ProviderQuery) -> Result<ProviderQueryResult> {
        Ok(self.provider.query(query)?)
    }

    pub(crate) fn relationships(
        &self,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Vec<DerivedRelationship>>> {
        let revision = self.refresh()?;
        Ok(self.index.relationships(&revision, identity)?)
    }

    pub(crate) fn boards(&self, scope: &RecordScope) -> Result<Indexed<Vec<DerivedBoard>>> {
        let ProviderQueryResult::Boards { revision, boards } = self
            .provider
            .query(ProviderQuery::Boards { scope: Some(scope.clone()) })?
        else {
            unreachable!("board query returns boards")
        };
        Ok(Indexed::Current {
            source_revision: revision,
            value: boards,
        })
    }

    pub(crate) fn diagnostics(&self) -> Result<Indexed<Vec<Diagnostic>>> {
        let snapshot = self.provider.snapshot()?;
        Ok(Indexed::Current {
            source_revision: snapshot.revision,
            value: snapshot.diagnostics,
        })
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
    ) -> Result<ProviderApplyOutcome<ProviderRecordApplyResult>, casefile_store::ProviderError> {
        self.provider.apply_record(preview)
    }

    fn refresh(&self) -> Result<Revision> {
        let snapshot = self.provider.snapshot()?;
        if !matches!(
            self.index.state(&snapshot.revision)?,
            Indexed::Current { .. }
        ) {
            bail!("provider cache did not publish the canonical revision");
        }
        Ok(snapshot.revision)
    }
}
