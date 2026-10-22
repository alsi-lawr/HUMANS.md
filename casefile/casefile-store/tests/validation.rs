use casefile_core::{Kind, RecordDraft};
use casefile_store::Store;
use std::fs;

#[test]
fn long_supersession_chains_and_cycle_reachable_ancestors_preserve_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let scope = "projects/demo/investigations/sample";
    let directory = root.path().join(format!("{scope}/tickets/accepted"));
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['{scope}']\n"
        ),
    )
    .unwrap();
    fs::write(
        root.path().join("projects.toml"),
        "[projects]\ndemo = '//offline/demo'\n",
    )
    .unwrap();
    let source = include_str!(
        "fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md"
    );
    let RecordDraft::Ticket(mut template) = casefile_core::parse_draft(
        &format!("{scope}/tickets/accepted/HMD-011.md"),
        Kind::Ticket,
        source,
    )
    .unwrap() else {
        panic!("ticket")
    };
    template.decision_refs.clear();
    for number in 100..1300 {
        let mut item = template.clone();
        item.id = format!("HMD-{number}");
        if number < 1299 {
            item.supersedes = vec![format!("HMD-{}", number + 1)];
        }
        let path = format!("{scope}/tickets/accepted/{}.md", item.id);
        fs::write(
            root.path().join(&path),
            casefile_core::render_draft(&path, &RecordDraft::Ticket(item)).unwrap(),
        )
        .unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    assert_eq!(store.check(Some(scope)).unwrap().valid, Some(true));
    let mut end = template;
    end.id = "HMD-1299".into();
    end.supersedes = vec!["HMD-500".into()];
    let path = format!("{scope}/tickets/accepted/HMD-1299.md");
    fs::write(
        root.path().join(&path),
        casefile_core::render_draft(&path, &RecordDraft::Ticket(end)).unwrap(),
    )
    .unwrap();
    let checked = store.check(Some(scope)).unwrap();
    assert_eq!(
        checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "supersession_cycle")
            .count(),
        1200
    );
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == "supersession_cycle")
    );
    let orphan = root.path().join(format!("{scope}/boards"));
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("duplicate.toml"), "schema_version = 1\nid = 'HMD-100'\ntitle = 'Duplicate identity'\n[[columns]]\nname = 'Accepted'\nstatuses = ['accepted']\n").unwrap();
    assert!(
        store
            .check(Some(scope))
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "duplicate_identity")
    );
}

#[test]
fn explicit_same_id_cross_scope_supersession_edges_detect_cycles_and_invalid_required_records() {
    use casefile_core::ChangeRequest;
    let root = tempfile::tempdir().unwrap();
    let first_scope = "projects/demo/investigations/first";
    let second_scope = "projects/demo/investigations/second";
    fs::write(root.path().join("casefile.toml"), format!("schema_version=1\n[projects.demo]\nprefix='HMD'\ninvestigations=['{first_scope}','{second_scope}']\n")).unwrap();
    fs::write(
        root.path().join("projects.toml"),
        "[projects]\ndemo='//source/demo'\n",
    )
    .unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let source = include_str!(
        "fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md"
    )
    .replace("decision_refs: [HMD-D-001]", "decision_refs: []");
    let first = format!("{first_scope}/tickets/accepted/HMD-011.md");
    let second = format!("{second_scope}/tickets/accepted/HMD-011.md");
    for (path, investigation) in [(&first, "first"), (&second, "second")] {
        fs::create_dir_all(root.path().join(path).parent().unwrap()).unwrap();
        fs::write(
            root.path().join(path),
            source.replace(
                "investigation: \"sample\"",
                &format!("investigation: \"{investigation}\""),
            ),
        )
        .unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    assert_eq!(store.check(Some(first_scope)).unwrap().valid, Some(true));
    assert!(
        store
            .check(None)
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "duplicate_identity")
    );
    let mut draft = casefile_core::parse_draft(
        &first,
        Kind::Ticket,
        &fs::read_to_string(root.path().join(&first)).unwrap(),
    )
    .unwrap();
    if let RecordDraft::Ticket(item) = &mut draft {
        item.supersedes = vec![second.clone()];
    }
    let preview = store
        .preview(ChangeRequest::Replace {
            path: first.clone(),
            draft,
        })
        .unwrap();
    assert!(preview.diagnostics.is_empty(), "{:?}", preview.diagnostics);
    store.apply(preview).unwrap();
    let unchanged = fs::read_to_string(root.path().join(&second)).unwrap();
    let mut draft = casefile_core::parse_draft(&second, Kind::Ticket, &unchanged).unwrap();
    if let RecordDraft::Ticket(item) = &mut draft {
        item.supersedes = vec![first.clone()];
    }
    let preview = store
        .preview(ChangeRequest::Replace {
            path: second.clone(),
            draft,
        })
        .unwrap();
    assert!(
        preview
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "supersession_cycle")
    );
    assert!(store.apply(preview).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join(&second)).unwrap(),
        unchanged
    );
    fs::write(
        root.path().join(&second),
        unchanged.replace("# HMD-011", "# Wrong identity"),
    )
    .unwrap();
    let checked = store.check(Some(first_scope)).unwrap();
    assert_eq!(checked.valid, Some(false));
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == second && diagnostic.code == "identity_heading")
    );
}
