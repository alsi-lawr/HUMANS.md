use regex::Regex;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    diagnostic::Diagnostic,
    markdown::markdown_headings,
    record::{Kind, RecordDraft},
};

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

pub(crate) fn parse(path: &str, kind: Kind, text: &str) -> Result<RecordDraft, Vec<Diagnostic>> {
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
    validate(path, kind, &item).map_err(|diagnostic| vec![diagnostic])?;
    Ok(if kind == Kind::Ticket {
        RecordDraft::Ticket(item)
    } else {
        RecordDraft::Epic(item)
    })
}

#[allow(clippy::result_large_err)]
pub(crate) fn validate(path: &str, kind: Kind, item: &WorkItemDraft) -> Result<(), Diagnostic> {
    let pattern = if kind == Kind::Ticket {
        r"^[A-Z][A-Z0-9_]*-[0-9]{3,}$"
    } else {
        r"^[A-Z][A-Z0-9_]*-E-[0-9]{3,}$"
    };
    if !Regex::new(pattern).expect("fixed regex").is_match(&item.id) {
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
        != Some(&item.id)
    {
        return Err(
            Diagnostic::new(path, "filename_identity", "filename stem must equal ID").field("id"),
        );
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
            return Err(
                Diagnostic::new(path, "empty_field", "required string must be non-empty")
                    .field(name),
            );
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
            return Err(
                Diagnostic::new(path, "invalid_timestamp", "timestamp must be RFC 3339")
                    .field(name),
            );
        }
    }
    Ok(())
}

pub(crate) fn render(item: &WorkItemDraft) -> String {
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
