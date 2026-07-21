//! Filesystem and Git boundary for the compact Casefile v1 contract.
#![allow(clippy::collapsible_if)] // Nested validation keeps individual rules readable.

mod activation;
mod derived;
mod index;
mod layout;
mod scanning;
mod store;
mod validation;
mod writing;

pub use activation::ActivationState;
pub use derived::{
    DerivedBoard, DerivedBoardColumn, DerivedCard, DerivedRecord, DerivedRelationship,
    DerivedSnapshot, RecordScope, RelationshipKind, ScopedIdentity,
};
pub use index::{DerivedIndex, Indexed, RevisionSource};
pub use scanning::ScanResult;
pub use store::{Store, StoreError};
