use casefile_core::{Classification, Kind, RecordDraft};
use casefile_store::{
    InvestigationScope, InvestigationScopedIdentity, Provider, ProviderQuery, ProviderQueryResult,
    Store,
};
use std::{fs, path::Path};
use tempfile::TempDir;

const SCOPE: &str = "projects/demo/investigations/sample";
fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum");
    for entry in walkdir::WalkDir::new(&source).min_depth(1) {
        let entry = entry.unwrap();
        let destination = root
            .path()
            .join(entry.path().strip_prefix(&source).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(destination).unwrap();
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
    root
}
fn scope() -> InvestigationScope {
    InvestigationScope {
        project: "demo".into(),
        investigation: "sample".into(),
    }
}
fn detail(
    provider: &Provider,
    id: &str,
) -> Result<ProviderQueryResult, casefile_store::ProviderError> {
    provider.query(ProviderQuery::RecordDetail {
        identity: InvestigationScopedIdentity {
            scope: scope(),
            identity: id.into(),
        },
    })
}
fn index(provider: &Provider) -> ProviderQueryResult {
    provider
        .query(ProviderQuery::RecordIndex { scope: scope() })
        .unwrap()
}
fn work(root: &Path, kind: Kind, status: &str, id: &str) {
    let source =
        fs::read_to_string(root.join(format!("{SCOPE}/tickets/accepted/HMD-011.md"))).unwrap();
    let RecordDraft::Ticket(mut item) = casefile_core::parse_draft(
        &format!("{SCOPE}/tickets/accepted/HMD-011.md"),
        Kind::Ticket,
        &source,
    )
    .unwrap() else {
        panic!("ticket")
    };
    item.id = id.into();
    item.status = status.into();
    let (directory, draft) = if kind == Kind::Ticket {
        ("tickets", RecordDraft::Ticket(item))
    } else {
        ("epics", RecordDraft::Epic(item))
    };
    let path = format!("{SCOPE}/{directory}/{status}/{id}.md");
    fs::create_dir_all(root.join(&path).parent().unwrap()).unwrap();
    fs::write(
        root.join(&path),
        casefile_core::render_draft(&path, &draft).unwrap(),
    )
    .unwrap();
}

#[test]
fn exact_detail_distinguishes_supported_records_malformed_duplicates_and_absence() {
    let root = fixture();
    let provider = Provider::without_cache(Store::open(root.path()).unwrap());
    fs::create_dir_all(root.path().join(format!("{SCOPE}/progress"))).unwrap();
    fs::write(
        root.path().join(format!("{SCOPE}/progress/log.toml")),
        "malformed unrelated progress",
    )
    .unwrap();
    assert!(
        matches!(detail(&provider, "HMD-404").unwrap(), ProviderQueryResult::RecordDetail {record: None, freshness, ..} if !freshness.dependencies.iter().any(|dependency| matches!(dependency, casefile_store::ReadDependency::Progress {..})))
    );
    for (index, (kind, status)) in [
        (Kind::Ticket, "accepted"),
        (Kind::Ticket, "provisional"),
        (Kind::Ticket, "rejected"),
        (Kind::Epic, "accepted"),
        (Kind::Epic, "provisional"),
        (Kind::Epic, "rejected"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!(
            "HMD-{}{}",
            if kind == Kind::Epic { "E-" } else { "" },
            100 + index
        );
        work(root.path(), kind, status, &id);
        let progress_path = root.path().join(format!("{SCOPE}/progress/log.toml"));
        if kind == Kind::Ticket && status == "accepted" {
            let error = detail(&provider, &id).unwrap_err().to_string();
            assert!(error.contains(&format!("{SCOPE}/progress/log.toml")));
            fs::write(&progress_path, "schema_version=1\n").unwrap();
        }
        let ProviderQueryResult::RecordDetail {
            record: Some(record),
            ..
        } = detail(&provider, &id).unwrap()
        else {
            panic!("existing detail")
        };
        assert_eq!(record.kind, kind);
        assert_eq!(record.identity.identity, id);
        fs::write(&progress_path, "malformed unrelated progress").unwrap();
    }
    let bad = format!("{SCOPE}/tickets/accepted/HMD-404.md");
    fs::write(root.path().join(&bad), "# HMD-404\nwrong format").unwrap();
    let error = detail(&provider, "HMD-404").unwrap_err().to_string();
    assert!(error.contains(&bad));
    assert!(matches!(
        detail(&provider, "HMD-405").unwrap(),
        ProviderQueryResult::RecordDetail { record: None, .. }
    ));
    work(root.path(), Kind::Ticket, "provisional", "HMD-011");
    assert!(
        matches!(detail(&provider, "HMD-011"), Err(casefile_store::ProviderError::AmbiguousRecordIdentity {paths}) if paths.len() == 2)
    );
}

#[test]
fn typed_scope_tokens_track_only_affecting_data_and_selected_mapping_values() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let provider = Provider::without_cache(store.clone());
    let before = index(&provider);

    fs::write(
        root.path().join(format!("{SCOPE}/evidence/unrelated.md")),
        "# Changed evidence\n",
    )
    .unwrap();
    let diagnostics_before = provider
        .query(ProviderQuery::Diagnostics { scope: scope() })
        .unwrap();
    fs::write(
        root.path().join("projects.toml"),
        "[projects]\ndemo = '//source/demo'\nforeign = 37\n",
    )
    .unwrap();
    assert_eq!(index(&provider), before);
    assert_eq!(
        provider
            .query(ProviderQuery::Diagnostics { scope: scope() })
            .unwrap(),
        diagnostics_before
    );
    assert_eq!(
        store.check(None).unwrap().valid,
        Some(false),
        "global map validation remains strict"
    );
    fs::write(
        root.path().join("projects.toml"),
        "[projects]\ndemo = '//source/changed'\nforeign = 37\n",
    )
    .unwrap();
    assert_ne!(
        provider
            .query(ProviderQuery::Diagnostics { scope: scope() })
            .unwrap(),
        diagnostics_before
    );
    assert_eq!(index(&provider), before);
    let path = root
        .path()
        .join(format!("{SCOPE}/tickets/accepted/HMD-011.md"));
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("title: \"Implement sample\"", "title: \"Changed sample\"")
            + "\nordinary affecting edit\n",
    )
    .unwrap();
    assert_ne!(index(&provider), before);
    let missing = detail(&provider, "HMD-499").unwrap();
    work(root.path(), Kind::Ticket, "accepted", "HMD-499");
    assert_ne!(detail(&provider, "HMD-499").unwrap(), missing);
}

#[test]
fn deepest_activated_scope_excludes_nested_records_boards_and_progress() {
    let root = fixture();
    let nested = format!("{SCOPE}/nested");
    let config = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        config.replace(
            &format!("\"{SCOPE}\"]"),
            &format!("\"{SCOPE}\", \"{nested}\"]"),
        ),
    )
    .unwrap();
    let provider = Provider::without_cache(Store::open(root.path()).unwrap());
    let before = index(&provider);
    let boards = provider
        .query(ProviderQuery::Boards { scope: scope() })
        .unwrap();
    for (relative, bytes) in [
        ("tickets/accepted/HMD-991.md", "malformed nested ticket"),
        ("boards/bad.toml", "malformed nested board"),
        ("progress/log.toml", "malformed nested progress"),
    ] {
        let path = root.path().join(&nested).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    assert_eq!(index(&provider), before);
    assert_eq!(
        provider
            .query(ProviderQuery::Boards { scope: scope() })
            .unwrap(),
        boards
    );
    assert!(matches!(
        detail(&provider, "HMD-991").unwrap(),
        ProviderQueryResult::RecordDetail { record: None, .. }
    ));
    let scan = Store::open(root.path()).unwrap().scan().unwrap();
    assert_eq!(
        scan.scope_for_path(&format!("{nested}/evidence/a.md"))
            .unwrap()
            .1,
        Some("sample/nested")
    );
    fs::write(root.path().join("casefile.toml"), config).unwrap();
    let scan = Store::open(root.path()).unwrap().scan().unwrap();
    assert_eq!(
        scan.scope_for_path(&format!("{nested}/evidence/a.md"))
            .unwrap()
            .1,
        Some("sample")
    );
    let config = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        config.replace(
            &format!("\"{SCOPE}\"]"),
            &format!("\"{SCOPE}\", \"{SCOPE}/tickets/accepted\", \"{SCOPE}/progress\"]"),
        ),
    )
    .unwrap();
    assert!(matches!(
        detail(&provider, "HMD-011").unwrap(),
        ProviderQueryResult::RecordDetail { record: None, .. }
    ));
}

#[test]
fn flat_project_decisions_do_not_govern_nested_archives_and_activation_errors_are_unique() {
    let root = fixture();
    let path = root
        .path()
        .join("projects/demo/decision-log/archive/HMD-D-999.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "malformed archive").unwrap();
    let store = Store::open(root.path()).unwrap();
    let scan = store.scan().unwrap();
    let entry = scan
        .snapshot
        .entries
        .iter()
        .find(|entry| entry.path.ends_with("archive/HMD-D-999.md"))
        .unwrap();
    assert_eq!(entry.classification, Classification::Ungoverned);
    assert!(entry.kind.is_none());
    fs::write(root.path().join("casefile.toml"), "malformed [").unwrap();
    let scan = store.scan().unwrap();
    assert_eq!(
        scan.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_activation")
            .count(),
        1
    );
    assert_eq!(scan.diagnostics, store.check(None).unwrap().diagnostics);
}

#[cfg(target_os = "linux")]
#[test]
fn native_names_never_alias() {
    use std::os::unix::ffi::OsStringExt;
    let root = fixture();
    for byte in [0x80, 0x81] {
        fs::write(
            root.path()
                .join(std::ffi::OsString::from_vec(vec![b'x', byte])),
            "distinct",
        )
        .unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    assert!(
        store
            .scan_summary()
            .unwrap_err()
            .to_string()
            .contains("UTF-8")
    );
    for byte in [0x80, 0x81] {
        fs::remove_file(
            root.path()
                .join(std::ffi::OsString::from_vec(vec![b'x', byte])),
        )
        .unwrap();
    }
}

#[cfg(unix)]
#[test]
fn opaque_symlinks_are_not_followed_in_deep_trees() {
    use std::os::unix::fs::symlink;
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("secret"), "not canonical").unwrap();
    symlink(external.path(), root.path().join("opaque-directory")).unwrap();
    symlink(
        external.path().join("secret"),
        root.path().join("opaque-file"),
    )
    .unwrap();
    let mut deep = root.path().join("deep");
    for _ in 0..300 {
        deep.push("d");
        fs::create_dir_all(&deep).unwrap();
    }
    fs::write(deep.join("leaf"), "retained").unwrap();
    let binary = root.path().join("raw.bin");
    fs::write(&binary, [0xff, 0xfe]).unwrap();
    let invalid = root
        .path()
        .join(format!("{SCOPE}/tickets/accepted/HMD-988.md"));
    fs::write(&invalid, [0xff, 0xfe]).unwrap();
    let provider = Provider::without_cache(store.clone());
    assert!(
        detail(&provider, "HMD-988")
            .unwrap_err()
            .to_string()
            .contains("invalid_utf8")
    );
    fs::remove_file(invalid).unwrap();
    let scan = store.scan().unwrap();
    assert!(
        scan.snapshot
            .entries
            .iter()
            .any(|entry| entry.path.ends_with("/leaf"))
    );
    assert!(
        scan.snapshot
            .entries
            .iter()
            .all(|entry| !entry.path.starts_with("opaque-directory/"))
    );
    assert!(scan.diagnostics.is_empty());
    let governed = root
        .path()
        .join(format!("{SCOPE}/tickets/accepted/HMD-999.md"));
    symlink(external.path().join("secret"), &governed).unwrap();
    let provider = Provider::without_cache(store);
    assert!(
        detail(&provider, "HMD-999")
            .unwrap_err()
            .to_string()
            .contains("unsafe_path")
    );
}

#[test]
fn scoped_attachment_freshness_tracks_existence_type_and_containment_not_body_edits() {
    let root = fixture();
    let evidence = root.path().join(format!("{SCOPE}/evidence/attachments.md"));
    fs::write(
        evidence,
        "---\nattachments: [payload.bin]\n---\n# Attachment evidence\n",
    )
    .unwrap();
    let payload = root.path().join(format!("{SCOPE}/evidence/payload.bin"));
    let ignored = root.path().join(format!("{SCOPE}/evidence/ignored.bin"));
    fs::write(&payload, "original bytes").unwrap();
    fs::write(&ignored, "irrelevant bytes").unwrap();
    let provider = Provider::without_cache(Store::open(root.path()).unwrap());
    let query = || {
        provider
            .query(ProviderQuery::Diagnostics { scope: scope() })
            .unwrap()
    };
    let before = query();
    fs::write(&payload, "changed referenced bytes, same regular existence").unwrap();
    fs::write(&ignored, "changed unreferenced bytes").unwrap();
    assert_eq!(query(), before);
    fs::remove_file(&payload).unwrap();
    let missing = query();
    assert_ne!(missing, before);
    assert!(
        matches!(missing, ProviderQueryResult::Diagnostics {diagnostics, ..} if diagnostics.iter().any(|diagnostic| diagnostic.code == "missing_attachment"))
    );
    fs::create_dir(&payload).unwrap();
    assert_ne!(query(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::remove_dir(&payload).unwrap();
        symlink(&ignored, &payload).unwrap();
        assert_ne!(query(), before);
    }
}

#[cfg(unix)]
#[test]
fn consumed_attachment_types_agree_across_scan_checks_and_independent_preview() {
    use casefile_core::ChangeRequest;
    use std::{os::unix::fs::symlink, process::Command};

    let root = fixture();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let evidence_path = format!("{SCOPE}/evidence/ref.md");
    let evidence = root.path().join(&evidence_path);
    let referenced =
        "---\nrefs: [HMD-011]\nattachments: [payload.bin]\n---\n# Attachment evidence\n";
    fs::write(&evidence, referenced).unwrap();
    let payload_path = format!("{SCOPE}/evidence/payload.bin");
    let payload = root.path().join(&payload_path);
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret"), "do not follow").unwrap();
    symlink(outside.path().join("secret"), &payload).unwrap();
    let store = Store::open(root.path()).unwrap();
    let check_diagnostics = || {
        let scan = store.scan().unwrap();
        let global = store.check(None).unwrap();
        let scoped = store.check(Some(SCOPE)).unwrap();
        assert_eq!(scan.diagnostics, global.diagnostics);
        assert_eq!(scan.diagnostics, scoped.diagnostics);
        scan
    };
    let invalid = check_diagnostics();
    assert_eq!(invalid.diagnostics.len(), 1);
    assert_eq!(invalid.diagnostics[0].code, "missing_attachment");
    assert_eq!(invalid.diagnostics[0].path, evidence_path);
    assert!(
        invalid
            .snapshot
            .entries
            .iter()
            .any(|entry| entry.path == payload_path)
    );
    let ticket_path = format!("{SCOPE}/tickets/accepted/HMD-011.md");
    let source = fs::read(root.path().join(&ticket_path)).unwrap();
    let draft = casefile_core::parse_draft(
        &ticket_path,
        Kind::Ticket,
        std::str::from_utf8(&source).unwrap(),
    )
    .unwrap();
    let request = ChangeRequest::Replace {
        path: ticket_path.clone(),
        draft,
    };
    assert!(matches!(
        store.preview(request.clone()),
        Err(casefile_store::StoreError::Invalid(_))
    ));
    assert_eq!(fs::read(root.path().join(&ticket_path)).unwrap(), source);

    fs::write(
        &evidence,
        "---\nrefs: [HMD-011]\n---\n# Unreferenced evidence\n",
    )
    .unwrap();
    assert!(check_diagnostics().diagnostics.is_empty());
    assert!(
        store
            .preview(request.clone())
            .unwrap()
            .diagnostics
            .is_empty()
    );
    fs::write(&evidence, referenced).unwrap();
    fs::remove_file(&payload).unwrap();
    assert_eq!(
        check_diagnostics().diagnostics[0].code,
        "missing_attachment"
    );
    // Preview reports introduced diagnostics, not unchanged pre-existing missing attachments.
    assert!(
        store
            .preview(request.clone())
            .unwrap()
            .diagnostics
            .is_empty()
    );
    fs::create_dir(&payload).unwrap();
    assert_eq!(
        check_diagnostics().diagnostics[0].code,
        "missing_attachment"
    );
    assert!(matches!(
        store.preview(request.clone()),
        Err(casefile_store::StoreError::Invalid(_))
    ));
    fs::remove_dir(&payload).unwrap();
    let ancestor = root.path().join(format!("{SCOPE}/evidence/linked"));
    symlink(outside.path(), &ancestor).unwrap();
    fs::write(
        &evidence,
        "---\nrefs: [HMD-011]\nattachments: [linked/secret]\n---\n# Attachment evidence\n",
    )
    .unwrap();
    let unsafe_parent = check_diagnostics();
    assert_eq!(unsafe_parent.diagnostics[0].code, "missing_attachment");
    assert!(
        unsafe_parent
            .snapshot
            .entries
            .iter()
            .all(|entry| !entry.path.ends_with("/linked/secret"))
    );
    assert!(matches!(
        store.preview(request.clone()),
        Err(casefile_store::StoreError::Invalid(_))
    ));
    fs::write(&evidence, referenced).unwrap();
    fs::write(&payload, []).unwrap();
    let valid = check_diagnostics();
    assert!(valid.diagnostics.is_empty());
    assert!(
        valid
            .snapshot
            .entries
            .iter()
            .any(|entry| entry.path == payload_path && entry.original_bytes.is_empty())
    );
    assert!(store.preview(request).unwrap().diagnostics.is_empty());
    assert_eq!(fs::read(root.path().join(ticket_path)).unwrap(), source);
}

#[test]
fn binding_diagnostics_use_canonical_scope_despite_parent_nested_parent_order() {
    let root = fixture();
    let nested = format!("{SCOPE}/strategy/c-nested");
    let config = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
    fs::write(
        root.path().join("casefile.toml"),
        config.replace(
            &format!("\"{SCOPE}\"]"),
            &format!("\"{SCOPE}\", \"{nested}\"]"),
        ),
    )
    .unwrap();
    fs::create_dir_all(root.path().join(format!("{nested}/strategy"))).unwrap();
    fs::write(
        root.path()
            .join(format!("{nested}/strategy/implementation.toml")),
        "schema_version = 1\nstrategy_id = 'solo'\nphase = 'implementation'\nadapter = 'test'\n",
    )
    .unwrap();
    fs::write(root.path().join(format!("{SCOPE}/strategy/bindings.toml")), "schema_version = 1\nadapter = 'other'\nrole = 'implementation-writer'\nmodel = 'default'\nreasoning_effort = 'high'\n[resolution]\nmode = 'explicit'\nvalue = 'default'\n").unwrap();
    let store = Store::open(root.path()).unwrap();
    let checked = store.check(None).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "binding_adapter"
                && diagnostic.path == format!("{SCOPE}/strategy/bindings.toml"))
    );
    assert_eq!(checked.diagnostics, store.scan().unwrap().diagnostics);
}

#[cfg(unix)]
#[test]
fn requested_governed_containers_distinguish_missing_from_unsafe_without_following() {
    use std::os::unix::fs::symlink;
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("HMD-OUTSIDE.md"),
        "malformed outside record",
    )
    .unwrap();
    symlink(outside.path(), root.path().join("opaque-symlink")).unwrap();
    let store = Store::open(root.path()).unwrap();
    let provider = Provider::without_cache(store.clone());
    let accepted = root.path().join(format!("{SCOPE}/tickets/accepted"));
    let saved = root.path().join("saved-tickets");
    fs::rename(&accepted, &saved).unwrap();
    symlink(outside.path(), &accepted).unwrap();
    assert!(
        provider
            .query(ProviderQuery::RecordIndex { scope: scope() })
            .is_err()
    );
    assert!(store.check(Some(SCOPE)).is_err());
    let checked = store.check(None).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path.ends_with("tickets/accepted")
                && diagnostic.code == "unsafe_path")
    );
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.path.contains("HMD-OUTSIDE"))
    );
    let scan = store.scan().unwrap();
    assert!(
        scan.snapshot
            .entries
            .iter()
            .all(|entry| !entry.path.contains("HMD-OUTSIDE"))
    );
    fs::remove_file(&accepted).unwrap();
    let ProviderQueryResult::RecordIndex { records, .. } = index(&provider) else {
        panic!("index")
    };
    assert!(
        records
            .iter()
            .all(|record| record.kind != Some(Kind::Ticket))
    );
    fs::rename(saved, &accepted).unwrap();

    let strategy = root.path().join(format!("{SCOPE}/strategy"));
    let saved = root.path().join("saved-strategy");
    fs::rename(&strategy, &saved).unwrap();
    symlink(outside.path(), &strategy).unwrap();
    let before = index(&provider);
    assert!(
        provider
            .query(ProviderQuery::StrategyTransitions { scope: scope() })
            .is_err()
    );
    assert!(store.check(Some(SCOPE)).is_err());
    fs::remove_file(&strategy).unwrap();
    assert_eq!(index(&provider), before);
    assert!(
        matches!(provider.query(ProviderQuery::StrategyTransitions { scope: scope() }).unwrap(), ProviderQueryResult::StrategyTransitions {transitions, ..} if transitions.is_empty())
    );
    fs::rename(saved, &strategy).unwrap();
    let transitions = strategy.join("transitions");
    symlink(outside.path(), &transitions).unwrap();
    assert!(
        provider
            .query(ProviderQuery::StrategyTransitions { scope: scope() })
            .is_err()
    );
    assert_eq!(index(&provider), before);
    fs::remove_file(&transitions).unwrap();
    fs::write(&transitions, "not a directory").unwrap();
    assert!(
        provider
            .query(ProviderQuery::StrategyTransitions { scope: scope() })
            .is_err()
    );
    fs::remove_file(transitions).unwrap();
    let investigation = root.path().join(SCOPE);
    fs::rename(&investigation, root.path().join("saved-investigation")).unwrap();
    symlink(outside.path(), &investigation).unwrap();
    assert!(store.check(Some(SCOPE)).is_err());
    assert!(
        provider
            .query(ProviderQuery::RecordIndex { scope: scope() })
            .is_err()
    );
    assert!(
        store
            .check(None)
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == SCOPE && diagnostic.code == "unsafe_path")
    );
    let scanned = store.scan().unwrap();
    assert!(
        scanned
            .snapshot
            .entries
            .iter()
            .any(|entry| entry.path == SCOPE && entry.classification == Classification::Invalid)
    );
    assert!(
        scanned
            .snapshot
            .entries
            .iter()
            .all(|entry| !entry.path.contains("HMD-OUTSIDE"))
    );
    fs::remove_file(&investigation).unwrap();
    assert_eq!(store.check(None).unwrap().valid, Some(true));
}

#[test]
fn editable_acquisition_is_exact_and_preserves_original_receipt_across_nested_owners() {
    let root = fixture();
    let path = format!("{SCOPE}/tickets/accepted/HMD-011.md");
    let nested = format!("{SCOPE}/archive");
    fs::write(root.path().join("casefile.toml"), format!("schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['{SCOPE}', '{nested}']\n")).unwrap();
    fs::create_dir_all(root.path().join(format!("{nested}/boards"))).unwrap();
    let board_path = format!("{nested}/boards/work.toml");
    fs::write(root.path().join(&board_path), "schema_version = 1\nid = 'HMD-B-099'\ntitle = 'Nested board'\nstatus_source = 'disposition'\n[[columns]]\nname = 'Accepted'\nstatuses = ['accepted']\n").unwrap();
    fs::write(
        root.path()
            .join(format!("{SCOPE}/strategy/implementation.toml")),
        "unrelated malformed strategy",
    )
    .unwrap();
    fs::write(
        root.path()
            .join(format!("{SCOPE}/tickets/accepted/HMD-999.md")),
        [0xff],
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let opaque = root.path().join("unreadable-unrelated.bin");
        fs::write(&opaque, "unrelated").unwrap();
        fs::set_permissions(opaque, fs::Permissions::from_mode(0o000)).unwrap();
    }
    let store = Store::open(root.path()).unwrap();
    let entry = store
        .read_editable_entry(&path.replace('/', "\\\\"), Kind::Ticket)
        .unwrap()
        .unwrap();
    assert_eq!(entry.path, path);
    assert_eq!(
        entry.original_bytes,
        fs::read(root.path().join(&path)).unwrap()
    );
    assert!(
        store
            .read_editable_entry(&board_path, Kind::Board)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .read_editable_entry(
                &format!("{SCOPE}/tickets/accepted/HMD-404.md"),
                Kind::Ticket
            )
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .read_editable_entry(
                &format!("{SCOPE}/tickets/accepted/HMD-999.md"),
                Kind::Ticket
            )
            .is_err()
    );
    fs::write(
        root.path().join(&path),
        entry
            .original_bytes
            .iter()
            .copied()
            .chain(b"\nExternal edit\n".iter().copied())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_ne!(
        entry.content_revision,
        store
            .read_editable_entry(&path, Kind::Ticket)
            .unwrap()
            .unwrap()
            .content_revision
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("ticket"), &entry.original_bytes).unwrap();
        fs::remove_file(root.path().join(&path)).unwrap();
        std::os::unix::fs::symlink(outside.path().join("ticket"), root.path().join(&path)).unwrap();
        assert!(store.read_editable_entry(&path, Kind::Ticket).is_err());
        fs::remove_file(root.path().join(&path)).unwrap();
        let parent = root.path().join(format!("{SCOPE}/tickets/accepted"));
        fs::rename(&parent, parent.with_extension("saved")).unwrap();
        std::os::unix::fs::symlink(outside.path(), &parent).unwrap();
        assert!(store.read_editable_entry(&path, Kind::Ticket).is_err());
    }
}
