use casefile_core::Kind;

use crate::{activation::Activation, store::StoreError};

pub(super) fn checked_path(path: &str) -> Result<String, StoreError> {
    normalize_planning_relative(path)
        .map_err(|_| StoreError::Invalid("path must be contained and relative".into()))
}

pub(super) fn safe_relative(path: &str) -> bool {
    normalize_planning_relative(path).is_ok_and(|canonical| canonical == path)
}

pub fn normalize_planning_relative(path: &str) -> Result<String, &'static str> {
    if path.is_empty() {
        return Err("must be a non-empty relative path");
    }
    if path.starts_with(['/', '\\'])
        || path.as_bytes().get(1).is_some_and(|byte| *byte == b':')
            && path.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err("must be a contained relative path");
    }
    let segments = path
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| matches!(*segment, "." | "..") || segment.contains('\0'))
    {
        return Err("must contain only normal path segments");
    }
    Ok(segments.join("/"))
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
        ["strategy", "transitions", name] if name.ends_with(".toml") && name.contains('-') => {
            Some(Kind::StrategyTransition)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_planning_relative_grammar_is_host_independent() {
        for (input, expected) in [
            ("projects/demo", "projects/demo"),
            (r"projects\demo", "projects/demo"),
            ("projects//demo///tickets/", "projects/demo/tickets"),
            (r"projects\\demo\\tickets\\", "projects/demo/tickets"),
        ] {
            assert_eq!(normalize_planning_relative(input), Ok(expected.into()));
        }
        for input in [
            "",
            "///",
            r"\\\\",
            "/projects/demo",
            r"\projects\demo",
            "C:/projects/demo",
            r"C:\projects\demo",
            "C:projects/demo",
            r"\\server\share\demo",
            r"\\?\C:\projects\demo",
            r"\\.\C:\projects\demo",
            "projects/./demo",
            "projects/../demo",
            "projects/demo\0ticket",
        ] {
            assert!(
                normalize_planning_relative(input).is_err(),
                "unexpectedly accepted {input:?}"
            );
        }
    }

    #[test]
    fn persisted_paths_remain_strictly_slash_canonical() {
        assert!(safe_relative("projects/demo"));
        for input in [
            r"projects\demo",
            "projects//demo",
            "projects/demo/",
            "C:demo",
        ] {
            assert!(!safe_relative(input), "unexpectedly accepted {input:?}");
        }
    }
}
