use super::*;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum PresentationTarget {
    Store,
    Project { project: String },
    Investigation { project: String, path: String },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PresentationScope {
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investigation: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationCoverageState {
    Pending,
    Partial,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationCoverage {
    pub catalogue: PresentationCoverageState,
    pub payload: PresentationCoverageState,
    pub facts: PresentationCoverageState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationProgress {
    pub completed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactAvailability {
    Unavailable,
    Available,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "availability", content = "value", rename_all = "snake_case")]
pub enum PresentationFact<T> {
    Unavailable,
    Available(T),
}

impl<T> PresentationFact<T> {
    pub fn availability(&self) -> FactAvailability {
        match self {
            Self::Unavailable => FactAvailability::Unavailable,
            Self::Available(_) => FactAvailability::Available,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationFileKind {
    Regular,
    Directory,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationFileMetadata {
    pub kind: PresentationFileKind,
    pub length: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_unix_nanos: Option<u128>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationProject {
    pub slug: String,
    pub prefix: String,
    pub investigations: Vec<PresentationInvestigation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationInvestigation {
    pub identity: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationCatalogue {
    pub activation: ActivationState,
    pub projects: Vec<PresentationProject>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationSummary {
    pub title: String,
    pub record: RecordSummary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationContentHandle {
    pub(super) session: u64,
    pub(super) id: u64,
    pub(super) path: String,
}

impl PresentationContentHandle {
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<PresentationScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub metadata: PresentationFileMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_handle: Option<PresentationContentHandle>,
    pub classification: PresentationFact<Classification>,
    pub identity: PresentationFact<Option<String>>,
    pub summary: PresentationFact<Option<PresentationSummary>>,
    /// Local read/parse and scope-local diagnostics, not canonical whole-store validation.
    pub diagnostics: PresentationFact<Vec<Diagnostic>>,
    pub progress: PresentationFact<Option<DerivedTicketProgress>>,
    pub relationships: PresentationFact<Vec<DerivedRelationship>>,
    pub boards: PresentationFact<Vec<DerivedBoard>>,
    pub body: PresentationFact<Vec<u8>>,
    pub derived: Option<DerivedRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationLoadRequest {
    pub generation: u64,
    pub target: PresentationTarget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PresentationEvent {
    Catalogue {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
        catalogue: PresentationCatalogue,
    },
    Entries {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
        entries: Vec<Arc<PresentationEntry>>,
    },
    Complete {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
    },
    Failure {
        generation: u64,
        target: PresentationTarget,
        coverage: PresentationCoverage,
        progress: PresentationProgress,
        message: String,
    },
}

impl PresentationEvent {
    pub fn generation(&self) -> u64 {
        match self {
            Self::Catalogue { generation, .. }
            | Self::Entries { generation, .. }
            | Self::Complete { generation, .. }
            | Self::Failure { generation, .. } => *generation,
        }
    }

    pub fn target(&self) -> &PresentationTarget {
        match self {
            Self::Catalogue { target, .. }
            | Self::Entries { target, .. }
            | Self::Complete { target, .. }
            | Self::Failure { target, .. } => target,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "selector", rename_all = "snake_case")]
pub enum PresentationContentSelector {
    Handle { handle: PresentationContentHandle },
    Path { path: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresentationContentRequest {
    pub generation: u64,
    pub target: PresentationTarget,
    pub selector: PresentationContentSelector,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PresentationContentEvent {
    Pending {
        generation: u64,
        target: PresentationTarget,
        path: String,
    },
    Loaded {
        generation: u64,
        target: PresentationTarget,
        entry: Box<PresentationEntry>,
    },
    Failure {
        generation: u64,
        target: PresentationTarget,
        path: Option<String>,
        message: String,
    },
}
