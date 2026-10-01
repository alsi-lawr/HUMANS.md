//! Bounded, catalogue-first presentation loading kept separate from canonical Store state.
//!
//! Presentation completion covers only the advertised catalogue, entries, and explicit fact
//! availability. It is never a canonical revision and is never consumed by Provider or writer
//! admission. Canonical scans continue to read and preserve every included body.

use crate::{
    activation::{Activation, ActivationState, ScopeIndex, activation, investigation_identity},
    derived::{
        DerivedBoard, DerivedRecord, DerivedRelationship, DerivedTicketProgress, derive_boards,
        fold_progress, local_record, project_binding, ticket_progress,
    },
    layout::normalize_planning_relative,
    scanning::{binding_diagnostics_facts, classify_facts, is_store_path_excluded},
    store::StoreError,
};
use casefile_core::{Classification, Diagnostic, EntrySnapshot, Kind, RecordSummary, Revision};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::UNIX_EPOCH,
};

mod model;
pub use model::*;
mod session;
use session::*;
pub use session::{PresentationContentStream, PresentationSession, PresentationStream};
mod catalogue;
use catalogue::*;
mod reader;
use reader::*;
mod loading;
use loading::*;
mod scope;
use scope::*;
mod facts;
use facts::*;
mod entries;
use entries::*;
mod content;
use content::*;

pub const PRESENTATION_BATCH_LIMIT: usize = 1024;
pub const PRESENTATION_CHANNEL_CAPACITY: usize = 8;
static NEXT_PRESENTATION_SESSION: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
mod tests;
