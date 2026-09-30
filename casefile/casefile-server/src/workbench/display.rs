use anyhow::{Context, Result, bail};
use casefile_core::{Kind, RecordDraft, WorkItemDraft};
use casefile_store::DerivedRecord;
use serde::Serialize;

/// The existing HTTP record contract includes editing and rendered display fields.
/// These are materialized only for returned records, not retained in the cache document.
#[derive(Serialize)]
pub(crate) struct DisplayRecord {
    #[serde(flatten)]
    record: DerivedRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    rendered_markdown: Option<String>,
    search_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    work_item: Option<WorkItemDraft>,
}

impl DisplayRecord {
    pub(crate) fn from_record(mut record: DerivedRecord) -> Result<Self> {
        let work_item = if record.work_item.take().is_some() {
            let text = record
                .content
                .as_deref()
                .context("work item display requires source content")?;
            let kind = record
                .kind
                .context("work item display requires record kind")?;
            if !matches!(kind, Kind::Ticket | Kind::Epic) {
                bail!("work item display has incompatible kind");
            }
            match casefile_core::parse_draft(&record.path, kind, text).map_err(|diagnostics| {
                anyhow::anyhow!(
                    "{}: invalid cached work item: {:?}",
                    record.path,
                    diagnostics
                )
            })? {
                RecordDraft::Ticket(item) | RecordDraft::Epic(item) => Some(item),
                _ => bail!("work item display has incompatible draft"),
            }
        } else {
            None
        };
        Ok(Self {
            rendered_markdown: record.rendered_markdown(),
            search_text: record.search_text(),
            record,
            work_item,
        })
    }
}
