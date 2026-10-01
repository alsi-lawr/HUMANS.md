use super::progressive_tests::{finish, fixture};
use super::*;
use crate::test_support;
use casefile_store::{RelationshipKind, Store};
use std::{fs, path::Path};
use tempfile::TempDir;

const SOURCE: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
const TARGET: &str = "projects/demo/investigations/z-target/tickets/accepted/HMD-012.md";
const DUPLICATE: &str = "projects/demo/investigations/z-target/tickets/provisional/HMD-012.md";
const DECISION: &str = "projects/demo/decision-log/HMD-D-002-project.md";

#[test]
fn canonical_relationships_follow_cross_scope_targets_without_replacing_source_bodies() {
    let root = relationship_fixture();
    let store = Store::open(root.path()).unwrap();
    let mut coordinator = Coordinator::start(store.presentation_session(), None).unwrap();
    let mut app = App::from_projection(coordinator.projection(), None);
    finish(&mut coordinator, &mut app);
    app.set_view(View::Tickets);
    let source_index = app
        .browser
        .entries(&app.scan)
        .iter()
        .position(|entry| entry.path == SOURCE)
        .unwrap();
    app.browser.select_edge(&app.scan, false);
    app.browser.select_offset(&app.scan, source_index as isize);
    assert_eq!(app.browser.selected_path(), Some(SOURCE));
    assert_canonical(&store, &app);
    let edges = source_edges(&app);
    assert_eq!(edges.len(), 5);
    assert!(
        edges
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related
                && edge.target.scope.investigation.as_deref() == Some("z-target"))
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Decision
                && edge.target.scope.investigation.is_none())
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Supersedes)
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.kind == RelationshipKind::SupersededBy)
    );
    assert!(
        !app.unavailable
            .get(SOURCE)
            .is_some_and(|fields| fields.contains("relationships"))
    );
    let original = source_bytes(&app);
    let target = fs::read_to_string(root.path().join(TARGET)).unwrap();

    fs::remove_file(root.path().join(TARGET)).unwrap();
    refresh_target(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        !source_edges(&app)
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related)
    );
    assert_eq!(source_bytes(&app), original);

    write(root.path(), TARGET, &target);
    refresh_target(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        source_edges(&app)
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related)
    );
    write(
        root.path(),
        DUPLICATE,
        &target.replace("status: accepted", "status: provisional"),
    );
    refresh_target(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        !source_edges(&app)
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related)
    );
    fs::remove_file(root.path().join(TARGET)).unwrap();
    refresh_target(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        source_edges(&app)
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related)
    );
    assert_eq!(source_bytes(&app), original);

    let renamed = DUPLICATE.replace("HMD-012", "HMD-015");
    fs::remove_file(root.path().join(DUPLICATE)).unwrap();
    write(
        root.path(),
        &renamed,
        &target
            .replace("HMD-012", "HMD-015")
            .replace("status: accepted", "status: provisional"),
    );
    refresh_target(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        !source_edges(&app)
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related)
    );
    assert_eq!(source_bytes(&app), original);

    fs::remove_file(root.path().join(DECISION)).unwrap();
    coordinator
        .refresh(PresentationTarget::Project {
            project: "demo".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        !source_edges(&app)
            .iter()
            .any(|edge| edge.target.identity == "HMD-D-002")
    );
    assert_eq!(source_bytes(&app), original);

    let source = fs::read_to_string(root.path().join(SOURCE)).unwrap();
    write(root.path(), SOURCE, &no_references(&source));
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: "projects/demo/investigations/sample".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(source_edges(&app).is_empty());
    assert_eq!(app.browser.selected_path(), Some(SOURCE));
    assert!(
        !app.unavailable
            .get(SOURCE)
            .is_some_and(|fields| fields.contains("relationships"))
    );
    assert!(!test_support::render(&app, 180, 40).contains("Unavailable: relationships"));
    let relationships = app.relationships.clone();
    coordinator.request_content(Some(
        "projects/demo/investigations/sample/evidence/observation.md",
    ));
    finish(&mut coordinator, &mut app);
    assert_eq!(app.relationships, relationships);
    assert!(
        !app.unavailable
            .get("projects/demo/investigations/sample/evidence/observation.md")
            .is_some_and(|fields| fields.contains("relationships"))
    );
}

#[test]
fn relationship_availability_tracks_project_coverage_not_a_stale_store_complete_flag() {
    let root = relationship_fixture();
    let store = Store::open(root.path()).unwrap();
    let mut coordinator = Coordinator::start(store.presentation_session(), None).unwrap();
    let mut app = App::from_projection(coordinator.projection(), None);
    finish(&mut coordinator, &mut app);
    let original = source_bytes(&app);
    let activation = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
    write(
        root.path(),
        "casefile.toml",
        &activation.replace(
            "'projects/demo/investigations/z-target'",
            "'projects/demo/investigations/z-target', 'projects/demo/investigations/new-scope'",
        ),
    );
    refresh_target(&mut coordinator, &mut app);
    assert!(
        app.unavailable
            .get(SOURCE)
            .is_some_and(|fields| fields.contains("relationships"))
    );
    assert!(source_edges(&app).is_empty());
    assert!(
        !app.unavailable
            .get(TARGET)
            .is_some_and(|fields| fields.contains("relationships"))
    );
    assert_eq!(source_bytes(&app), original);

    coordinator
        .refresh(PresentationTarget::Project {
            project: "demo".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        !app.unavailable
            .get(SOURCE)
            .is_some_and(|fields| fields.contains("relationships"))
    );
    write(
        root.path(),
        "casefile.toml",
        &activation.replace(", 'projects/demo/investigations/z-target'", ""),
    );
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: "projects/demo/investigations/sample".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert!(
        app.unavailable
            .get(SOURCE)
            .is_some_and(|fields| fields.contains("relationships"))
    );
    coordinator
        .refresh(PresentationTarget::Project {
            project: "demo".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert_canonical(&store, &app);
    assert!(
        !source_edges(&app)
            .iter()
            .any(|edge| edge.kind == RelationshipKind::Related)
    );

    write(
        root.path(),
        "casefile.toml",
        "schema_version = 1\n[projects.other]\nprefix = 'OTHER'\ninvestigations = ['projects/other/investigations/other']\n",
    );
    coordinator
        .refresh(PresentationTarget::Project {
            project: "other".into(),
        })
        .unwrap();
    finish(&mut coordinator, &mut app);
    assert!(source_edges(&app).is_empty());
    assert!(
        app.unavailable
            .get(SOURCE)
            .is_some_and(|fields| fields.contains("relationships"))
    );
}

fn relationship_fixture() -> TempDir {
    let root = fixture();
    write(
        root.path(),
        "casefile.toml",
        "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/sample', 'projects/demo/investigations/z-target']\n[projects.other]\nprefix = 'OTHER'\ninvestigations = ['projects/other/investigations/other']\n",
    );
    let source = fs::read_to_string(root.path().join(SOURCE)).unwrap();
    let target = no_references(&source);
    for id in ["HMD-012", "HMD-013", "HMD-014"] {
        write(
            root.path(),
            &TARGET.replace("HMD-012", id),
            &target
                .replace("HMD-011", id)
                .replace("\"sample\"", "\"z-target\""),
        );
    }
    write(
        root.path(),
        "projects/other/investigations/other/tickets/accepted/OTHER-001.md",
        &target
            .replace("HMD-011", "OTHER-001")
            .replace("\"demo\"", "\"other\"")
            .replace("\"sample\"", "\"other\""),
    );
    write(
        root.path(),
        DECISION,
        "# HMD-D-002 - Project choice\n\n## Status\n\naccepted\n\n## Decision\n\nProject-level choice.\n",
    );
    write(
        root.path(),
        SOURCE,
        &source
            .replace(
                "decision_refs: [HMD-D-001]",
                "decision_refs: [HMD-D-001, HMD-D-002]",
            )
            .replace(
                "related_tickets: []",
                "related_tickets: [HMD-012, HMD-012, HMD-099, OTHER-001]",
            )
            .replace("supersedes: []", "supersedes: [HMD-013]")
            .replace("superseded_by: []", "superseded_by: [HMD-014]"),
    );
    root
}

fn no_references(text: &str) -> String {
    text.lines()
        .map(|line| {
            for field in [
                "decision_refs",
                "related_tickets",
                "supersedes",
                "superseded_by",
            ] {
                if line.starts_with(&format!("{field}:")) {
                    return format!("{field}: []");
                }
            }
            line.into()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn refresh_target(coordinator: &mut Coordinator, app: &mut App) {
    coordinator
        .refresh(PresentationTarget::Investigation {
            project: "demo".into(),
            path: "projects/demo/investigations/z-target".into(),
        })
        .unwrap();
    finish(coordinator, app);
}
fn assert_canonical(store: &Store, app: &App) {
    assert_eq!(
        app.relationships
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>(),
        store.derive_snapshot(&store.scan().unwrap()).relationships
    );
}
fn source_edges(app: &App) -> Vec<&casefile_store::DerivedRelationship> {
    app.relationships
        .values()
        .flatten()
        .filter(|edge| edge.source.identity == "HMD-011")
        .collect()
}
fn source_bytes(app: &App) -> *const u8 {
    app.body_bytes(SOURCE).unwrap().as_ptr()
}
