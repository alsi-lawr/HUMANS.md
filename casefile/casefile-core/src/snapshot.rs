use serde::{Deserialize, Serialize};

use crate::record::{Classification, Kind, RecordSummary};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CasefileSnapshot {
    pub revision: Revision,
    pub entries: Vec<EntrySnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntrySnapshot {
    pub path: String,
    pub classification: Classification,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    pub content_revision: Revision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RecordSummary>,
    pub original_bytes: Vec<u8>,
}
