use super::*;

pub(crate) fn classify(
    path: &str,
    bytes: &[u8],
    active: &Activation,
) -> (
    Classification,
    Option<Kind>,
    Option<String>,
    Option<RecordSummary>,
    Vec<Diagnostic>,
) {
    let classified = classify_facts(path, bytes, active, kind_for_path(path, active));
    classified.classification
}

#[derive(Default)]
pub(crate) struct ParsedFacts {
    pub draft: Option<RecordDraft>,
    pub metadata: Option<(Vec<String>, Vec<String>)>,
    pub progress: Option<casefile_core::ProgressLog>,
    pub strategy: Option<casefile_core::StrategyProjection>,
}

pub(crate) struct Classified {
    pub classification: (
        Classification,
        Option<Kind>,
        Option<String>,
        Option<RecordSummary>,
        Vec<Diagnostic>,
    ),
    pub facts: ParsedFacts,
}

pub(crate) fn classify_facts(
    path: &str,
    bytes: &[u8],
    active: &Activation,
    expected_kind: Option<Kind>,
) -> Classified {
    let mut facts = ParsedFacts::default();
    let classification = classify_parsed(path, bytes, active, expected_kind, &mut facts);
    Classified {
        classification,
        facts,
    }
}

fn classify_parsed(
    path: &str,
    bytes: &[u8],
    active: &Activation,
    expected_kind: Option<Kind>,
    facts: &mut ParsedFacts,
) -> (
    Classification,
    Option<Kind>,
    Option<String>,
    Option<RecordSummary>,
    Vec<Diagnostic>,
) {
    if path == "casefile.toml" {
        return activation_entry(path, bytes, active);
    }
    if path == "projects.toml" {
        return project_map_entry(path, bytes, active);
    }
    let Some(kind) = expected_kind else {
        return (
            if in_active(path, active) {
                Classification::Raw
            } else {
                Classification::Ungoverned
            },
            None,
            None,
            None,
            Vec::new(),
        );
    };
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            return invalid(
                path,
                Some(kind),
                "invalid_utf8",
                "governed text must be UTF-8",
            );
        }
    };
    let result = match kind {
        Kind::Ticket | Kind::Epic | Kind::Board => casefile_core::parse_draft(path, kind, text)
            .map(|draft| {
                let summary = match &draft {
                    RecordDraft::Ticket(item) | RecordDraft::Epic(item) => (
                        Some(item.id.clone()),
                        Some(RecordSummary::WorkItem {
                            id: item.id.clone(),
                            title: item.title.clone(),
                            status: item.status.clone(),
                            rank: item.rank,
                        }),
                    ),
                    RecordDraft::Board(board) => (
                        Some(board.id.clone()),
                        Some(RecordSummary::Board {
                            id: board.id.clone(),
                            title: board.title.clone(),
                            columns: board
                                .columns
                                .iter()
                                .map(|column| column.name.clone())
                                .collect(),
                        }),
                    ),
                };
                facts.draft = Some(draft);
                summary
            }),
        Kind::Request => parse_request(path, text).map(|summary| (None, Some(summary))),
        Kind::Decision => parse_decision(path, text),
        Kind::Evidence | Kind::Review => casefile_core::validate_markdown(path, text, &[], None)
            .and_then(|summary| {
                parse_metadata_arrays(path, text).map(|metadata| {
                    facts.metadata = Some(metadata);
                    summary
                })
            })
            .map(|summary| (None, Some(summary))),
        Kind::Plan => casefile_core::validate_markdown(path, text, &["Objective"], None)
            .map(|summary| (None, Some(summary))),
        Kind::Closeout => {
            casefile_core::validate_markdown(path, text, &["Scope disposition"], None)
                .map(|summary| (None, Some(summary)))
        }
        Kind::Strategy => casefile_core::parse_strategy_with_projection(path, text).map(
            |(summary, projection)| {
                facts.strategy = projection;
                (None, Some(summary))
            },
        ),
        Kind::StrategyBinding => {
            parse_strategy_binding(path, text).map(|summary| (None, Some(summary)))
        }
        Kind::StrategyTransition => {
            let value = match toml::from_str::<toml::Value>(text) {
                Ok(value) => value,
                Err(error) => {
                    return (
                        Classification::Invalid,
                        Some(kind),
                        None,
                        None,
                        vec![Diagnostic::new(
                            path,
                            "invalid_strategy_transition",
                            error.to_string(),
                        )],
                    );
                }
            };
            if is_legacy_strategy_transition(&value) {
                return (Classification::Raw, None, None, None, Vec::new());
            }
            casefile_core::parse_strategy_transition_value(path, value).map(|record| {
                (
                    Some(format!(
                        "strategy-transition:{path}:{}",
                        record.operation_id
                    )),
                    Some(RecordSummary::StrategyTransition {
                        record: Box::new(record),
                    }),
                )
            })
        }
        Kind::Progress => parse_progress_log(path, text).map(|log| {
            facts.progress = Some(log);
            (None, Some(RecordSummary::Progress))
        }),
        Kind::Activation | Kind::ProjectMap => unreachable!(),
    };
    match result {
        Ok((identity, summary)) => (
            Classification::Governed,
            Some(kind),
            identity,
            summary,
            Vec::new(),
        ),
        Err(diagnostics) => (Classification::Invalid, Some(kind), None, None, diagnostics),
    }
}

fn is_legacy_strategy_transition(value: &toml::Value) -> bool {
    value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        == Some(1)
        && value.get("operation_id").is_none()
        && value
            .get("timestamp")
            .and_then(toml::Value::as_str)
            .is_some()
        && value.get("mode").and_then(toml::Value::as_str).is_some()
        && value
            .get("selected_matrix")
            .and_then(toml::Value::as_str)
            .is_some()
        && value
            .get("backup_path")
            .and_then(toml::Value::as_str)
            .is_some()
}

fn project_map_entry(
    path: &str,
    bytes: &[u8],
    active: &Activation,
) -> (
    Classification,
    Option<Kind>,
    Option<String>,
    Option<RecordSummary>,
    Vec<Diagnostic>,
) {
    let governed_projects = active
        .projects
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    match parse_project_map(path, bytes, &governed_projects) {
        Ok(summary) => (
            Classification::Governed,
            Some(Kind::ProjectMap),
            None,
            Some(summary),
            Vec::new(),
        ),
        Err(diagnostics) => (
            Classification::Invalid,
            Some(Kind::ProjectMap),
            None,
            None,
            diagnostics,
        ),
    }
}

pub(crate) fn invalid(
    path: &str,
    kind: Option<Kind>,
    code: &str,
    message: &str,
) -> (
    Classification,
    Option<Kind>,
    Option<String>,
    Option<RecordSummary>,
    Vec<Diagnostic>,
) {
    (
        Classification::Invalid,
        kind,
        None,
        None,
        vec![Diagnostic::new(path, code, message)],
    )
}
