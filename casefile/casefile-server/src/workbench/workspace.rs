use super::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceContext {
    pub(crate) known_token: Option<casefile_store::WorkspaceReadToken>,
    pub(crate) search: Option<String>,
}

pub(super) enum WorkspaceValue {
    Unchanged {
        matching_paths: Vec<String>,
    },
    Updated {
        records: Vec<display::DisplayRecord>,
        diagnostics: Vec<Diagnostic>,
        matching_paths: Vec<String>,
    },
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum WorkspaceResponse {
    Unchanged {
        freshness: casefile_store::WorkspaceReadToken,
        matching_paths: Vec<String>,
    },
    Updated {
        freshness: casefile_store::WorkspaceReadToken,
        records: Vec<display::DisplayRecord>,
        diagnostics: Vec<Diagnostic>,
        matching_paths: Vec<String>,
    },
}

impl WorkspaceResponse {
    pub(super) fn from_projection(
        freshness: casefile_store::WorkspaceReadToken,
        value: WorkspaceValue,
    ) -> Self {
        match value {
            WorkspaceValue::Unchanged { matching_paths } => Self::Unchanged {
                freshness,
                matching_paths,
            },
            WorkspaceValue::Updated {
                records,
                diagnostics,
                matching_paths,
            } => Self::Updated {
                freshness,
                records,
                diagnostics,
                matching_paths,
            },
        }
    }
}

pub(super) fn project(
    index: &SqliteIndex,
    token: &casefile_store::WorkspaceReadToken,
    context: &WorkspaceContext,
) -> Result<WorkspaceValue> {
    if context.known_token.as_ref() == Some(token) {
        return Ok(WorkspaceValue::Unchanged {
            matching_paths: current(index.record_paths(
                &token.source_revision,
                None,
                context.search.as_deref(),
            )?)?,
        });
    }
    let records = current(index.records(&token.source_revision, None, None)?)?;
    let matching_paths = match context.search.as_deref() {
        Some(search) if !search.is_empty() => {
            current(index.record_paths(&token.source_revision, None, Some(search))?)?
        }
        _ => records.iter().map(|record| record.path.clone()).collect(),
    };
    let diagnostics = current(index.diagnostics(&token.source_revision)?)?;
    let records = records
        .into_iter()
        .map(display::DisplayRecord::from_record)
        .collect::<Result<_>>()?;
    Ok(WorkspaceValue::Updated {
        records,
        diagnostics,
        matching_paths,
    })
}
