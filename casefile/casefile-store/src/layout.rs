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
            .any(|segment| matches!(*segment, "." | "..") || !portable_segment(segment))
    {
        return Err("must contain only normal path segments");
    }
    Ok(segments.join("/"))
}

fn portable_segment(segment: &str) -> bool {
    if segment.ends_with([' ', '.'])
        || segment.bytes().any(|byte| {
            byte <= 0x1f || matches!(byte, b'"' | b'*' | b':' | b'<' | b'>' | b'?' | b'|')
        })
    {
        return false;
    }

    let basename = segment
        .split_once('.')
        .map_or(segment, |(basename, _)| basename)
        .to_ascii_uppercase();
    if matches!(
        basename.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) {
        return false;
    }
    !["COM", "LPT"].iter().any(|prefix| {
        basename.strip_prefix(prefix).is_some_and(|port| {
            matches!(
                port,
                "1" | "2"
                    | "3"
                    | "4"
                    | "5"
                    | "6"
                    | "7"
                    | "8"
                    | "9"
                    | "\u{b9}"
                    | "\u{b2}"
                    | "\u{b3}"
            )
        })
    })
}

pub(super) fn kind_for_path(path: &str, active: &Activation) -> Option<Kind> {
    if let Some(project) = crate::activation::project_for(path, active)
        && project_decision(path, project)
    {
        return Some(Kind::Decision);
    }
    kind_in_scope(path, crate::activation::scope_for(path, active)?)
}

pub(super) fn project_decision(path: &str, project: &str) -> bool {
    path.strip_prefix("projects/")
        .and_then(|rest| rest.strip_prefix(project))
        .and_then(|rest| rest.strip_prefix("/decision-log/"))
        .is_some_and(|name| !name.contains('/') && name.ends_with(".md") && name.contains('-'))
}

pub(super) fn kind_in_scope(path: &str, scope: &str) -> Option<Kind> {
    let rest = path.strip_prefix(scope)?.strip_prefix('/')?;
    kind_from_components(rest.split('/'))
}

pub(super) fn kind_in_native_relative_path(path: &str) -> Option<Kind> {
    kind_from_components(path.split(['/', std::path::MAIN_SEPARATOR]))
}

fn kind_from_components<'a>(
    mut components: impl DoubleEndedIterator<Item = &'a str> + Clone,
) -> Option<Kind> {
    let mut tail = components.clone();
    let first = components.next()?;
    let second = components.next();
    let third = components.next();
    let fourth = components.next();
    match (first, second, third, fourth) {
        ("request.md", None, _, _) => Some(Kind::Request),
        ("final-disposition.md", None, _, _) => Some(Kind::Closeout),
        ("implementation-plan", Some("PLAN.md"), None, _) => Some(Kind::Plan),
        ("strategy", Some("bindings.toml"), None, _) => Some(Kind::StrategyBinding),
        ("strategy", Some("transitions"), Some(name), None)
            if name.ends_with(".toml") && name.contains('-') =>
        {
            Some(Kind::StrategyTransition)
        }
        (
            "strategy",
            Some("investigation.toml" | "review.toml" | "implementation.toml"),
            None,
            _,
        ) => Some(Kind::Strategy),
        ("decision-log", Some(name), None, _) if name.ends_with(".md") && name.contains('-') => {
            Some(Kind::Decision)
        }
        ("evidence", Some(name), None, _) if name.ends_with(".md") => Some(Kind::Evidence),
        ("review", Some(_), _, _) if tail.next_back().is_some_and(|name| name.ends_with(".md")) => {
            Some(Kind::Review)
        }
        ("tickets" | "epics", Some("provisional" | "accepted" | "rejected"), Some(name), None)
            if name.ends_with(".md") =>
        {
            Some(if first == "tickets" {
                Kind::Ticket
            } else {
                Kind::Epic
            })
        }
        ("boards", Some(name), None, _) if name.ends_with(".toml") => Some(Kind::Board),
        ("progress", Some("log.toml"), None, _) => Some(Kind::Progress),
        _ => None,
    }
}

pub(super) fn scope_container(path: &str, scope: &str) -> bool {
    if path == scope {
        return true;
    }
    let Some(local) = path
        .strip_prefix(scope)
        .and_then(|path| path.strip_prefix('/'))
    else {
        return false;
    };
    scope_container_components(local.split('/'))
}

pub(super) fn scope_container_native_relative_path(path: &str) -> bool {
    path.is_empty() || scope_container_components(path.split(['/', std::path::MAIN_SEPARATOR]))
}

fn scope_container_components<'a>(mut components: impl Iterator<Item = &'a str>) -> bool {
    matches!(
        (components.next(), components.next(), components.next()),
        (
            Some(
                "implementation-plan"
                    | "strategy"
                    | "decision-log"
                    | "evidence"
                    | "review"
                    | "tickets"
                    | "epics"
                    | "boards"
                    | "progress"
            ),
            None,
            None
        ) | (Some("strategy"), Some("transitions"), None)
            | (
                Some("tickets" | "epics"),
                Some("accepted" | "provisional" | "rejected"),
                None
            )
    )
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
            (
                "projects/.draft/COM10/LPT0/auxiliary",
                "projects/.draft/COM10/LPT0/auxiliary",
            ),
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
            "projects/.. /demo",
            "projects/item./demo",
            "projects/item /demo",
            "projects/C:stream/demo",
            "projects/item:stream/demo",
            "projects/item?/demo",
            "projects/item\u{1f}/demo",
            "projects/demo\0ticket",
        ] {
            assert!(
                normalize_planning_relative(input).is_err(),
                "unexpectedly accepted {input:?}"
            );
        }
        for device in [
            "CON",
            "PRN",
            "AUX",
            "NUL",
            "CLOCK$",
            "CONIN$",
            "CONOUT$",
            "COM1",
            "COM2",
            "COM3",
            "COM4",
            "COM5",
            "COM6",
            "COM7",
            "COM8",
            "COM9",
            "COM\u{b9}",
            "COM\u{b2}",
            "COM\u{b3}",
            "LPT1",
            "LPT2",
            "LPT3",
            "LPT4",
            "LPT5",
            "LPT6",
            "LPT7",
            "LPT8",
            "LPT9",
            "LPT\u{b9}",
            "LPT\u{b2}",
            "LPT\u{b3}",
        ] {
            for suffix in ["", ".tar.gz"] {
                let input = format!("projects/{device}{suffix}/demo");
                assert!(
                    normalize_planning_relative(&input).is_err(),
                    "unexpectedly accepted {input:?}"
                );
            }
        }
        assert!(normalize_planning_relative("projects/cOn.TxT/demo").is_err());
        assert!(normalize_planning_relative("projects/cOnIn$/demo").is_err());
        assert!(normalize_planning_relative("projects/ConOut$.TxT/demo").is_err());
    }

    #[test]
    fn persisted_paths_remain_strictly_slash_canonical() {
        assert!(safe_relative("projects/demo"));
        for input in [
            r"projects\demo",
            "projects//demo",
            "projects/demo/",
            "C:demo",
            "projects/item.",
            "projects/NUL.txt",
        ] {
            assert!(!safe_relative(input), "unexpectedly accepted {input:?}");
        }
    }
}
