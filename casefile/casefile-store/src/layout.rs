use std::path::{Component, Path};

use casefile_core::Kind;

use crate::{activation::Activation, store::StoreError};

pub(super) fn checked_path(path: &str) -> Result<String, StoreError> {
    if !safe_relative(path) {
        return Err(StoreError::Invalid(
            "path must be a contained relative path".into(),
        ));
    }
    Ok(path.into())
}

pub(super) fn safe_relative(path: &str) -> bool {
    let value = Path::new(path);
    !value.is_absolute()
        && !path.is_empty()
        && value
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

pub(super) fn kind_for_path(path: &str, active: &Activation) -> Option<Kind> {
    if active.projects.keys().any(|slug| {
        path.strip_prefix(&format!("projects/{slug}/decision-log/"))
            .is_some_and(|name| name.ends_with(".md") && name.contains('-'))
    }) {
        return Some(Kind::Decision);
    }
    let (_, rest) = active
        .projects
        .values()
        .flat_map(|project| {
            project
                .investigations
                .iter()
                .map(move |base| (project, base))
        })
        .filter_map(|(project, base)| {
            path.strip_prefix(&format!("{base}/"))
                .map(|rest| (project, base, rest))
        })
        .max_by_key(|(_, base, _)| base.len())
        .map(|(project, _, rest)| (project, rest))?;
    let segments: Vec<_> = rest.split('/').collect();
    match segments.as_slice() {
        ["request.md"] => Some(Kind::Request),
        ["final-disposition.md"] => Some(Kind::Closeout),
        ["implementation-plan", "PLAN.md"] => Some(Kind::Plan),
        ["strategy", "bindings.toml"] => Some(Kind::StrategyBinding),
        ["strategy", name]
            if matches!(
                *name,
                "investigation.toml" | "review.toml" | "implementation.toml"
            ) =>
        {
            Some(Kind::Strategy)
        }
        ["decision-log", name] if name.ends_with(".md") && name.contains('-') => {
            Some(Kind::Decision)
        }
        ["evidence", name] if name.ends_with(".md") => Some(Kind::Evidence),
        ["review", .., name] if name.ends_with(".md") => Some(Kind::Review),
        [
            "tickets" | "epics",
            "provisional" | "accepted" | "rejected",
            name,
        ] if name.ends_with(".md") => Some(if segments[0] == "tickets" {
            Kind::Ticket
        } else {
            Kind::Epic
        }),
        ["boards", name] if name.ends_with(".toml") => Some(Kind::Board),
        ["progress", "log.toml"] => Some(Kind::Progress),
        _ => None,
    }
}
