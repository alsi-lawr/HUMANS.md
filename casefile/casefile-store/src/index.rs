use casefile_core::{Diagnostic, Revision};
use serde::{Deserialize, Serialize};
use std::path::Path;

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IndexPublicationId(Revision);

impl IndexPublicationId {
    pub fn for_file(path: &Path) -> Result<Option<Self>, StoreError> {
        Ok(crate::revision::target_revision(path)?.map(Self))
    }
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
    fn publication(&self, current: &Revision) -> Result<Indexed<IndexPublicationId>, Self::Error>;
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
    fn record_paths(
        &self,
        current: &Revision,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<String>>, Self::Error>;
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
