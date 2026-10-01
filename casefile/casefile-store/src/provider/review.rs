use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPreviewKind {
    Record,
    RecordBatch,
    Progress,
    DefaultDeliveryBoard,
    StrategyTransition,
    WriterBinding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReviewOperationKind {
    Create,
    Replace,
    Delete,
    Bootstrap,
    Append,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderReviewOperation {
    pub operation: ProviderReviewOperationKind,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPreview {
    pub preview_id: String,
    pub kind: ProviderPreviewKind,
    pub approval_required: bool,
    pub no_op: bool,
    pub operations: Vec<ProviderReviewOperation>,
    pub diagnostics: Vec<Diagnostic>,
    pub diff: String,
}

fn record_operation(request: &ChangeRequest) -> ProviderReviewOperation {
    ProviderReviewOperation {
        operation: match request {
            ChangeRequest::Create { .. } => ProviderReviewOperationKind::Create,
            ChangeRequest::Replace { .. } => ProviderReviewOperationKind::Replace,
            ChangeRequest::Delete { .. } => ProviderReviewOperationKind::Delete,
        },
        path: request.path().to_owned(),
    }
}

pub(super) fn preview(id: &str, original: &StoredPreview) -> ProviderPreview {
    let (operations, diagnostics, diff, no_op, approval_required) = match original {
        StoredPreview::Record(value) | StoredPreview::Board(value) => (
            vec![record_operation(&value.request)],
            value.diagnostics.clone(),
            value.diff.clone(),
            value.diff.is_empty() && value.diagnostics.is_empty(),
            record_approval_required(&value.request),
        ),
        StoredPreview::RecordBatch(value) => (
            value.requests.iter().map(record_operation).collect(),
            value.diagnostics.clone(),
            value.diff.clone(),
            value.diff.is_empty() && value.diagnostics.is_empty(),
            value.requests.iter().any(record_approval_required),
        ),
        StoredPreview::Progress(value) => (
            vec![ProviderReviewOperation {
                operation: if value.request.bootstrap {
                    ProviderReviewOperationKind::Bootstrap
                } else {
                    ProviderReviewOperationKind::Append
                },
                path: value.path.clone(),
            }],
            value.diagnostics.clone(),
            value.diff.clone(),
            value.no_op,
            false,
        ),
        StoredPreview::StrategyTransition(value) => {
            governed(&value.changes, &value.diagnostics, value.no_op)
        }
        StoredPreview::WriterBinding(value) => {
            governed(&value.changes, &value.diagnostics, value.no_op)
        }
    };
    ProviderPreview {
        preview_id: id.to_owned(),
        kind: original.kind(),
        approval_required,
        no_op,
        operations,
        diagnostics,
        diff,
    }
}

fn governed(
    changes: &[crate::GovernedChange],
    diagnostics: &[Diagnostic],
    no_op: bool,
) -> (
    Vec<ProviderReviewOperation>,
    Vec<Diagnostic>,
    String,
    bool,
    bool,
) {
    let operations = changes
        .iter()
        .map(|change| ProviderReviewOperation {
            operation: match (
                change.expected_target_revision.is_some(),
                change.proposed_target_revision.is_some(),
            ) {
                (false, true) => ProviderReviewOperationKind::Create,
                (true, false) => ProviderReviewOperationKind::Delete,
                _ => ProviderReviewOperationKind::Replace,
            },
            path: change.path.clone(),
        })
        .collect();
    let diff = changes
        .iter()
        .map(|change| change.diff.as_str())
        .collect::<String>();
    (operations, diagnostics.to_owned(), diff, no_op, false)
}
