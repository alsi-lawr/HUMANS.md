use serde::{Deserialize, Serialize};

use crate::{diagnostic::Diagnostic, record::RecordDraft, snapshot::Revision};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ChangeRequest {
    Create { path: String, draft: RecordDraft },
    Replace { path: String, draft: RecordDraft },
    Delete { path: String },
}

impl ChangeRequest {
    pub fn path(&self) -> &str {
        match self {
            Self::Create { path, .. } | Self::Replace { path, .. } | Self::Delete { path } => path,
        }
    }

    pub fn rendered(&self) -> Option<Result<Vec<u8>, Diagnostic>> {
        match self {
            Self::Create { path, draft } | Self::Replace { path, draft } => {
                Some(crate::render_draft(path, draft))
            }
            Self::Delete { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub request: ChangeRequest,
    pub expected_target_revision: Option<Revision>,
    pub expected_store_revision: Revision,
    pub proposed_store_revision: Revision,
    pub diagnostics: Vec<Diagnostic>,
    pub diff: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyResult {
    pub path: String,
    pub resulting_target_revision: Option<Revision>,
    pub resulting_store_revision: Revision,
    pub diff: String,
}
