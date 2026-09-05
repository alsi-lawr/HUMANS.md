use crate::{
    ProgressOperation, Provider, Store, StrategyTransitionRequest, WriterBindingRequest,
    mutation_hooks::{self, Boundary},
};
use casefile_core::{
    BoardColumn, BoardDraft, BoardStatusSource, ChangeRequest, ProgressEntry, ProgressStatus,
    RecordDraft,
};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
use tempfile::TempDir;
const BASE: &str = "projects/demo/investigations/sample";

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}
fn fixture() -> TempDir {
    let root = TempDir::new().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
        root.path(),
    );
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    root
}
fn board_at(base: &str, name: &str, id: &str) -> ChangeRequest {
    ChangeRequest::Create {
        path: format!("{base}/boards/{name}.toml"),
        draft: RecordDraft::Board(BoardDraft {
            id: id.into(),
            title: name.into(),
            status_source: BoardStatusSource::Progress,
            filter_statuses: None,
            filter_kinds: None,
            columns: vec![BoardColumn {
                name: "TODO".into(),
                statuses: vec!["unknown".into()],
            }],
        }),
    }
}
fn board(name: &str) -> ChangeRequest {
    board_at(BASE, name, &format!("HMD-{name}"))
}
fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), bytes).unwrap();
}
fn disjoint_at(boundary: Boundary, other: casefile_core::Preview) {
    let mut fired = false;
    mutation_hooks::set(move |event, root, _| {
        if event == boundary && !fired {
            fired = true;
            Store::open(root)
                .unwrap()
                .apply(other.clone())
                .expect("disjoint write while first is protected");
        }
    });
}

#[test]
fn provider_same_session_writes_survive_locked_validation_and_result_windows() {
    for boundary in [Boundary::Locked, Boundary::Commit, Boundary::Result] {
        for cross_project in [false, true] {
            let root = fixture();
            let other_base = "projects/other/investigations/sample";
            if cross_project {
                let mut config = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
                config.push_str(&format!(
                    "\n[projects.other]\nprefix = \"OTHER\"\ninvestigations = [\"{other_base}\"]\n"
                ));
                write(root.path(), "casefile.toml", config);
                write(
                    root.path(),
                    "projects.toml",
                    "[projects]\ndemo = \"/source/demo\"\nother = \"/source/other\"\n",
                );
            }
            let provider = Arc::new(Provider::without_cache(Store::open(root.path()).unwrap()));
            let first = provider.preview_record(board("first")).unwrap();
            let second = provider
                .preview_record(if cross_project {
                    board_at(other_base, "second", "OTHER-second")
                } else {
                    board("second")
                })
                .unwrap();
            let shared = Arc::clone(&provider);
            let mut second = Some(second);
            mutation_hooks::set(move |event, _, _| {
                if event == boundary {
                    if let Some(preview) = second.take() {
                        shared
                            .apply_record(preview)
                            .expect("same Provider disjoint write");
                    }
                }
            });
            provider
                .apply_record(first)
                .expect("first committed outcome");
            mutation_hooks::clear();
            assert!(
                root.path()
                    .join(format!("{BASE}/boards/first.toml"))
                    .exists()
            );
            let base = if cross_project { other_base } else { BASE };
            assert!(
                root.path()
                    .join(format!("{base}/boards/second.toml"))
                    .exists()
            );
        }
    }
}

#[test]
fn growth_and_nontransitive_neighbors_do_not_expand_body_reads_or_conflicts() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let ticket = format!("{BASE}/tickets/accepted/HMD-011.md");
    let template = fs::read_to_string(root.path().join(&ticket)).unwrap();
    for (id, neighbor) in [("HMD-012", "HMD-013"), ("HMD-013", "")] {
        write(
            root.path(),
            &format!("{BASE}/tickets/accepted/{id}.md"),
            template.replace("HMD-011", id).replace(
                "related_tickets: []",
                &format!("related_tickets: [{neighbor}]"),
            ),
        );
    }
    write(
        root.path(),
        &ticket,
        template.replace("related_tickets: []", "related_tickets:\n  - HMD-012"),
    );
    let mut draft = casefile_core::parse_draft(
        &ticket,
        casefile_core::Kind::Ticket,
        &fs::read_to_string(root.path().join(&ticket)).unwrap(),
    )
    .unwrap();
    if let RecordDraft::Ticket(item) = &mut draft {
        item.title = "changed".into();
    }
    let request = ChangeRequest::Replace {
        path: ticket.clone(),
        draft,
    };
    let reads = Arc::new(Mutex::new(BTreeSet::new()));
    let recorded = Arc::clone(&reads);
    mutation_hooks::set(move |boundary, _, path| {
        if boundary == Boundary::Read {
            recorded.lock().unwrap().insert(path.to_owned());
        }
    });
    let preview = store.preview(request.clone()).unwrap();
    mutation_hooks::clear();
    assert!(preview.diagnostics.is_empty(), "{:?}", preview.diagnostics);
    let baseline = reads.lock().unwrap().clone();
    let neighbor = format!("{BASE}/tickets/accepted/HMD-013.md");
    assert!(!baseline.contains(&neighbor));
    assert!(!preview.expected_input_revisions.contains_key(&neighbor));
    for i in 100..150 {
        write(
            root.path(),
            &format!("{BASE}/tickets/accepted/HMD-{i}.md"),
            template.replace("HMD-011", &format!("HMD-{i}")),
        );
        write(
            root.path(),
            &format!("{BASE}/raw/{i}.bin"),
            vec![0xff; 10000],
        );
    }
    reads.lock().unwrap().clear();
    let recorded = Arc::clone(&reads);
    mutation_hooks::set(move |boundary, _, path| {
        if boundary == Boundary::Read {
            recorded.lock().unwrap().insert(path.to_owned());
        }
    });
    let after = store.preview(request).unwrap();
    mutation_hooks::clear();
    assert_eq!(baseline, *reads.lock().unwrap());
    assert_eq!(
        preview.expected_input_revisions,
        after.expected_input_revisions
    );
    let neighbor_draft = casefile_core::parse_draft(
        &neighbor,
        casefile_core::Kind::Ticket,
        &fs::read_to_string(root.path().join(&neighbor)).unwrap(),
    )
    .unwrap();
    let mut neighbor_draft = neighbor_draft;
    if let RecordDraft::Ticket(item) = &mut neighbor_draft {
        item.title = "unrelated neighbor edit".into();
    }
    let other = store
        .preview(ChangeRequest::Replace {
            path: neighbor,
            draft: neighbor_draft,
        })
        .unwrap();
    disjoint_at(Boundary::Result, other);
    store
        .apply(preview)
        .expect("non-transitive neighbor cannot invalidate result");
    mutation_hooks::clear();
}

#[test]
fn global_identity_phantoms_and_multiline_board_metadata_remain_conflicts() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let first = store
        .preview(board_at(BASE, "first", "HMD-shared"))
        .unwrap();
    let second = store
        .preview(board_at(BASE, "second", "HMD-shared"))
        .unwrap();
    store.apply(first).unwrap();
    assert!(store.apply(second).is_err());
    assert!(
        !root
            .path()
            .join(format!("{BASE}/boards/second.toml"))
            .exists()
    );
    let path = format!("{BASE}/boards/first.toml");
    let source = fs::read_to_string(root.path().join(&path)).unwrap();
    let source = source
        .lines()
        .filter(|line| !line.starts_with("id =") && !line.starts_with("title ="))
        .collect::<Vec<_>>()
        .join("\n");
    write(
        root.path(),
        &path,
        format!("title = '''first\n[not a table]\n'''\nid = 'HMD-shared'\n{source}\n"),
    );
    assert!(store.scan().unwrap().diagnostics.is_empty());
    assert!(
        !store
            .preview(board_at(BASE, "second", "HMD-shared"))
            .unwrap()
            .diagnostics
            .is_empty()
    );
}

fn append() -> ProgressOperation {
    ProgressOperation::Append {
        investigation: BASE.into(),
        entries: vec![ProgressEntry::Transition {
            id: "start".into(),
            recorded_at: "2026-07-27T12:00:00Z".into(),
            recorded_by: "root".into(),
            ticket_id: "HMD-011".into(),
            from: ProgressStatus::Unknown,
            to: ProgressStatus::InProgress,
        }],
    }
}
#[test]
fn progress_bootstrap_append_replay_and_record_batch_have_narrow_result_windows() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let provider = Provider::without_cache(store.clone());
    let bootstrap = provider.bootstrap_progress(BASE).unwrap();
    disjoint_at(
        Boundary::Result,
        store.preview(board("bootstrap-overlap")).unwrap(),
    );
    provider.apply_progress(bootstrap).unwrap();
    mutation_hooks::clear();
    let progress = provider.preview_progress(append()).unwrap();
    disjoint_at(
        Boundary::Result,
        store.preview(board("append-overlap")).unwrap(),
    );
    provider.apply_progress(progress.clone()).unwrap();
    mutation_hooks::clear();
    disjoint_at(
        Boundary::Locked,
        store.preview(board("replay-overlap")).unwrap(),
    );
    assert!(provider.apply_progress(progress).unwrap().result.no_op);
    mutation_hooks::clear();
    let batch = store
        .preview_batch(vec![board("batch-a"), board("batch-b")])
        .unwrap();
    disjoint_at(
        Boundary::Result,
        store.preview(board("batch-overlap")).unwrap(),
    );
    let result = store.apply_batch(batch).unwrap();
    mutation_hooks::clear();
    assert_eq!(result.resulting_target_revisions.len(), 2);
}

#[test]
fn shared_reference_changes_and_reverse_progress_membership_cannot_write_skew() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let first = format!("{BASE}/tickets/accepted/HMD-011.md");
    let second = format!("{BASE}/tickets/accepted/HMD-012.md");
    let template = fs::read_to_string(root.path().join(&first)).unwrap();
    write(root.path(), &second, template.replace("HMD-011", "HMD-012"));
    let mut previews = Vec::new();
    for (path, other) in [(&first, "HMD-012"), (&second, "HMD-011")] {
        let mut draft = casefile_core::parse_draft(
            path,
            casefile_core::Kind::Ticket,
            &fs::read_to_string(root.path().join(path)).unwrap(),
        )
        .unwrap();
        if let RecordDraft::Ticket(item) = &mut draft {
            item.supersedes = vec![other.into()];
        }
        previews.push(
            store
                .preview(ChangeRequest::Replace {
                    path: path.clone(),
                    draft,
                })
                .unwrap(),
        );
    }
    store.apply(previews.remove(0)).unwrap();
    assert!(store.apply(previews.remove(0)).is_err());
    let provider = Provider::without_cache(store.clone());
    let pending = provider.preview_progress(append()).unwrap();
    let deletion = store
        .preview(ChangeRequest::Delete {
            path: first.clone(),
        })
        .unwrap();
    assert!(deletion.diagnostics.is_empty());
    provider.apply_progress(pending).unwrap();
    assert!(store.apply(deletion).is_err());
    assert!(root.path().join(&first).exists());
    assert!(store.scan().unwrap().diagnostics.is_empty());
}

#[test]
fn record_batch_failure_restores_prior_targets_without_touching_unrelated_data() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let batch = store
        .preview_batch(vec![board("first"), board("second")])
        .unwrap();
    let second = format!("{BASE}/boards/second.toml");
    mutation_hooks::fail_write(second);
    assert!(store.apply_batch(batch).is_err());
    mutation_hooks::clear();
    assert!(
        !root
            .path()
            .join(format!("{BASE}/boards/first.toml"))
            .exists()
    );
    assert!(
        root.path()
            .join(format!("{BASE}/boards/main.toml"))
            .exists()
    );
}

fn matrix(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../adapters/codex/matrices")
            .join(name),
    )
    .unwrap()
}
fn transition() -> StrategyTransitionRequest {
    StrategyTransitionRequest {
        investigation: BASE.into(),
        operation_id: "select-pipeline".into(),
        recorded_at: "2026-07-27T12:00:00Z".into(),
        selected_matrix_origin: "pipeline.toml".into(),
        selected_matrix_source: matrix("casefile-implement-pipeline.toml"),
        available_capabilities: vec![
            "exclusive_writer".into(),
            "shared_writable_planning".into(),
            "subagents".into(),
        ],
        preserved_work_paths: vec![],
        active_ownership: vec![],
        rationale: "selected".into(),
    }
}
#[test]
fn governed_multi_file_and_binding_results_and_legacy_replay_are_independent() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    write(
        root.path(),
        &format!("{BASE}/strategy/implementation.toml"),
        matrix("casefile-implement-ticket-batch.toml"),
    );
    let request = transition();
    let preview = store.preview_strategy_transition(request.clone()).unwrap();
    assert!(preview.transition_record.expected_store_revision.is_none());
    let history_path = preview.changes[1].path.clone();
    disjoint_at(
        Boundary::Result,
        store.preview(board("governance-overlap")).unwrap(),
    );
    store.apply_strategy_transition(preview).unwrap();
    mutation_hooks::clear();
    let history = fs::read_to_string(root.path().join(&history_path)).unwrap();
    let mut legacy = casefile_core::parse_strategy_transition(&history_path, &history).unwrap();
    legacy.expected_store_revision =
        Some(casefile_core::Revision("historical-global-revision".into()));
    let legacy_bytes = casefile_core::render_strategy_transition(&legacy);
    write(root.path(), &history_path, &legacy_bytes);
    let replay = store.preview_strategy_transition(request).unwrap();
    assert!(replay.no_op);
    assert_eq!(replay.transition_record, legacy);
    store.apply_strategy_transition(replay).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join(&history_path)).unwrap(),
        legacy_bytes
    );
    let mut next = transition();
    next.operation_id = "select-again".into();
    next.recorded_at = "2026-07-27T13:00:00Z".into();
    let next = store.preview_strategy_transition(next).unwrap();
    store.apply_strategy_transition(next).unwrap();
    let records = store
        .scan()
        .unwrap()
        .snapshot
        .entries
        .into_iter()
        .filter_map(|e| match e.summary {
            Some(casefile_core::RecordSummary::StrategyTransition { record }) => Some(record),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .any(|r| r.expected_store_revision == legacy.expected_store_revision)
    );
    assert!(records.iter().any(|r| r.expected_store_revision.is_none()));
    let provider = Provider::without_cache(store.clone());
    provider
        .apply_progress(provider.bootstrap_progress(BASE).unwrap())
        .unwrap();
    let binding = WriterBindingRequest { investigation: BASE.into(), binding_source:"schema_version = 1\nadapter = \"codex\"\nrole = \"implementation-writer\"\nmodel = \"gpt-6-astra\"\nreasoning_effort = \"high\"\n\n[resolution]\nmode = \"named_agent_type\"\nvalue = \"casefile-implementation-writer-gpt-6-astra-high\"\n".into() };
    let preview = store.preview_writer_binding(binding).unwrap();
    disjoint_at(
        Boundary::Result,
        store.preview(board("binding-overlap")).unwrap(),
    );
    store.apply_writer_binding(preview).unwrap();
    mutation_hooks::clear();
}

#[test]
fn governed_mid_transaction_failure_restores_the_matrix() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let matrix_path = format!("{BASE}/strategy/implementation.toml");
    let original = matrix("casefile-implement-ticket-batch.toml");
    write(root.path(), &matrix_path, &original);
    let preview = store.preview_strategy_transition(transition()).unwrap();
    let fail_path = preview.changes[1].path.clone();
    mutation_hooks::fail_write(fail_path);
    assert!(store.apply_strategy_transition(preview).is_err());
    mutation_hooks::clear();
    assert_eq!(
        fs::read_to_string(root.path().join(matrix_path)).unwrap(),
        original
    );
}

#[test]
fn process_writer() {
    let Some(manifest) = std::env::var_os("CASEFILE_MUTATION_CHILD") else {
        return;
    };
    let input: (std::path::PathBuf, casefile_core::Preview, bool) =
        serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
    let mut announced = false;
    mutation_hooks::set(move |event, _, _| {
        if event == Boundary::Attempt && !announced {
            announced = true;
            use std::io::Write;
            println!("CASEFILE_ATTEMPT");
            std::io::stdout().flush().unwrap();
        }
    });
    let result = Store::open(input.0).unwrap().apply(input.1);
    assert_eq!(result.is_ok(), input.2, "{result:?}");
}

fn child(
    root: &Path,
    preview: &casefile_core::Preview,
    succeeds: bool,
    manifest: &Path,
) -> std::process::Child {
    fs::write(
        manifest,
        serde_json::to_vec(&(root, preview, succeeds)).unwrap(),
    )
    .unwrap();
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "mutation_tests::process_writer", "--nocapture"])
        .env("CASEFILE_MUTATION_CHILD", manifest)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn independent_processes_coordinate_disjoint_targets_replacement_creation_and_identity_claims() {
    use std::io::BufRead;
    for (same_path, same_identity, replace, cross_project) in [
        (false, false, false, false),
        (false, false, false, true),
        (true, false, false, false),
        (true, false, true, false),
        (false, true, false, false),
    ] {
        let root = fixture();
        let store = Store::open(root.path()).unwrap();
        let other_base = "projects/other/investigations/sample";
        if cross_project {
            let mut config = fs::read_to_string(root.path().join("casefile.toml")).unwrap();
            config.push_str(&format!(
                "\n[projects.other]\nprefix = \"OTHER\"\ninvestigations = [\"{other_base}\"]\n"
            ));
            write(root.path(), "casefile.toml", config);
            write(
                root.path(),
                "projects.toml",
                "[projects]\ndemo = \"/source/demo\"\nother = \"/source/other\"\n",
            );
        }
        let mut first = board("first");
        if replace {
            store.apply(store.preview(first.clone()).unwrap()).unwrap();
            if let ChangeRequest::Create { path, mut draft } = first {
                if let RecordDraft::Board(board) = &mut draft {
                    board.title = "replacement-one".into();
                }
                first = ChangeRequest::Replace { path, draft };
            }
        }
        let mut second = if cross_project {
            board_at(other_base, "second", "OTHER-second")
        } else if same_path {
            first.clone()
        } else if same_identity {
            board_at(BASE, "second", "HMD-first")
        } else {
            board("second")
        };
        if let ChangeRequest::Replace {
            draft: RecordDraft::Board(board),
            ..
        } = &mut second
        {
            board.title = "replacement-two".into();
        }
        let first = store.preview(first).unwrap();
        let second = store.preview(second).unwrap();
        let manifests = TempDir::new().unwrap();
        let manifest = manifests.path().join("request.json");
        let pending = Arc::new(Mutex::new(None));
        let state = Arc::clone(&pending);
        let mut fired = false;
        mutation_hooks::set(move |event, root, _| {
            if event == Boundary::Locked && !fired {
                fired = true;
                let mut process = child(root, &second, !same_path && !same_identity, &manifest);
                let mut reader = std::io::BufReader::new(process.stdout.take().unwrap());
                let mut line = String::new();
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line.trim() == "CASEFILE_ATTEMPT" {
                        break;
                    }
                }
                process.stdout = Some(reader.into_inner());
                if !same_path && !same_identity {
                    let output = process.wait_with_output().unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                } else {
                    *state.lock().unwrap() = Some(process);
                }
            }
        });
        store.apply(first).unwrap();
        mutation_hooks::clear();
        if let Some(process) = pending.lock().unwrap().take() {
            let output = process.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(store.scan().unwrap().diagnostics.is_empty());
    }
}

#[test]
fn record_delete_no_op_and_default_board_receipts_survive_disjoint_result_changes() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    let provider = Provider::without_cache(store.clone());
    let created = store
        .apply(store.preview(board("target")).unwrap())
        .unwrap();
    let ChangeRequest::Create { path, draft } = board("target") else {
        unreachable!()
    };
    let no_op = provider
        .preview_record(ChangeRequest::Replace {
            path: path.clone(),
            draft,
        })
        .unwrap();
    assert!(no_op.no_op);
    disjoint_at(
        Boundary::Result,
        store.preview(board("no-op-overlap")).unwrap(),
    );
    let result = provider.apply_record(no_op).unwrap();
    mutation_hooks::clear();
    assert_eq!(
        result.result.result.resulting_target_revision,
        created.resulting_target_revision
    );
    let deletion = store.preview(ChangeRequest::Delete { path }).unwrap();
    disjoint_at(
        Boundary::Result,
        store.preview(board("delete-overlap")).unwrap(),
    );
    assert!(
        store
            .apply(deletion)
            .unwrap()
            .resulting_target_revision
            .is_none()
    );
    mutation_hooks::clear();
    let preview = provider.preview_default_delivery_board(BASE).unwrap();
    disjoint_at(
        Boundary::Result,
        store.preview(board("default-overlap")).unwrap(),
    );
    let created = provider.apply_default_delivery_board(preview).unwrap();
    mutation_hooks::clear();
    let repeated = provider.preview_default_delivery_board(BASE).unwrap();
    assert!(repeated.no_op);
    disjoint_at(
        Boundary::Result,
        store.preview(board("default-no-op-overlap")).unwrap(),
    );
    let result = provider.apply_default_delivery_board(repeated).unwrap();
    mutation_hooks::clear();
    assert_eq!(
        created.result.result.resulting_target_revision,
        result.result.result.resulting_target_revision
    );
}

#[test]
fn reverse_evidence_uses_canonical_metadata_and_aliases_cannot_bypass_it() {
    let root = fixture();
    let store = Store::open(root.path()).unwrap();
    store
        .apply(store.preview(board("target")).unwrap())
        .unwrap();
    write(
        root.path(),
        &format!("{BASE}/evidence/edge.md"),
        "---\nid: 123\nphase: [ignored]\nrefs:\n  - HMD-target\nattachments: []\n---\n\n# Evidence\n\nRetain the referenced board.\n",
    );
    assert!(store.scan().unwrap().diagnostics.is_empty());
    let path = format!("{BASE}/boards/target.toml");
    let preview = store
        .preview(ChangeRequest::Delete { path: path.clone() })
        .unwrap();
    assert!(
        preview
            .diagnostics
            .iter()
            .any(|d| d.code == "unresolved_reference")
    );
    let alias = format!("{BASE}/boards/TARGET.toml");
    let aliases_target = root.path().join(&alias).exists();
    let alias_preview = store
        .preview(ChangeRequest::Delete { path: alias })
        .unwrap();
    assert!(!alias_preview.diagnostics.is_empty());
    if aliases_target {
        assert_eq!(alias_preview.request.path(), path);
    }
    assert!(store.apply(alias_preview).is_err());
    assert!(root.path().join(path).exists());
}
