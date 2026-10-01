use super::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename = "catalogue")]
pub struct CatalogueToken {
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum ScopeReadTarget {
    RecordIndex {
        scope: InvestigationScope,
    },
    RecordDetail {
        identity: InvestigationScopedIdentity,
    },
    Boards {
        scope: InvestigationScope,
    },
    StrategyTransitions {
        scope: InvestigationScope,
    },
    Diagnostics {
        scope: InvestigationScope,
    },
}

impl ScopeReadTarget {
    pub(crate) fn scope(&self) -> &InvestigationScope {
        match self {
            Self::RecordDetail { identity } => &identity.scope,
            Self::RecordIndex { scope }
            | Self::Boards { scope }
            | Self::StrategyTransitions { scope }
            | Self::Diagnostics { scope } => scope,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "dependency", rename_all = "snake_case")]
pub enum ReadDependency {
    ActivationSelection {
        revision: Revision,
    },
    SelectedRecords {
        revision: Revision,
    },
    Progress {
        path: String,
        revision: Option<Revision>,
    },
    ProjectSupport {
        revision: Revision,
    },
    Attachments {
        targets: BTreeMap<String, AttachmentState>,
    },
    ProjectMapping {
        project: String,
        revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentState {
    Missing,
    Regular,
    Unsafe,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename = "scope_read")]
pub struct ScopeReadToken {
    pub target: ScopeReadTarget,
    pub dependencies: Vec<ReadDependency>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckFreshness {
    Store { revision: Revision },
    ScopeRead { token: ScopeReadToken },
}
