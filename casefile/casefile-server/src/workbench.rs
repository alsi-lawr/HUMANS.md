use anyhow::{Result, bail};
use casefile_core::{ApplyResult, ChangeRequest, Diagnostic, Preview, Revision};
use casefile_store::{
    DerivedBoard, DerivedIndex, DerivedRecord, DerivedRelationship, Indexed, RecordScope,
    ScopedIdentity, Store, StoreError,
};
use casefile_store_sqlite::SqliteIndex;

pub(crate) struct Workbench {
    store: Store,
    index: SqliteIndex,
}

pub(crate) struct ApplyOutcome {
    pub(crate) result: ApplyResult,
    pub(crate) index_error: Option<anyhow::Error>,
}

impl Workbench {
    pub(crate) fn new(store: Store, index: SqliteIndex) -> Self {
        Self { store, index }
    }

    pub(crate) fn records(
        &self,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<DerivedRecord>>> {
        let revision = self.refresh()?;
        Ok(self.index.records(&revision, scope, search)?)
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

    pub(crate) fn preview(&self, request: ChangeRequest) -> Result<Preview, StoreError> {
        self.store.preview(request)
    }

    pub(crate) fn apply(&self, preview: Preview) -> Result<ApplyOutcome, StoreError> {
        let result = self.store.apply(preview)?;
        let index_error = self.refresh().err();
        Ok(ApplyOutcome {
            result,
            index_error,
        })
    }

    fn refresh(&self) -> Result<Revision> {
        let snapshot = self.store.derived_snapshot()?;
        match self.index.state(&snapshot.source_revision)? {
            Indexed::Current { .. } => {}
            Indexed::Missing | Indexed::Stale { .. } => {
                let prepared = self.index.prepare(&snapshot)?;
                if !matches!(
                    self.index.publish(prepared, &self.store)?,
                    Indexed::Current { .. }
                ) {
                    bail!("canonical content changed during index refresh");
                }
            }
        }
        Ok(snapshot.source_revision)
    }
}
