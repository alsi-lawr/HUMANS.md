use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{Diagnostic, SCHEMA_VERSION};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    Unknown,
    InProgress,
    InReview,
    Verifying,
    Blocked,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressNoteCategory {
    Deviation,
    Quirk,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProgressEntry {
    Transition {
        id: String,
        recorded_at: String,
        recorded_by: String,
        ticket_id: String,
        from: ProgressStatus,
        to: ProgressStatus,
    },
    Note {
        id: String,
        recorded_at: String,
        recorded_by: String,
        ticket_id: String,
        category: ProgressNoteCategory,
        message: String,
    },
}

impl ProgressEntry {
    pub fn id(&self) -> &str {
        match self {
            Self::Transition { id, .. } | Self::Note { id, .. } => id,
        }
    }

    pub fn ticket_id(&self) -> &str {
        match self {
            Self::Transition { ticket_id, .. } | Self::Note { ticket_id, .. } => ticket_id,
        }
    }
}

impl ProgressStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::InProgress => "in_progress",
            Self::InReview => "in_review",
            Self::Verifying => "verifying",
            Self::Blocked => "blocked",
            Self::Complete => "complete",
        }
    }
}

impl ProgressNoteCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deviation => "deviation",
            Self::Quirk => "quirk",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressLog {
    pub entries: Vec<ProgressEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogWire {
    schema_version: i64,
    #[serde(default)]
    entries: Vec<EntryWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    id: String,
    recorded_at: String,
    recorded_by: String,
    ticket_id: String,
    kind: String,
    #[serde(rename = "from")]
    from: Option<ProgressStatus>,
    to: Option<ProgressStatus>,
    category: Option<ProgressNoteCategory>,
    message: Option<String>,
}

#[allow(clippy::result_large_err)]
pub fn parse_progress_log(path: &str, text: &str) -> Result<ProgressLog, Vec<Diagnostic>> {
    let wire: LogWire = toml::from_str(text).map_err(|error| {
        vec![Diagnostic::new(
            path,
            "invalid_progress_log",
            error.to_string(),
        )]
    })?;
    if wire.schema_version != i64::from(SCHEMA_VERSION) {
        return Err(vec![
            Diagnostic::new(path, "invalid_schema_version", "schema_version must be 1")
                .field("schema_version"),
        ]);
    }
    let mut entries = Vec::with_capacity(wire.entries.len());
    for wire in wire.entries {
        let entry = match wire.kind.as_str() {
            "transition" => {
                if wire.category.is_some() || wire.message.is_some() {
                    return Err(vec![Diagnostic::new(
                        path,
                        "invalid_progress_entry",
                        "transition entries may not contain note fields",
                    )]);
                }
                let (Some(from), Some(to)) = (wire.from, wire.to) else {
                    return Err(vec![Diagnostic::new(
                        path,
                        "invalid_progress_entry",
                        "transition entries need from and to statuses",
                    )]);
                };
                ProgressEntry::Transition {
                    id: wire.id,
                    recorded_at: wire.recorded_at,
                    recorded_by: wire.recorded_by,
                    ticket_id: wire.ticket_id,
                    from,
                    to,
                }
            }
            "note" => {
                if wire.from.is_some() || wire.to.is_some() {
                    return Err(vec![Diagnostic::new(
                        path,
                        "invalid_progress_entry",
                        "note entries may not contain transition fields",
                    )]);
                }
                let (Some(category), Some(message)) = (wire.category, wire.message) else {
                    return Err(vec![Diagnostic::new(
                        path,
                        "invalid_progress_entry",
                        "note entries need category and message",
                    )]);
                };
                ProgressEntry::Note {
                    id: wire.id,
                    recorded_at: wire.recorded_at,
                    recorded_by: wire.recorded_by,
                    ticket_id: wire.ticket_id,
                    category,
                    message,
                }
            }
            _ => {
                return Err(vec![Diagnostic::new(
                    path,
                    "invalid_progress_entry",
                    "entry kind must be transition or note",
                )]);
            }
        };
        entries.push(entry);
    }
    let log = ProgressLog { entries };
    validate_progress_log(path, &log).map_err(|diagnostic| vec![diagnostic])?;
    Ok(log)
}

#[allow(clippy::result_large_err)]
pub fn validate_progress_log(path: &str, log: &ProgressLog) -> Result<(), Diagnostic> {
    let mut ids = BTreeSet::new();
    let mut current = BTreeMap::<&str, ProgressStatus>::new();
    for entry in &log.entries {
        let (id, recorded_at, recorded_by, ticket_id) = match entry {
            ProgressEntry::Transition {
                id,
                recorded_at,
                recorded_by,
                ticket_id,
                ..
            }
            | ProgressEntry::Note {
                id,
                recorded_at,
                recorded_by,
                ticket_id,
                ..
            } => (id, recorded_at, recorded_by, ticket_id),
        };
        if id.trim().is_empty() || !ids.insert(id) {
            return Err(Diagnostic::new(
                path,
                "invalid_progress_operation_id",
                "operation IDs must be non-empty and unique",
            ));
        }
        if OffsetDateTime::parse(recorded_at, &Rfc3339).is_err() {
            return Err(Diagnostic::new(
                path,
                "invalid_progress_timestamp",
                "recorded_at must be RFC 3339",
            ));
        }
        if recorded_by.trim().is_empty() || ticket_id.trim().is_empty() {
            return Err(Diagnostic::new(
                path,
                "invalid_progress_entry",
                "recorded_by and ticket_id must be non-empty",
            ));
        }
        match entry {
            ProgressEntry::Transition { from, to, .. } => {
                let prior = current
                    .get(ticket_id.as_str())
                    .copied()
                    .unwrap_or(ProgressStatus::Unknown);
                if *from != prior {
                    return Err(Diagnostic::new(
                        path,
                        "stale_progress_from",
                        "transition from must match the ticket's current progress status",
                    ));
                }
                if from == to {
                    return Err(Diagnostic::new(
                        path,
                        "no_op_progress_transition",
                        "transition from and to must differ",
                    ));
                }
                current.insert(ticket_id, *to);
            }
            ProgressEntry::Note { message, .. } if message.trim().is_empty() => {
                return Err(Diagnostic::new(
                    path,
                    "invalid_progress_entry",
                    "note message must be non-empty",
                ));
            }
            ProgressEntry::Note { .. } => {}
        }
    }
    Ok(())
}

pub fn render_progress_log(log: &ProgressLog) -> String {
    let mut output = String::from("schema_version = 1\n");
    for entry in &log.entries {
        output.push_str("\n[[entries]]\n");
        match entry {
            ProgressEntry::Transition {
                id,
                recorded_at,
                recorded_by,
                ticket_id,
                from,
                to,
            } => {
                output.push_str(&format!(
                    "id = {}\nrecorded_at = {}\nrecorded_by = {}\nticket_id = {}\nkind = \"transition\"\nfrom = {}\nto = {}\n",
                    toml_string(id), toml_string(recorded_at), toml_string(recorded_by), toml_string(ticket_id), toml_string(from.as_str()), toml_string(to.as_str())
                ));
            }
            ProgressEntry::Note {
                id,
                recorded_at,
                recorded_by,
                ticket_id,
                category,
                message,
            } => {
                output.push_str(&format!(
                    "id = {}\nrecorded_at = {}\nrecorded_by = {}\nticket_id = {}\nkind = \"note\"\ncategory = {}\nmessage = {}\n",
                    toml_string(id), toml_string(recorded_at), toml_string(recorded_by), toml_string(ticket_id), toml_string(category.as_str()), toml_string(message)
                ));
            }
        }
    }
    output
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.into()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_declaration_ordered_log() {
        let source = "schema_version = 1\n\n[[entries]]\nid = \"start\"\nrecorded_at = \"2026-07-26T10:00:00Z\"\nrecorded_by = \"root\"\nticket_id = \"HMD-001\"\nkind = \"transition\"\nfrom = \"unknown\"\nto = \"in_progress\"\n\n[[entries]]\nid = \"note\"\nrecorded_at = \"2026-07-26T10:01:00Z\"\nrecorded_by = \"root\"\nticket_id = \"HMD-001\"\nkind = \"note\"\ncategory = \"quirk\"\nmessage = \"Needs a small fixture.\"\n";
        let parsed = parse_progress_log("progress/log.toml", source).expect("parse");
        assert_eq!(
            parsed,
            parse_progress_log("progress/log.toml", &render_progress_log(&parsed))
                .expect("round trip")
        );
    }

    #[test]
    fn rejects_mixed_fields_duplicate_ids_and_stale_or_no_op_transitions() {
        for source in [
            "schema_version = 1\n[[entries]]\nid='one'\nrecorded_at='2026-07-26T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='note'\nfrom='unknown'\ncategory='quirk'\nmessage='x'\n",
            "schema_version = 1\n[[entries]]\nid='one'\nrecorded_at='2026-07-26T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='transition'\nfrom='unknown'\nto='in_progress'\n[[entries]]\nid='one'\nrecorded_at='2026-07-26T10:01:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='note'\ncategory='quirk'\nmessage='x'\n",
            "schema_version = 1\n[[entries]]\nid='one'\nrecorded_at='2026-07-26T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='transition'\nfrom='in_progress'\nto='complete'\n",
            "schema_version = 1\n[[entries]]\nid='one'\nrecorded_at='2026-07-26T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='transition'\nfrom='unknown'\nto='unknown'\n",
        ] {
            assert!(parse_progress_log("progress/log.toml", source).is_err());
        }
    }
}
