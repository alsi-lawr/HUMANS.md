//! Pure v1 Casefile records, diagnostics, and whole-record renderers.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub schema_version: u32,
    pub code: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    pub message: String,
}

impl Diagnostic {
    pub fn new(path: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            code: code.into(),
            path: path.into(),
            field: None,
            section: None,
            message: message.into(),
        }
    }
    pub fn field(mut self, field: &str) -> Self {
        self.field = Some(field.into());
        self
    }
    pub fn section(mut self, section: &str) -> Self {
        self.section = Some(section.into());
        self
    }
}

pub fn stable(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|a, b| {
        (&a.path, &a.code, &a.field, &a.section, &a.message)
            .cmp(&(&b.path, &b.code, &b.field, &b.section, &b.message))
    });
    diagnostics
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Governed,
    Ungoverned,
    Invalid,
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Activation,
    ProjectMap,
    Request,
    Decision,
    Evidence,
    Review,
    Plan,
    Closeout,
    Strategy,
    Ticket,
    Epic,
    Board,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CasefileSnapshot {
    pub revision: Revision,
    pub entries: Vec<EntrySnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntrySnapshot {
    pub path: String,
    pub classification: Classification,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    pub content_revision: Revision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RecordSummary>,
    pub original_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecordSummary {
    Activation {
        projects: Vec<String>,
    },
    ProjectMap {
        projects: Vec<String>,
    },
    Markdown {
        title: String,
    },
    Strategy {
        strategy_id: String,
        phase: String,
        adapter: String,
    },
    WorkItem {
        id: String,
        title: String,
        status: String,
        rank: Option<u64>,
    },
    Board {
        id: String,
        title: String,
        columns: Vec<String>,
    },
}

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
                Some(render_draft(path, draft))
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordDraft {
    Ticket(WorkItemDraft),
    Epic(WorkItemDraft),
    Board(BoardDraft),
}

impl RecordDraft {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Ticket(_) => Kind::Ticket,
            Self::Epic(_) => Kind::Epic,
            Self::Board(_) => Kind::Board,
        }
    }
    pub fn identity(&self) -> &str {
        match self {
            Self::Ticket(d) | Self::Epic(d) => &d.id,
            Self::Board(d) => &d.id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkItemDraft {
    pub id: String,
    pub title: String,
    pub project: String,
    pub investigation: String,
    pub status: String,
    pub reported_by_role: String,
    pub reported_by_agent: String,
    pub source_commit: String,
    pub created_at: String,
    pub updated_at: String,
    pub confidence: String,
    pub decision_refs: Vec<String>,
    pub related_tickets: Vec<String>,
    pub supersedes: Vec<String>,
    pub superseded_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u64>,
    pub requirement_and_evidence: String,
    pub impact: String,
    pub resolution_boundary: String,
    pub acceptance_criteria: String,
    pub verification: String,
    pub relationships_and_duplicate_analysis: String,
    pub review_and_disposition_history: String,
}

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkItemWire {
    id: String,
    title: String,
    project: String,
    investigation: String,
    status: String,
    reported_by_role: String,
    reported_by_agent: String,
    source_commit: String,
    created_at: String,
    updated_at: String,
    confidence: String,
    decision_refs: Vec<String>,
    related_tickets: Vec<String>,
    supersedes: Vec<String>,
    superseded_by: Vec<String>,
    rank: Option<u64>,
}

const SECTIONS: [&str; 7] = [
    "Requirement and evidence",
    "Impact",
    "Resolution boundary",
    "Acceptance criteria",
    "Verification",
    "Relationships and duplicate analysis",
    "Review and disposition history",
];

pub fn parse_draft(path: &str, kind: Kind, text: &str) -> Result<RecordDraft, Vec<Diagnostic>> {
    match kind {
        Kind::Ticket | Kind::Epic => parse_work_item(path, kind, text),
        Kind::Board => parse_board(path, text),
        _ => Err(vec![Diagnostic::new(
            path,
            "read_only_kind",
            "only ticket, epic, and board records are writable",
        )]),
    }
}

#[allow(clippy::result_large_err)]
pub fn render_draft(path: &str, draft: &RecordDraft) -> Result<Vec<u8>, Diagnostic> {
    validate_draft(path, draft)?;
    let rendered = match draft {
        RecordDraft::Ticket(item) | RecordDraft::Epic(item) => render_work_item(item),
        RecordDraft::Board(board) => render_board(board),
    };
    let parsed = parse_draft(path, draft.kind(), &rendered).map_err(|errors| {
        errors.into_iter().next().unwrap_or_else(|| {
            Diagnostic::new(path, "render_invalid", "rendered record did not validate")
        })
    })?;
    if &parsed != draft {
        return Err(Diagnostic::new(
            path,
            "render_round_trip",
            "rendered record did not round-trip",
        ));
    }
    Ok(rendered.into_bytes())
}

#[allow(clippy::result_large_err)]
pub fn validate_draft(path: &str, draft: &RecordDraft) -> Result<(), Diagnostic> {
    let kind = draft.kind();
    let id = draft.identity();
    match kind {
        Kind::Ticket | Kind::Epic => {
            let item = match draft {
                RecordDraft::Ticket(item) | RecordDraft::Epic(item) => item,
                _ => unreachable!(),
            };
            let pattern = if kind == Kind::Ticket {
                r"^[A-Z][A-Z0-9_]*-[0-9]{3,}$"
            } else {
                r"^[A-Z][A-Z0-9_]*-E-[0-9]{3,}$"
            };
            if !Regex::new(pattern).expect("fixed regex").is_match(id) {
                return Err(Diagnostic::new(
                    path,
                    "invalid_identity",
                    "ID does not have the required project-prefix syntax",
                )
                .field("id"));
            }
            if path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".md"))
                != Some(id)
            {
                return Err(Diagnostic::new(
                    path,
                    "filename_identity",
                    "filename stem must equal ID",
                )
                .field("id"));
            }
            let status_dir = path.split('/').rev().nth(1);
            if status_dir != Some(item.status.as_str()) {
                return Err(Diagnostic::new(
                    path,
                    "status_placement",
                    "status must match containing directory",
                )
                .field("status"));
            }
            for (name, value) in [
                ("title", &item.title),
                ("project", &item.project),
                ("investigation", &item.investigation),
                ("reported_by_role", &item.reported_by_role),
                ("reported_by_agent", &item.reported_by_agent),
                ("source_commit", &item.source_commit),
            ] {
                if value.trim().is_empty() {
                    return Err(Diagnostic::new(
                        path,
                        "empty_field",
                        "required string must be non-empty",
                    )
                    .field(name));
                }
            }
            if !matches!(
                item.status.as_str(),
                "provisional" | "accepted" | "rejected"
            ) {
                return Err(Diagnostic::new(
                    path,
                    "invalid_status",
                    "status must be provisional, accepted, or rejected",
                )
                .field("status"));
            }
            if !matches!(item.confidence.as_str(), "low" | "medium" | "high") {
                return Err(Diagnostic::new(
                    path,
                    "invalid_confidence",
                    "confidence must be low, medium, or high",
                )
                .field("confidence"));
            }
            for (name, value) in [
                ("created_at", &item.created_at),
                ("updated_at", &item.updated_at),
            ] {
                if OffsetDateTime::parse(value, &Rfc3339).is_err() {
                    return Err(Diagnostic::new(
                        path,
                        "invalid_timestamp",
                        "timestamp must be RFC 3339",
                    )
                    .field(name));
                }
            }
        }
        Kind::Board => {
            let board = match draft {
                RecordDraft::Board(board) => board,
                _ => unreachable!(),
            };
            if board.id.trim().is_empty()
                || board.title.trim().is_empty()
                || board.columns.is_empty()
            {
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
        }
        _ => {
            return Err(Diagnostic::new(
                path,
                "read_only_kind",
                "only ticket, epic, and board records are writable",
            ));
        }
    }
    Ok(())
}

fn parse_work_item(path: &str, kind: Kind, text: &str) -> Result<RecordDraft, Vec<Diagnostic>> {
    let (frontmatter, body) = split_frontmatter(path, text)?;
    let wire: WorkItemWire = serde_saphyr::from_str(frontmatter).map_err(|error| {
        vec![Diagnostic::new(
            path,
            "invalid_frontmatter",
            error.to_string(),
        )]
    })?;
    let sections = required_sections(path, body)?;
    let item = WorkItemDraft {
        id: wire.id,
        title: wire.title,
        project: wire.project,
        investigation: wire.investigation,
        status: wire.status,
        reported_by_role: wire.reported_by_role,
        reported_by_agent: wire.reported_by_agent,
        source_commit: wire.source_commit,
        created_at: wire.created_at,
        updated_at: wire.updated_at,
        confidence: wire.confidence,
        decision_refs: wire.decision_refs,
        related_tickets: wire.related_tickets,
        supersedes: wire.supersedes,
        superseded_by: wire.superseded_by,
        rank: wire.rank,
        requirement_and_evidence: sections[0].clone(),
        impact: sections[1].clone(),
        resolution_boundary: sections[2].clone(),
        acceptance_criteria: sections[3].clone(),
        verification: sections[4].clone(),
        relationships_and_duplicate_analysis: sections[5].clone(),
        review_and_disposition_history: sections[6].clone(),
    };
    let (h1, _) = markdown_headings(path, body).map_err(|diagnostic| vec![diagnostic])?;
    if h1[0] != item.id {
        return Err(vec![Diagnostic::new(
            path,
            "identity_heading",
            "H1 must equal the work-item ID",
        )]);
    }
    let draft = if kind == Kind::Ticket {
        RecordDraft::Ticket(item)
    } else {
        RecordDraft::Epic(item)
    };
    validate_draft(path, &draft).map_err(|diagnostic| vec![diagnostic])?;
    Ok(draft)
}

#[allow(clippy::result_large_err)]
fn parse_board(path: &str, text: &str) -> Result<RecordDraft, Vec<Diagnostic>> {
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
    let draft = RecordDraft::Board(BoardDraft {
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
    });
    validate_draft(path, &draft).map_err(|diagnostic| vec![diagnostic])?;
    Ok(draft)
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

#[allow(clippy::result_large_err)]
pub fn markdown_headings(path: &str, text: &str) -> Result<(Vec<String>, Vec<String>), Diagnostic> {
    let mut h1 = Vec::new();
    let mut h2 = Vec::new();
    let mut level = None;
    let mut current = String::new();
    for event in Parser::new_ext(text, Options::all()) {
        match event {
            Event::Start(Tag::Heading { level: heading, .. }) => {
                level = Some(heading);
                current.clear();
            }
            Event::Text(value) | Event::Code(value) if level.is_some() => current.push_str(&value),
            Event::End(TagEnd::Heading(_)) => match level.take() {
                Some(pulldown_cmark::HeadingLevel::H1) => h1.push(current.trim().into()),
                Some(pulldown_cmark::HeadingLevel::H2) => h2.push(current.trim().into()),
                _ => {}
            },
            _ => {}
        }
    }
    if h1.len() != 1 {
        return Err(Diagnostic::new(
            path,
            "h1_count",
            "Markdown record must contain exactly one H1",
        ));
    }
    Ok((h1, h2))
}

pub fn validate_markdown(
    path: &str,
    text: &str,
    required_h2: &[&str],
    title_contains: Option<&str>,
) -> Result<RecordSummary, Vec<Diagnostic>> {
    let (mut h1, h2) = markdown_headings(path, text).map_err(|diagnostic| vec![diagnostic])?;
    if title_contains.is_some_and(|value| !h1[0].contains(value)) {
        return Err(vec![Diagnostic::new(
            path,
            "identity_heading",
            "H1 must contain the record ID",
        )]);
    }
    for expected in required_h2 {
        if !h2.iter().any(|actual| actual == expected) {
            return Err(vec![
                Diagnostic::new(path, "missing_section", "required H2 is missing")
                    .section(expected),
            ]);
        }
    }
    Ok(RecordSummary::Markdown {
        title: h1.remove(0),
    })
}

fn split_frontmatter<'a>(path: &str, text: &'a str) -> Result<(&'a str, &'a str), Vec<Diagnostic>> {
    let rest = text.strip_prefix("---\n").ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "missing_frontmatter",
            "work item needs YAML frontmatter",
        )]
    })?;
    let (frontmatter, body) = rest.split_once("\n---\n").ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_frontmatter",
            "frontmatter closing delimiter is missing",
        )]
    })?;
    Ok((frontmatter, body))
}

fn required_sections(path: &str, body: &str) -> Result<Vec<String>, Vec<Diagnostic>> {
    let (_, headings) = markdown_headings(path, body).map_err(|diagnostic| vec![diagnostic])?;
    if headings != SECTIONS {
        return Err(vec![Diagnostic::new(
            path,
            "work_item_sections",
            "required H2 headings must occur exactly once and in order",
        )]);
    }
    let mut values = Vec::new();
    for (index, heading) in SECTIONS.iter().enumerate() {
        let marker = format!("## {heading}");
        let start = body.find(&marker).expect("heading parsed") + marker.len();
        let end = SECTIONS
            .get(index + 1)
            .and_then(|next| body.find(&format!("## {next}")))
            .unwrap_or(body.len());
        values.push(body[start..end].trim().to_owned());
    }
    Ok(values)
}

fn render_work_item(item: &WorkItemDraft) -> String {
    let optional_rank = item
        .rank
        .map(|rank| format!("rank: {rank}\n"))
        .unwrap_or_default();
    format!(
        "---\nid: {}\ntitle: {}\nproject: {}\ninvestigation: {}\nstatus: {}\nreported_by_role: {}\nreported_by_agent: {}\nsource_commit: {}\ncreated_at: {}\nupdated_at: {}\nconfidence: {}\ndecision_refs: {}\nrelated_tickets: {}\nsupersedes: {}\nsuperseded_by: {}\n{}---\n\n# {}\n\n## Requirement and evidence\n\n{}\n\n## Impact\n\n{}\n\n## Resolution boundary\n\n{}\n\n## Acceptance criteria\n\n{}\n\n## Verification\n\n{}\n\n## Relationships and duplicate analysis\n\n{}\n\n## Review and disposition history\n\n{}\n",
        item.id,
        yaml_string(&item.title),
        yaml_string(&item.project),
        yaml_string(&item.investigation),
        item.status,
        yaml_string(&item.reported_by_role),
        yaml_string(&item.reported_by_agent),
        yaml_string(&item.source_commit),
        item.created_at,
        item.updated_at,
        item.confidence,
        yaml_list(&item.decision_refs),
        yaml_list(&item.related_tickets),
        yaml_list(&item.supersedes),
        yaml_list(&item.superseded_by),
        optional_rank,
        item.id,
        item.requirement_and_evidence,
        item.impact,
        item.resolution_boundary,
        item.acceptance_criteria,
        item.verification,
        item.relationships_and_duplicate_analysis,
        item.review_and_disposition_history
    )
}

fn yaml_string(value: &str) -> String {
    serde_saphyr::to_string(&value)
        .expect("strings serialize")
        .trim()
        .to_owned()
}
fn yaml_list(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| yaml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn render_board(board: &BoardDraft) -> String {
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

fn toml_string(value: &str) -> String {
    toml::Value::String(value.into()).to_string()
}
fn toml_list(values: &[String]) -> String {
    toml::Value::Array(values.iter().cloned().map(toml::Value::String).collect()).to_string()
}
