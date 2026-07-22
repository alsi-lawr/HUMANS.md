use casefile_core::{Classification, EntrySnapshot, Kind};

/// A request to edit one supported governed record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditIntent {
    pub path: String,
    pub kind: Kind,
}

/// The result of one workbench interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Interaction {
    Quit,
    Edit(EditIntent),
}

pub(crate) fn edit_selection(entry: Option<&EntrySnapshot>) -> Result<Interaction, &'static str> {
    let Some(entry) = entry else {
        return Err("No selected record to edit.");
    };
    if entry.classification == Classification::Governed && entry.kind.is_some_and(Kind::is_writable)
    {
        Ok(Interaction::Edit(EditIntent {
            path: entry.path.clone(),
            kind: entry.kind.expect("matched writable kind"),
        }))
    } else {
        Err("Read-only: e edits governed tickets, epics, and boards only.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::entry;
    use casefile_core::{Classification, Kind};

    #[test]
    fn edit_selection_yields_only_supported_governed_records() {
        let ticket = entry(
            "a-ticket.md",
            Classification::Governed,
            Some(Kind::Ticket),
            None,
            b"ticket",
        );
        assert_eq!(
            edit_selection(Some(&ticket)),
            Ok(Interaction::Edit(EditIntent {
                path: "a-ticket.md".into(),
                kind: Kind::Ticket,
            }))
        );

        let raw = entry("raw.txt", Classification::Raw, None, None, b"raw");
        assert_eq!(
            edit_selection(Some(&raw)),
            Err("Read-only: e edits governed tickets, epics, and boards only.")
        );
        assert_eq!(edit_selection(None), Err("No selected record to edit."));
    }
}
