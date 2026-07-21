use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    diagnostic::{Diagnostic, SCHEMA_VERSION},
    record::RecordDraft,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BoardDraft {
    pub id: String,
    pub title: String,
    pub filter_statuses: Option<Vec<String>>,
    pub filter_kinds: Option<Vec<String>>,
    pub columns: Vec<BoardColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BoardColumn {
    pub name: String,
    pub statuses: Vec<String>,
}

#[allow(clippy::result_large_err)]
pub(crate) fn parse(path: &str, text: &str) -> Result<RecordDraft, Vec<Diagnostic>> {
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = value.as_table().ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_board",
            "board must be a TOML table",
        )]
    })?;
    let schema_ok = table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        == Some(i64::from(SCHEMA_VERSION));
    let columns = table
        .get("columns")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            vec![Diagnostic::new(
                path,
                "missing_columns",
                "board needs one or more columns",
            )]
        })?
        .iter()
        .map(|column| {
            let item = column.as_table().ok_or_else(|| {
                Diagnostic::new(path, "invalid_board_column", "column must be a table")
            })?;
            Ok(BoardColumn {
                name: item
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                statuses: strings(item.get("statuses")).ok_or_else(|| {
                    Diagnostic::new(
                        path,
                        "invalid_board_column",
                        "column statuses must be strings",
                    )
                })?,
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()
        .map_err(|diagnostic| vec![diagnostic])?;
    if !schema_ok {
        return Err(vec![
            Diagnostic::new(path, "invalid_schema_version", "schema_version must be 1")
                .field("schema_version"),
        ]);
    }
    for name in ["filter_statuses", "filter_kinds"] {
        if table.contains_key(name) && strings(table.get(name)).is_none() {
            return Err(vec![
                Diagnostic::new(
                    path,
                    "invalid_board_filter",
                    "board filters must be string arrays",
                )
                .field(name),
            ]);
        }
    }
    let board = BoardDraft {
        id: table
            .get("id")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .into(),
        title: table
            .get("title")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .into(),
        filter_statuses: strings(table.get("filter_statuses")),
        filter_kinds: strings(table.get("filter_kinds")),
        columns,
    };
    validate(path, &board).map_err(|diagnostic| vec![diagnostic])?;
    Ok(RecordDraft::Board(board))
}

#[allow(clippy::result_large_err)]
pub(crate) fn validate(path: &str, board: &BoardDraft) -> Result<(), Diagnostic> {
    if board.id.trim().is_empty() || board.title.trim().is_empty() || board.columns.is_empty() {
        return Err(Diagnostic::new(
            path,
            "invalid_board",
            "board ID, title, and at least one column are required",
        ));
    }
    let mut names = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    for column in &board.columns {
        if column.name.trim().is_empty()
            || column.statuses.is_empty()
            || !names.insert(&column.name)
        {
            return Err(Diagnostic::new(
                path,
                "invalid_board_column",
                "columns need unique names and statuses",
            ));
        }
        for status in &column.statuses {
            if !statuses.insert(status) {
                return Err(Diagnostic::new(
                    path,
                    "overlapping_board_status",
                    "column statuses must not overlap",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn render(board: &BoardDraft) -> String {
    let mut output = format!(
        "schema_version = 1\nid = {}\ntitle = {}\n",
        toml_string(&board.id),
        toml_string(&board.title)
    );
    if let Some(values) = &board.filter_statuses {
        output.push_str(&format!("filter_statuses = {}\n", toml_list(values)));
    }
    if let Some(values) = &board.filter_kinds {
        output.push_str(&format!("filter_kinds = {}\n", toml_list(values)));
    }
    for column in &board.columns {
        output.push_str(&format!(
            "\n[[columns]]\nname = {}\nstatuses = {}\n",
            toml_string(&column.name),
            toml_list(&column.statuses)
        ));
    }
    output
}

fn strings(value: Option<&toml::Value>) -> Option<Vec<String>> {
    value.and_then(toml::Value::as_array).and_then(|items| {
        items
            .iter()
            .map(toml::Value::as_str)
            .collect::<Option<Vec<_>>>()
            .map(|items| items.into_iter().map(str::to_owned).collect())
    })
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.into()).to_string()
}

fn toml_list(values: &[String]) -> String {
    toml::Value::Array(values.iter().cloned().map(toml::Value::String).collect()).to_string()
}
