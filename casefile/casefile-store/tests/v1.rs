use casefile_core::{ChangeRequest, Classification, Kind, RecordDraft};
use casefile_store::{ActivationState, RelationshipKind, Store};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn fixture() -> TempDir {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
    command(temporary.path(), ["init", "-q"]);
    command(
        temporary.path(),
        ["config", "user.email", "casefile@example.test"],
    );
    command(temporary.path(), ["config", "user.name", "Casefile Test"]);
    command(temporary.path(), ["add", "."]);
    command(temporary.path(), ["commit", "-qm", "fixture"]);
    temporary
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("fixture type").is_dir() {
            fs::create_dir_all(&target).expect("fixture directory");
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("fixture file");
        }
    }
}
fn command(root: &Path, args: impl IntoIterator<Item = &'static str>) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success());
}
fn ticket(root: &Path) -> RecordDraft {
    let path = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    let text = fs::read_to_string(root.join(path)).expect("ticket");
    casefile_core::parse_draft(path, Kind::Ticket, &text).expect("draft")
}
fn path(root: &Path, name: &str) -> std::path::PathBuf {
    if matches!(name, "casefile.toml" | "projects.toml") {
        root.join(name)
    } else {
        root.join("projects/demo/investigations/sample").join(name)
    }
}
fn scan_has(root: &Path, code: &str) {
    assert!(
        Store::open(root)
            .expect("store")
            .scan()
            .expect("scan")
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code),
        "missing {code}"
    );
}

#[test]
fn scans_each_v1_kind_and_preserves_raw_material() {
    let root = fixture();
    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/legacy.txt"),
        "legacy",
    )
    .expect("legacy");
    let result = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan");
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    for kind in [
        Kind::Activation,
        Kind::ProjectMap,
        Kind::Request,
        Kind::Decision,
        Kind::Evidence,
        Kind::Review,
        Kind::Plan,
        Kind::Closeout,
        Kind::Strategy,
        Kind::Ticket,
        Kind::Epic,
        Kind::Board,
    ] {
        assert!(
            result
                .snapshot
                .entries
                .iter()
                .any(|entry| entry.kind == Some(kind)
                    && entry.classification == Classification::Governed),
            "missing {kind:?}"
        );
    }
    assert_eq!(
        Some(Classification::Raw),
        result
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.path.ends_with("legacy.txt"))
            .map(|entry| entry.classification)
    );
}

#[test]
fn activation_state_distinguishes_unactivated_and_invalid_roots() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    assert_eq!(
        ActivationState::Active,
        store.scan().expect("scan").activation
    );

    fs::remove_file(root.path().join("casefile.toml")).expect("remove activation");
    let unactivated = store.scan().expect("unactivated scan");
    assert_eq!(ActivationState::Unactivated, unactivated.activation);
    assert!(unactivated.diagnostics.is_empty());
    assert!(
        unactivated
            .snapshot
            .entries
            .iter()
            .all(|entry| entry.classification == Classification::Ungoverned)
    );

    fs::write(root.path().join("casefile.toml"), "not = [valid").expect("bad activation");
    let invalid = store.scan().expect("invalid scan");
    assert_eq!(ActivationState::Invalid, invalid.activation);
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_activation")
    );
}

#[test]
fn structural_faults_are_deterministic_and_drafts_round_trip() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let draft = ticket(root.path());
    let rendered = casefile_core::render_draft(
        "projects/demo/investigations/sample/tickets/accepted/HMD-011.md",
        &draft,
    )
    .expect("render");
    assert!(matches!(
        casefile_core::parse_draft(
            "projects/demo/investigations/sample/tickets/accepted/HMD-011.md",
            Kind::Ticket,
            std::str::from_utf8(&rendered).expect("UTF-8")
        ),
        Ok(RecordDraft::Ticket(_))
    ));
    let mut injected = draft;
    if let RecordDraft::Ticket(item) = &mut injected {
        item.impact = "safe\n\n## Verification\n\ninjected".into();
    }
    assert!(
        casefile_core::render_draft(
            "projects/demo/investigations/sample/tickets/accepted/HMD-011.md",
            &injected
        )
        .is_err()
    );
    fs::write(root.path().join("projects/demo/investigations/sample/boards/main.toml"), "schema_version = 1\nid = 'HMD-board'\ntitle = 'bad'\n[[columns]]\nname = 'same'\nstatuses = ['accepted']\n[[columns]]\nname = 'same'\nstatuses = ['accepted']\n").expect("bad board");
    let result = store.scan().expect("scan");
    let first = result.diagnostics.clone();
    let second = store.scan().expect("rescan").diagnostics;
    assert_eq!(first, second);
    assert!(
        first
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid_board_column"
                || diagnostic.code == "overlapping_board_status")
    );
}

#[test]
fn table_rows_and_review_faults_are_structural() {
    let faults = [
        (
            "casefile.toml",
            "schema_version = 2\n",
            "invalid_schema_version",
        ),
        (
            "projects.toml",
            "[projects]\nother = 'x'\n",
            "invalid_project_map",
        ),
        ("request.md", "# Requests\n\n## Boundary\n", "request_shape"),
        (
            "decision-log/HMD-D-001-scope.md",
            "# Other\n\n## Status\n\nok\n\n## Decision\n\nok\n",
            "decision_filename_identity",
        ),
        ("evidence/observation.md", "no heading\n", "h1_count"),
        ("review/round-1.md", "no heading\n", "h1_count"),
        ("implementation-plan/PLAN.md", "# Plan\n", "missing_section"),
        ("final-disposition.md", "# Close\n", "missing_section"),
        (
            "strategy/review.toml",
            "schema_version = 1\nstrategy_id = 'x'\nphase = 'wrong'\nadapter = 'x'\n",
            "strategy_phase",
        ),
        (
            "tickets/accepted/HMD-011.md",
            "# no frontmatter\n",
            "missing_frontmatter",
        ),
        (
            "epics/accepted/HMD-E-001.md",
            "# no frontmatter\n",
            "missing_frontmatter",
        ),
        (
            "boards/main.toml",
            "schema_version = 1\nid = 'HMD-board'\ntitle = 'bad'\n",
            "missing_columns",
        ),
    ];
    for (relative, text, code) in faults {
        let root = fixture();
        fs::write(path(root.path(), relative), text).expect("fault");
        scan_has(root.path(), code);
    }
    let root = fixture();
    fs::write(
        root.path().join("projects.toml"),
        "[projects]\ndemo = 'x'\nlegacy = 'keep'\n",
    )
    .expect("extra map");
    assert!(
        Store::open(root.path())
            .expect("store")
            .scan()
            .expect("scan")
            .diagnostics
            .is_empty()
    );
}

#[test]
fn reference_cycle_attachment_prefix_symlink_and_json_diagnostics_are_checked() {
    let root = fixture();
    let ticket_path = path(root.path(), "tickets/accepted/HMD-011.md");
    let original = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(
        &ticket_path,
        original.replace("decision_refs: [HMD-D-001]", "decision_refs: [MISSING]"),
    )
    .expect("references");
    scan_has(root.path(), "unresolved_reference");
    fs::write(
        &ticket_path,
        original.replace("supersedes: []", "supersedes: [HMD-012, HMD-E-001]"),
    )
    .expect("cycle start");
    fs::write(
        path(root.path(), "tickets/accepted/HMD-012.md"),
        original.replace("HMD-011", "HMD-012"),
    )
    .expect("cycle branch");
    let epic = fs::read_to_string(path(root.path(), "epics/accepted/HMD-E-001.md")).expect("epic");
    fs::write(
        path(root.path(), "epics/accepted/HMD-E-001.md"),
        epic.replace("supersedes: []", "supersedes: [HMD-011]"),
    )
    .expect("second edge");
    scan_has(root.path(), "supersession_cycle");
    fs::write(
        path(root.path(), "evidence/observation.md"),
        "---\nattachments: [missing.txt]\n---\n\n# Evidence\n",
    )
    .expect("attachment");
    scan_has(root.path(), "missing_attachment");
    fs::write(root.path().join("casefile.toml"), "schema_version = 1\n[projects.demo]\nprefix = 'NEW'\ninvestigations = ['projects/demo/investigations/sample']\n").expect("prefix");
    scan_has(root.path(), "project_prefix");
    let first = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan")
        .diagnostics;
    let second = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan")
        .diagnostics;
    assert_eq!(
        serde_json::to_vec(&first).expect("JSON"),
        serde_json::to_vec(&second).expect("JSON")
    );
    #[cfg(unix)]
    {
        let root = fixture();
        let request = path(root.path(), "request.md");
        fs::remove_file(&request).expect("remove");
        std::os::unix::fs::symlink("elsewhere", &request).expect("symlink");
        scan_has(root.path(), "unsafe_path");
    }
}

#[test]
fn project_decisions_resolve_within_the_project_only() {
    let root = fixture();
    let ticket_path = path(root.path(), "tickets/accepted/HMD-011.md");
    fs::create_dir_all(root.path().join("projects/demo/decision-log")).expect("project decisions");
    fs::write(
        root.path()
            .join("projects/demo/decision-log/HMD-D-100-project.md"),
        "# HMD-D-100 - Project\n\n## Status\n\naccepted\n\n## Decision\n\nProject scope.\n",
    )
    .expect("project decision");
    let ticket = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(&ticket_path, ticket.replace("HMD-D-001", "HMD-D-100")).expect("project reference");
    let scan = Store::open(root.path())
        .expect("store")
        .scan()
        .expect("scan");
    assert!(scan.diagnostics.is_empty(), "{:#?}", scan.diagnostics);
    assert!(scan.snapshot.entries.iter().any(|entry| entry.path
        == "projects/demo/decision-log/HMD-D-100-project.md"
        && entry.kind == Some(Kind::Decision)
        && entry.classification == Classification::Governed));

    fs::write(root.path().join("casefile.toml"), "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/sample']\n[projects.other]\nprefix = 'OTH'\ninvestigations = []\n").expect("second project");
    fs::create_dir_all(root.path().join("projects/other/decision-log")).expect("other decisions");
    fs::write(
        root.path()
            .join("projects/other/decision-log/OTH-D-001-other.md"),
        "# OTH-D-001 - Other\n\n## Status\n\naccepted\n\n## Decision\n\nOther scope.\n",
    )
    .expect("other decision");
    let ticket = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(&ticket_path, ticket.replace("HMD-D-100", "OTH-D-001"))
        .expect("cross-project reference");
    scan_has(root.path(), "unresolved_reference");
}

#[test]
fn derived_relationships_are_unique_and_directed() {
    let root = fixture();
    let ticket_path = path(root.path(), "tickets/accepted/HMD-011.md");
    let ticket = fs::read_to_string(&ticket_path).expect("ticket");
    fs::write(
        &ticket_path,
        ticket.replace(
            "related_tickets: []",
            "related_tickets: [HMD-E-001, HMD-E-001]",
        ),
    )
    .expect("duplicate ticket relationships");
    let epic_path = path(root.path(), "epics/accepted/HMD-E-001.md");
    let epic = fs::read_to_string(&epic_path).expect("epic");
    fs::write(
        &epic_path,
        epic.replace("related_tickets: []", "related_tickets: [HMD-011]"),
    )
    .expect("reciprocal epic relationship");

    let relationships = Store::open(root.path())
        .expect("store")
        .derived_snapshot()
        .expect("derived snapshot")
        .relationships
        .into_iter()
        .filter(|relationship| relationship.kind == RelationshipKind::Related)
        .collect::<Vec<_>>();

    assert_eq!(relationships.len(), 2);
    assert!(relationships.iter().any(|relationship| {
        relationship.source.identity == "HMD-011" && relationship.target.identity == "HMD-E-001"
    }));
    assert!(relationships.iter().any(|relationship| {
        relationship.source.identity == "HMD-E-001" && relationship.target.identity == "HMD-011"
    }));
}

#[test]
fn nested_investigation_roots_have_distinct_scoped_identities() {
    let root = fixture();
    let ticket =
        fs::read_to_string(path(root.path(), "tickets/accepted/HMD-011.md")).expect("ticket");
    fs::write(
        root.path().join("casefile.toml"),
        "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/alpha/shared', 'projects/demo/investigations/beta/shared']\n",
    )
    .expect("nested activation");
    for investigation in ["alpha/shared", "beta/shared"] {
        let ticket_path = root.path().join(format!(
            "projects/demo/investigations/{investigation}/tickets/accepted/HMD-011.md"
        ));
        fs::create_dir_all(ticket_path.parent().expect("ticket parent")).expect("ticket directory");
        fs::write(
            ticket_path,
            ticket.replace(
                "investigation: \"sample\"",
                &format!("investigation: \"{investigation}\""),
            ),
        )
        .expect("ticket");
    }

    let store = Store::open(root.path()).expect("store");
    let scan = store.scan().expect("scan");
    assert_eq!(
        scan.scope_for_path(
            "projects/demo/investigations/alpha/shared/tickets/accepted/HMD-011.md"
        ),
        Some(("demo", Some("alpha/shared")))
    );
    assert_eq!(
        scan.scope_for_path("projects/demo/investigations/beta/shared/tickets/accepted/HMD-011.md"),
        Some(("demo", Some("beta/shared")))
    );

    let scopes = store
        .derived_snapshot()
        .expect("derived snapshot")
        .records
        .into_iter()
        .filter(|record| {
            record
                .identity
                .as_ref()
                .is_some_and(|identity| identity.identity == "HMD-011")
        })
        .map(|record| record.identity.expect("identity").scope)
        .collect::<Vec<_>>();
    assert!(scopes.contains(&casefile_store::RecordScope {
        project: "demo".into(),
        investigation: Some("alpha/shared".into()),
    }));
    assert!(scopes.contains(&casefile_store::RecordScope {
        project: "demo".into(),
        investigation: Some("beta/shared".into()),
    }));
}

#[test]
fn previews_and_applies_one_path_without_touching_index() {
    let root = fixture();
    let store = Store::open(root.path()).expect("store");
    let index = fs::read(root.path().join(".git/index")).expect("index");
    fs::write(root.path().join("unrelated.txt"), "dirty").expect("dirty worktree");
    let mut create = ticket(root.path());
    if let RecordDraft::Ticket(item) = &mut create {
        item.id = "HMD-012".into();
        item.status = "provisional".into();
        item.title = "Created ticket".into();
    }
    let create_path =
        "projects/demo/investigations/sample/tickets/provisional/HMD-012.md".to_owned();
    let preview = store
        .preview(ChangeRequest::Create {
            path: create_path.clone(),
            draft: create,
        })
        .expect("preview");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
    assert!(preview.diff.contains("new file mode"));
    let create_result = store.apply(preview).expect("create");
    assert!(root.path().join(&create_path).is_file());
    assert_headers(&create_result.diff, &create_path, false, true);
    assert_eq!(
        index,
        fs::read(root.path().join(".git/index")).expect("index preserved")
    );
    assert_eq!(
        "dirty",
        fs::read_to_string(root.path().join("unrelated.txt")).expect("unrelated")
    );
    let mut replacement = ticket(root.path());
    if let RecordDraft::Ticket(item) = &mut replacement {
        item.title = "Replacement".into();
    }
    let replace_path = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md".to_owned();
    let original = fs::read(root.path().join(&replace_path)).expect("original");
    let stale = store
        .preview(ChangeRequest::Replace {
            path: replace_path.clone(),
            draft: replacement,
        })
        .expect("replace preview");
    fs::write(root.path().join(&replace_path), "changed outside preview").expect("external change");
    assert!(store.apply(stale).is_err());
    fs::write(root.path().join(&replace_path), original).expect("restore fixture");
    let mut replacement = ticket(root.path());
    if let RecordDraft::Ticket(item) = &mut replacement {
        item.title = "Applied replacement".into();
    }
    let replace = store
        .preview(ChangeRequest::Replace {
            path: replace_path.clone(),
            draft: replacement,
        })
        .expect("replace");
    assert_headers(&replace.diff, &replace_path, true, true);
    let replace_result = store.apply(replace).expect("apply replacement");
    assert_headers(&replace_result.diff, &replace_path, true, true);
    assert_eq!(
        index,
        fs::read(root.path().join(".git/index")).expect("index after replace")
    );
    let delete = store
        .preview(ChangeRequest::Delete {
            path: create_path.clone(),
        })
        .expect("delete preview");
    assert!(delete.diagnostics.is_empty());
    assert_headers(&delete.diff, &create_path, true, false);
    let delete_result = store.apply(delete).expect("delete");
    assert_headers(&delete_result.diff, &create_path, true, false);
    assert_eq!(
        index,
        fs::read(root.path().join(".git/index")).expect("index after delete")
    );
    assert!(!root.path().join(create_path).exists());
}

#[test]
fn binary_delete_preview_has_canonical_paths() {
    let root = fixture();
    let target = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    fs::write(root.path().join(target), [0xff, 0x00]).expect("binary ticket");
    let preview = Store::open(root.path())
        .expect("store")
        .preview(ChangeRequest::Delete {
            path: target.into(),
        })
        .expect("preview");
    assert!(preview.diagnostics.is_empty(), "{:#?}", preview.diagnostics);
    assert!(
        preview
            .diff
            .contains(&format!("diff --git a/{target} b/{target}"))
    );
    assert!(
        preview
            .diff
            .contains(&format!("Binary files a/{target} and /dev/null differ")),
        "{}",
        preview.diff
    );
    assert!(
        !preview.diff.contains(".tmp") && !preview.diff.contains("/tmp/"),
        "{}",
        preview.diff
    );
}

fn assert_headers(diff: &str, path: &str, before: bool, after: bool) {
    let old = if before {
        format!("--- a/{path}")
    } else {
        "--- /dev/null".into()
    };
    let new = if after {
        format!("+++ b/{path}")
    } else {
        "+++ /dev/null".into()
    };
    assert!(
        diff.contains(&format!("diff --git a/{path} b/{path}")),
        "{diff}"
    );
    assert!(diff.contains(&old), "{diff}");
    assert!(diff.contains(&new), "{diff}");
    assert!(!diff.contains(".tmp") && !diff.contains("/tmp/"), "{diff}");
}
