use casefile_core::{ChangeRequest, Classification, Kind, RecordDraft};
use casefile_store::Store;
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
    store.apply(preview).expect("create");
    assert!(root.path().join(&create_path).is_file());
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
    let delete = store
        .preview(ChangeRequest::Delete {
            path: create_path.clone(),
        })
        .expect("delete preview");
    assert!(delete.diagnostics.is_empty());
    store.apply(delete).expect("delete");
    assert!(!root.path().join(create_path).exists());
}
