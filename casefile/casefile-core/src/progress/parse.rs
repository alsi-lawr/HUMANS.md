use super::{
    ChangeRef, Diagnostic, EntryRef, ProgressEntry, ProgressLog, ProgressNoteCategory,
    ProgressStatus, SCHEMA_VERSION, validate_entries,
};
use serde::{Deserialize, Deserializer, de::Visitor};
use std::{borrow::Cow, collections::BTreeMap, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressSummary {
    pub status: ProgressStatus,
    pub note_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressProjection {
    pub tickets: BTreeMap<String, ProgressSummary>,
    pub detail: Option<ProgressLog>,
}

struct Text<'a>(Cow<'a, str>);

impl<'de: 'a, 'a> Deserialize<'de> for Text<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TextVisitor;
        impl<'de> Visitor<'de> for TextVisitor {
            type Value = Cow<'de, str>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string")
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                Ok(Cow::Borrowed(value))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Cow::Owned(value.into()))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Cow::Owned(value))
            }
        }
        deserializer.deserialize_string(TextVisitor).map(Text)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogWire<'a> {
    schema_version: i64,
    #[serde(default, borrow)]
    entries: Vec<EntryWire<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire<'a> {
    #[serde(borrow)]
    id: Text<'a>,
    #[serde(borrow)]
    recorded_at: Text<'a>,
    #[serde(borrow)]
    recorded_by: Text<'a>,
    #[serde(borrow)]
    ticket_id: Text<'a>,
    #[serde(borrow)]
    kind: Text<'a>,
    #[serde(rename = "from")]
    from: Option<ProgressStatus>,
    to: Option<ProgressStatus>,
    category: Option<ProgressNoteCategory>,
    #[serde(borrow)]
    message: Option<Text<'a>>,
}

struct BorrowedEntry<'a> {
    id: Cow<'a, str>,
    recorded_at: Cow<'a, str>,
    recorded_by: Cow<'a, str>,
    ticket_id: Cow<'a, str>,
    change: BorrowedChange<'a>,
}

enum BorrowedChange<'a> {
    Transition {
        from: ProgressStatus,
        to: ProgressStatus,
    },
    Note {
        category: ProgressNoteCategory,
        message: Cow<'a, str>,
    },
}

impl BorrowedEntry<'_> {
    fn as_ref(&self) -> EntryRef<'_> {
        EntryRef {
            id: &self.id,
            recorded_at: &self.recorded_at,
            recorded_by: &self.recorded_by,
            ticket_id: &self.ticket_id,
            change: match &self.change {
                BorrowedChange::Transition { from, to } => ChangeRef::Transition {
                    from: *from,
                    to: *to,
                },
                BorrowedChange::Note { message, .. } => ChangeRef::Note { message },
            },
        }
    }

    fn into_owned(self) -> ProgressEntry {
        match self.change {
            BorrowedChange::Transition { from, to } => ProgressEntry::Transition {
                id: self.id.into_owned(),
                recorded_at: self.recorded_at.into_owned(),
                recorded_by: self.recorded_by.into_owned(),
                ticket_id: self.ticket_id.into_owned(),
                from,
                to,
            },
            BorrowedChange::Note { category, message } => ProgressEntry::Note {
                id: self.id.into_owned(),
                recorded_at: self.recorded_at.into_owned(),
                recorded_by: self.recorded_by.into_owned(),
                ticket_id: self.ticket_id.into_owned(),
                category,
                message: message.into_owned(),
            },
        }
    }
}

fn parse_entries<'a>(path: &str, text: &'a str) -> Result<Vec<BorrowedEntry<'a>>, Vec<Diagnostic>> {
    let wire: LogWire<'_> = toml_progress::from_str(text).map_err(|error| {
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
        let change = match wire.kind.0.as_ref() {
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
                BorrowedChange::Transition { from, to }
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
                BorrowedChange::Note {
                    category,
                    message: message.0,
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
        entries.push(BorrowedEntry {
            id: wire.id.0,
            recorded_at: wire.recorded_at.0,
            recorded_by: wire.recorded_by.0,
            ticket_id: wire.ticket_id.0,
            change,
        });
    }
    validate_entries(path, entries.iter().map(BorrowedEntry::as_ref))
        .map_err(|diagnostic| vec![diagnostic])?;
    Ok(entries)
}

pub fn parse_progress_log(path: &str, text: &str) -> Result<ProgressLog, Vec<Diagnostic>> {
    Ok(ProgressLog {
        entries: parse_entries(path, text)?
            .into_iter()
            .map(BorrowedEntry::into_owned)
            .collect(),
    })
}

pub fn parse_progress_operations(
    path: &str,
    text: &str,
) -> Result<Vec<(String, String)>, Vec<Diagnostic>> {
    Ok(parse_entries(path, text)?
        .into_iter()
        .map(|entry| (entry.id.into_owned(), entry.ticket_id.into_owned()))
        .collect())
}

pub fn parse_progress_projection(
    path: &str,
    text: &str,
    detail_ticket: Option<&str>,
) -> Result<ProgressProjection, Vec<Diagnostic>> {
    let entries = parse_entries(path, text)?;
    let mut tickets: BTreeMap<String, ProgressSummary> = BTreeMap::new();
    let mut detail = Vec::new();
    for entry in entries {
        if detail_ticket.is_none() || detail_ticket == Some(entry.ticket_id.as_ref()) {
            if !tickets.contains_key(entry.ticket_id.as_ref()) {
                tickets.insert(
                    entry.ticket_id.to_string(),
                    ProgressSummary {
                        status: ProgressStatus::Unknown,
                        note_count: 0,
                    },
                );
            }
            let summary = tickets
                .get_mut(entry.ticket_id.as_ref())
                .expect("ticket was inserted");
            match &entry.change {
                BorrowedChange::Transition { to, .. } => summary.status = *to,
                BorrowedChange::Note { .. } => summary.note_count += 1,
            }
        }
        if detail_ticket == Some(entry.ticket_id.as_ref()) {
            detail.push(entry.into_owned());
        }
    }
    Ok(ProgressProjection {
        tickets,
        detail: detail_ticket.map(|_| ProgressLog { entries: detail }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_progress_log;

    const NOTE: &str = "[[entries]]\nid='note'\nrecorded_at='2026-09-30T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='note'\ncategory='quirk'\nmessage=\"plain\"\n";

    #[test]
    fn summary_and_selected_detail_share_decoding_and_declaration_order() {
        for message in [
            r#""plain""#,
            r#""line\nquote\"""#,
            r#""\e\x41""#,
            "'''literal\nmessage'''",
            "\"\"\"\nmultiline\nmessage\"\"\"",
        ] {
            let source = format!(
                "schema_version=1\n{}\n{}\n[[entries]]\nid='move'\nrecorded_at='2026-09-30T10:01:00Z'\nrecorded_by='root'\nticket_id='HMD-001'\nkind='transition'\nfrom='unknown'\nto='in_review'\n",
                NOTE.replace("\"plain\"", message),
                NOTE.replace("id='note'", "id='other'")
                    .replace("HMD-001", "HMD-002")
            );
            let full = parse_progress_log("log.toml", &source).unwrap();
            assert_eq!(
                parse_progress_operations("log.toml", &source).unwrap(),
                full.entries
                    .iter()
                    .map(|entry| (entry.id().to_owned(), entry.ticket_id().to_owned()))
                    .collect::<Vec<_>>()
            );
            let projected =
                parse_progress_projection("log.toml", &source, Some("HMD-001")).unwrap();
            assert_eq!(
                projected.tickets["HMD-001"],
                ProgressSummary {
                    status: ProgressStatus::InReview,
                    note_count: 1
                }
            );
            let summary = parse_progress_projection("log.toml", &source, None).unwrap();
            assert_eq!(
                summary.tickets["HMD-002"],
                ProgressSummary {
                    status: ProgressStatus::Unknown,
                    note_count: 1
                }
            );
            assert_eq!(
                projected.detail.unwrap().entries,
                full.entries
                    .iter()
                    .filter(|entry| entry.ticket_id() == "HMD-001")
                    .cloned()
                    .collect::<Vec<_>>()
            );
            assert!(
                parse_progress_projection("log.toml", &source, None)
                    .unwrap()
                    .detail
                    .is_none()
            );
            assert_eq!(
                full,
                parse_progress_log("log.toml", &render_progress_log(&full)).unwrap()
            );
        }
        let source = "schema_version=1\nentries=[{\nid='inline', recorded_at='2026-09-30T10:00:00Z', recorded_by='root', ticket_id='HMD-001', kind='note', category='quirk', message='new inline',\n}]\n";
        assert_eq!(
            parse_progress_projection("log.toml", source, Some("HMD-001"))
                .unwrap()
                .detail
                .unwrap(),
            parse_progress_log("log.toml", source).unwrap()
        );
    }

    #[test]
    fn summary_cannot_hide_invalid_unselected_entries() {
        let valid = format!("schema_version=1\n{NOTE}");
        let invalid = [
            valid.replace("schema_version=1", "schema_version=2"),
            valid.replace("kind='note'", "kind='note'\nunknown='x'"),
            valid.replace("recorded_by='root'\n", ""),
            valid.replace("category='quirk'", "category='not_a_category'"),
            valid.replace("message=\"plain\"", "message=\"\\t\\n \""),
            valid.replace(
                "recorded_at='2026-09-30T10:00:00Z'",
                "recorded_at=2026-09-30T10:00:00Z",
            ),
            valid.replace("10:00:00Z", "10:00Z"),
            format!("{valid}{NOTE}"),
            valid.replace("kind='note'", "kind='note'\nfrom='unknown'"),
            valid.replace(
                "kind='note'\ncategory='quirk'\nmessage=\"plain\"",
                "kind='transition'\nfrom='in_progress'\nto='complete'",
            ),
            valid.replace(
                "kind='note'\ncategory='quirk'\nmessage=\"plain\"",
                "kind='transition'\nfrom='unknown'\nto='unknown'",
            ),
        ];
        for source in invalid {
            let full = parse_progress_log("log.toml", &source).unwrap_err();
            assert_eq!(
                full,
                parse_progress_operations("log.toml", &source).unwrap_err()
            );
            assert_eq!(
                full,
                parse_progress_projection("log.toml", &source, Some("OTHER")).unwrap_err()
            );
        }
    }
}
