use crate::{SCHEMA_VERSION, diagnostic::Diagnostic, record::RecordSummary};

pub fn parse(path: &str, text: &str) -> Result<RecordSummary, Vec<Diagnostic>> {
    let value: toml::Value = toml::from_str(text)
        .map_err(|error| vec![Diagnostic::new(path, "invalid_toml", error.to_string())])?;
    let table = value.as_table().ok_or_else(|| {
        vec![Diagnostic::new(
            path,
            "invalid_strategy",
            "strategy must be a TOML table",
        )]
    })?;
    let phase = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".toml"))
        .unwrap_or_default();
    let get = |name| {
        table
            .get(name)
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                vec![
                    Diagnostic::new(
                        path,
                        "invalid_strategy",
                        "strategy fields must be non-empty",
                    )
                    .field(name),
                ]
            })
    };
    if table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(i64::from(SCHEMA_VERSION))
    {
        return Err(vec![Diagnostic::new(
            path,
            "invalid_schema_version",
            "schema_version must be 1",
        )]);
    }
    let parsed_phase = get("phase")?;
    if parsed_phase != phase {
        return Err(vec![
            Diagnostic::new(path, "strategy_phase", "phase must match filename").field("phase"),
        ]);
    }
    Ok(RecordSummary::Strategy {
        strategy_id: get("strategy_id")?,
        phase: parsed_phase,
        adapter: get("adapter")?,
    })
}
