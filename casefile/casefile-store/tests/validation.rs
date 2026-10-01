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
