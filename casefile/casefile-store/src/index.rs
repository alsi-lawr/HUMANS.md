use casefile_core::{Diagnostic, Revision};
use serde::{Deserialize, Serialize};

use crate::{
    derived::{
        DerivedBoard, DerivedRecord, DerivedRelationship, DerivedSnapshot, RecordScope,
        ScopedIdentity,
    },
    store::StoreError,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Indexed<T> {
    Current {
        source_revision: Revision,
        value: T,
    },
    Missing,
    Stale {
        indexed_revision: Revision,
        current_revision: Revision,
    },
}

pub trait RevisionSource {
    fn current_revision(&self) -> Result<Revision, StoreError>;
}

pub trait DerivedIndex {
    type Prepared;
    type Error;
    fn prepare(&self, snapshot: &DerivedSnapshot) -> Result<Self::Prepared, Self::Error>;
    fn publish(
        &self,
        prepared: Self::Prepared,
        source: &dyn RevisionSource,
    ) -> Result<Indexed<()>, Self::Error>;
    fn state(&self, current: &Revision) -> Result<Indexed<()>, Self::Error>;
    fn record(
        &self,
        current: &Revision,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Option<DerivedRecord>>, Self::Error>;
    fn records(
        &self,
        current: &Revision,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<DerivedRecord>>, Self::Error>;
    fn relationships(
        &self,
        current: &Revision,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Vec<DerivedRelationship>>, Self::Error>;
    fn diagnostics(&self, current: &Revision) -> Result<Indexed<Vec<Diagnostic>>, Self::Error>;
    fn boards(
        &self,
        current: &Revision,
        scope: &RecordScope,
    ) -> Result<Indexed<Vec<DerivedBoard>>, Self::Error>;
}
