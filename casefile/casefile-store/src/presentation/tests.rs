use super::*;
use crate::activation::Project;
use std::{collections::BTreeSet, sync::Condvar};
use tempfile::TempDir;

const INVESTIGATION: &str = "projects/demo/investigations/sample";
const TICKET: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
const REVIEW: &str = "projects/demo/investigations/sample/review/round-1.md";
const EVIDENCE: &str = "projects/demo/investigations/sample/evidence/observation.md";
const RAW: &str = "projects/demo/investigations/sample/large.raw";

#[derive(Clone, Debug, Eq, PartialEq)]
enum Operation {
    Activation,
    ReadDir(String),
    Metadata(String),
    Body(String),
}

#[derive(Clone)]
enum FakeNode {
    Directory,
    File { bytes: Vec<u8>, version: u128 },
    Symlink,
}

struct FakeState {
    activation: (ActivationState, Activation, Vec<Diagnostic>),
    nodes: BTreeMap<String, FakeNode>,
    operations: Vec<Operation>,
    fail_reads: BTreeSet<String>,
    blocked_path: Option<String>,
    block_entered: bool,
    block_released: bool,
    next_version: u128,
}

struct FakeReader {
    state: Mutex<FakeState>,
    changed: Condvar,
}

impl FakeReader {
    fn active() -> Arc<Self> {
        let activation = active_activation();
        let reader = Arc::new(Self {
            state: Mutex::new(FakeState {
                activation: (ActivationState::Active, activation, Vec::new()),
                nodes: BTreeMap::new(),
                operations: Vec::new(),
                fail_reads: BTreeSet::new(),
                blocked_path: None,
                block_entered: false,
                block_released: false,
                next_version: 1,
            }),
            changed: Condvar::new(),
        });
        reader.insert_file(
                "casefile.toml",
                b"schema_version = 1\n[projects.demo]\nprefix = 'HMD'\ninvestigations = ['projects/demo/investigations/sample']\n".to_vec(),
            );
        reader.insert_file(
            "projects.toml",
            b"schema_version = 1\n[projects]\ndemo = 'projects/demo'\n".to_vec(),
        );
        reader.insert_file(
                TICKET,
                include_bytes!("../../tests/fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md").to_vec(),
            );
        reader.insert_file(
            REVIEW,
            include_bytes!(
                "../../tests/fixtures/minimum/projects/demo/investigations/sample/review/round-1.md"
            )
            .to_vec(),
        );
        reader.insert_file(
            EVIDENCE,
            b"---\nattachments: []\n---\n\n# Observation\n\nEvidence body.\n".to_vec(),
        );
        reader.insert_file(RAW, vec![b'x'; 1024 * 1024]);
        reader.insert_file(".git/ignored.raw", b"implementation metadata".to_vec());
        reader.insert_file(
            "projects/demo/investigations/sample/.git/visible.raw",
            b"nested same-name content".to_vec(),
        );
        reader.insert_file(
            "projects/demo/investigations/sample/NUL",
            b"unsafe portable handle".to_vec(),
        );
        reader.insert_file("projects/demo/investigations/sample/empty.raw", Vec::new());
        reader
    }

    fn early(state: ActivationState) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(FakeState {
                activation: (
                    state,
                    Activation::default(),
                    (state == ActivationState::Invalid)
                        .then(|| {
                            Diagnostic::new(
                                "casefile.toml",
                                "invalid_activation",
                                "invalid fixture",
                            )
                        })
                        .into_iter()
                        .collect(),
                ),
                nodes: BTreeMap::from([(
                    "ignored.raw".into(),
                    FakeNode::File {
                        bytes: b"must not read".to_vec(),
                        version: 1,
                    },
                )]),
                operations: Vec::new(),
                fail_reads: BTreeSet::new(),
                blocked_path: None,
                block_entered: false,
                block_released: false,
                next_version: 2,
            }),
            changed: Condvar::new(),
        })
    }

    fn many_raw(count: usize) -> Arc<Self> {
        let reader = Self::active();
        for index in 0..count {
            reader.insert_file(
                &format!("{INVESTIGATION}/raw/{index:04}.txt"),
                format!("raw {index}").into_bytes(),
            );
        }
        reader
    }

    fn insert_file(&self, path: &str, bytes: Vec<u8>) {
        let mut state = self.state.lock().expect("fake state");
        insert_parent_directories(&mut state.nodes, path);
        let version = state.next_version;
        state.next_version += 1;
        state
            .nodes
            .insert(path.into(), FakeNode::File { bytes, version });
    }

    fn remove(&self, path: &str) {
        self.state.lock().expect("fake state").nodes.remove(path);
    }

    fn replace(&self, path: &str, bytes: Vec<u8>) {
        self.insert_file(path, bytes);
    }

    fn symlink(&self, path: &str) {
        self.state
            .lock()
            .expect("fake state")
            .nodes
            .insert(path.into(), FakeNode::Symlink);
    }

    fn block(&self, path: &str) {
        let mut state = self.state.lock().expect("fake state");
        state.blocked_path = Some(path.into());
        state.block_entered = false;
        state.block_released = false;
    }

    fn wait_until_blocked(&self) {
        let mut state = self.state.lock().expect("fake state");
        while !state.block_entered {
            state = self.changed.wait(state).expect("fake block wait");
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("fake state");
        state.block_released = true;
        self.changed.notify_all();
    }

    fn operations(&self) -> Vec<Operation> {
        self.state.lock().expect("fake state").operations.clone()
    }
}

impl PresentationReader for FakeReader {
    fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
        let mut state = self.state.lock().expect("fake state");
        state.operations.push(Operation::Activation);
        Ok(state.activation.clone())
    }

    fn read_dir(&self, relative: &str, cancelled: &AtomicBool) -> Result<Vec<String>, StoreError> {
        let mut state = self.state.lock().expect("fake state");
        check_cancelled(cancelled)?;
        state.operations.push(Operation::ReadDir(relative.into()));
        let prefix = if relative.is_empty() {
            String::new()
        } else {
            format!("{relative}/")
        };
        let mut values = state
            .nodes
            .keys()
            .filter_map(|path| {
                let rest = path.strip_prefix(&prefix)?;
                (!rest.is_empty() && !rest.contains('/')).then(|| path.clone())
            })
            .collect::<Vec<_>>();
        values.sort();
        Ok(values)
    }

    fn metadata(&self, relative: &str) -> Result<ReaderMetadata, StoreError> {
        let mut state = self.state.lock().expect("fake state");
        state.operations.push(Operation::Metadata(relative.into()));
        let node = state.nodes.get(relative).cloned().ok_or_else(not_found)?;
        let (kind, length, version) = match node {
            FakeNode::Directory => (PresentationFileKind::Directory, 0, 0),
            FakeNode::File { bytes, version } => {
                (PresentationFileKind::Regular, bytes.len() as u64, version)
            }
            FakeNode::Symlink => (PresentationFileKind::Symlink, 0, 0),
        };
        Ok(ReaderMetadata {
            public: PresentationFileMetadata {
                kind,
                length,
                modified_unix_nanos: Some(version),
                revision: Revision(format!("fake-fsmeta-v1:{version}")),
            },
        })
    }

    fn read(&self, relative: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, StoreError> {
        let mut state = self.state.lock().expect("fake state");
        state.operations.push(Operation::Body(relative.into()));
        if state.fail_reads.contains(relative) {
            return Err(io::Error::other("injected read failure").into());
        }
        if state.blocked_path.as_deref() == Some(relative) {
            state.block_entered = true;
            self.changed.notify_all();
            while !state.block_released {
                state = self.changed.wait(state).expect("fake release wait");
            }
        }
        check_cancelled(cancelled)?;
        match state.nodes.get(relative) {
            Some(FakeNode::File { bytes, .. }) => Ok(bytes.clone()),
            Some(_) => Err(StoreError::Invalid(
                "fake path is not a regular file".into(),
            )),
            None => Err(not_found()),
        }
    }
}

#[test]
fn per_entry_read_failure_is_visible_and_retried_without_a_metadata_change() {
    let reader = FakeReader::active();
    reader
        .state
        .lock()
        .unwrap()
        .fail_reads
        .insert(TICKET.into());
    let session = PresentationSession::with_reader(reader.clone());
    let failed = drain(
        session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap(),
    );
    assert!(matches!(
        failed.last(),
        Some(PresentationEvent::Complete { .. })
    ));
    let entries = event_entries(&failed);
    assert!(
        matches!(&entries.iter().find(|entry| entry.path == TICKET).unwrap().diagnostics, PresentationFact::Available(diagnostics) if diagnostics.iter().any(|diagnostic| diagnostic.code == "presentation_read"))
    );
    reader.state.lock().unwrap().fail_reads.clear();
    let recovered = event_entries(&drain(
        session
            .load(load_request(2, PresentationTarget::Store))
            .unwrap(),
    ));
    assert!(matches!(
        recovered
            .iter()
            .find(|entry| entry.path == TICKET)
            .unwrap()
            .classification,
        PresentationFact::Available(Classification::Governed)
    ));
    assert_eq!(body_reads(&reader, TICKET), 2);
}

#[test]
fn live_save_replacement_and_deletion_are_reconciled_per_entry() {
    for mutation in ["save", "replace", "remove", "error"] {
        let reader = FakeReader::active();
        reader.block(TICKET);
        let session = PresentationSession::with_reader(reader.clone());
        let stream = session
            .load(load_request(1, PresentationTarget::Store))
            .expect("load");
        assert!(matches!(
            stream.recv().expect("catalogue"),
            PresentationEvent::Catalogue { .. }
        ));
        reader.wait_until_blocked();
        match mutation {
            "save" | "replace" => {
                let bytes = include_bytes!(
                    "../../tests/fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md"
                );
                let mut changed = bytes.to_vec();
                changed.extend_from_slice(b"\n<!-- saved -->\n");
                reader.replace(TICKET, changed);
            }
            "remove" => reader.remove(TICKET),
            "error" => reader.symlink(TICKET),
            _ => unreachable!(),
        }
        reader.release();
        let events = drain(stream);
        assert!(matches!(
            events.last(),
            Some(PresentationEvent::Complete { .. })
        ));
        let entries = event_entries(&events);
        assert!(entries.iter().any(|entry| entry.path == REVIEW));
        let ticket = entries.iter().find(|entry| entry.path == TICKET);
        match mutation {
            "save" | "replace" => assert!(
                matches!(&ticket.expect("updated ticket").body, PresentationFact::Available(bytes) if bytes.ends_with(b"<!-- saved -->\n"))
            ),
            "remove" => assert!(ticket.is_none()),
            "error" => assert!(
                matches!(&ticket.expect("failed row").diagnostics, PresentationFact::Available(diagnostics) if !diagnostics.is_empty())
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn unchanged_refresh_reuses_records_without_reading_bodies_and_detail_reads_only_selection() {
    let (reader, session, entries) = loaded_fake();
    let reads = reader
        .operations()
        .iter()
        .filter(|op| matches!(op, Operation::Body(_)))
        .count();
    drain(
        session
            .load(load_request(2, PresentationTarget::Store))
            .expect("refresh"),
    );
    assert_eq!(
        reader
            .operations()
            .iter()
            .filter(|op| matches!(op, Operation::Body(_)))
            .count(),
        reads
    );
    assert_eq!(body_reads(&reader, RAW), 0);
    assert_eq!(body_reads(&reader, EVIDENCE), 0);
    let handle = entries
        .iter()
        .find(|entry| entry.path == RAW)
        .unwrap()
        .content_handle
        .clone()
        .unwrap();
    reader.replace(RAW, b"saved content".to_vec());
    let content = drain_content(
        session
            .fetch_content(content_request(3, handle))
            .expect("content"),
    );
    assert!(
        matches!(&content[1], PresentationContentEvent::Loaded { entry, .. } if entry.body == PresentationFact::Available(b"saved content".to_vec()))
    );
    assert_eq!(body_reads(&reader, RAW), 1);
    assert_eq!(body_reads(&reader, EVIDENCE), 0);
}

#[test]
fn lazy_content_rejects_non_emitted_escaping_and_excluded_paths() {
    let (_, session, entries) = loaded_fake();
    for path in ["../escape", ".git/config", "not-emitted.txt"] {
        let stream = session
            .fetch_content(PresentationContentRequest {
                generation: 12,
                target: PresentationTarget::Store,
                selector: PresentationContentSelector::Path { path: path.into() },
            })
            .expect("content failure stream");
        let events = drain_content(stream);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PresentationContentEvent::Failure { .. }
        ));
    }

    let foreign = entries
        .iter()
        .find(|entry| entry.path == RAW)
        .expect("raw")
        .content_handle
        .clone()
        .expect("handle");
    let (_, other_session, _) = loaded_fake();
    let events = drain_content(
        other_session
            .fetch_content(content_request(12, foreign))
            .expect("foreign content failure"),
    );
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        PresentationContentEvent::Failure { .. }
    ));
}

#[cfg(unix)]
#[test]
fn production_fetch_rejects_a_catalogued_path_whose_parent_becomes_a_symlink() {
    let root = fixture();
    let raw_path = root.path().join(INVESTIGATION).join("lazy.raw");
    fs::write(&raw_path, "original").expect("raw");
    let store = crate::Store::open(root.path()).expect("store");
    let session = store.presentation_session();
    let entries = event_entries(&drain(
        session
            .load(load_request(13, PresentationTarget::Store))
            .expect("load"),
    ));
    let handle = entries
        .iter()
        .find(|entry| entry.path.ends_with("lazy.raw"))
        .expect("lazy entry")
        .content_handle
        .clone()
        .expect("handle");

    let investigation = root.path().join(INVESTIGATION);
    let moved = root.path().join("moved-investigation");
    fs::rename(&investigation, &moved).expect("move investigation");
    let outside = TempDir::new().expect("outside");
    fs::write(outside.path().join("lazy.raw"), "escaped").expect("outside raw");
    std::os::unix::fs::symlink(outside.path(), &investigation).expect("swap parent");

    let events = drain_content(
        session
            .fetch_content(content_request(13, handle))
            .expect("content"),
    );
    assert!(matches!(
        events[0],
        PresentationContentEvent::Pending { .. }
    ));
    assert!(matches!(
        events[1],
        PresentationContentEvent::Failure { .. }
    ));
}

#[cfg(unix)]
#[test]
fn production_catalogue_and_fetch_reject_a_root_renamed_to_a_symlink() {
    let root = fixture();
    fs::write(root.path().join(INVESTIGATION).join("lazy.raw"), "original").expect("raw");
    let store = crate::Store::open(root.path()).expect("store");
    let session = store.presentation_session();
    let entries = event_entries(&drain(
        session
            .load(load_request(14, PresentationTarget::Store))
            .expect("load"),
    ));
    let handle = entries
        .iter()
        .find(|entry| entry.path.ends_with("lazy.raw"))
        .expect("lazy entry")
        .content_handle
        .clone()
        .expect("handle");

    let moved = root.path().with_extension("hmd-047-moved");
    let outside = TempDir::new().expect("outside");
    fs::rename(root.path(), &moved).expect("move root");
    std::os::unix::fs::symlink(outside.path(), root.path()).expect("swap root");

    let content = drain_content(
        session
            .fetch_content(content_request(14, handle))
            .expect("content"),
    );
    assert!(matches!(
        content[0],
        PresentationContentEvent::Pending { .. }
    ));
    assert!(matches!(
        content[1],
        PresentationContentEvent::Failure { .. }
    ));
    let catalogue = drain(
        session
            .load(load_request(15, PresentationTarget::Store))
            .expect("catalogue failure stream"),
    );
    assert_eq!(catalogue.len(), 1);
    assert!(matches!(
        catalogue[0],
        PresentationEvent::Failure {
            ref coverage,
            ..
        } if coverage.catalogue == PresentationCoverageState::Pending
    ));

    fs::remove_file(root.path()).expect("remove root symlink");
    fs::rename(moved, root.path()).expect("restore root");
}

#[test]
fn unactivated_and_invalid_activation_complete_before_filesystem_catalogue_reads() {
    for state in [ActivationState::Unactivated, ActivationState::Invalid] {
        let reader = FakeReader::early(state);
        let session = PresentationSession::with_reader(reader.clone());
        let events = drain(
            session
                .load(load_request(3, PresentationTarget::Store))
                .expect("early load"),
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            PresentationEvent::Catalogue { catalogue, .. } => {
                assert_eq!(catalogue.activation, state);
                assert!(catalogue.projects.is_empty());
                assert_eq!(
                    catalogue.diagnostics.is_empty(),
                    state == ActivationState::Unactivated
                );
            }
            other => panic!("unexpected early event: {other:?}"),
        }
        assert!(matches!(events[1], PresentationEvent::Complete { .. }));
        assert_eq!(reader.operations(), vec![Operation::Activation]);
    }
}

#[test]
fn batches_and_channels_are_bounded_and_cancellation_stops_backpressure() {
    let reader =
        FakeReader::many_raw(PRESENTATION_BATCH_LIMIT * (PRESENTATION_CHANNEL_CAPACITY + 1));
    let session = PresentationSession::with_reader(reader);
    let stream = session
        .load(load_request(15, PresentationTarget::Store))
        .expect("load");

    while stream.try_recv().is_err() {
        thread::yield_now();
    }
    while stream.try_recv().is_err() {
        thread::yield_now();
    }
    stream.cancel();
    while !stream.is_finished() {
        thread::yield_now();
    }
    let queued = std::iter::from_fn(|| stream.try_recv().ok()).collect::<Vec<_>>();
    assert!(queued.len() <= PRESENTATION_CHANNEL_CAPACITY);
    assert!(queued.iter().all(|event| {
            !matches!(event, PresentationEvent::Entries { entries, .. } if entries.len() > PRESENTATION_BATCH_LIMIT)
        }));
}

#[test]
fn scoped_targets_emit_only_their_deterministic_activated_subtrees() {
    let reader = FakeReader::active();
    for target in [
        PresentationTarget::Project {
            project: "demo".into(),
        },
        PresentationTarget::Investigation {
            project: "demo".into(),
            path: INVESTIGATION.into(),
        },
    ] {
        let paths = |generation| {
            let session = PresentationSession::with_reader(reader.clone());
            let events = drain(
                session
                    .load(load_request(generation, target.clone()))
                    .expect("scoped load"),
            );
            assert!(events.iter().all(|event| event.target() == &target));
            event_entries(&events)
                .into_iter()
                .map(|entry| entry.path)
                .collect::<Vec<_>>()
        };
        let first = paths(30);
        let second = paths(31);
        assert_eq!(first, second);
        assert!(first.iter().all(|path| target_contains(&target, path)));
        assert!(first.iter().any(|path| path == TICKET));
    }
}

#[test]
fn scoped_load_keeps_untouched_target_content_handles_usable() {
    let reader = FakeReader::active();
    let session = PresentationSession::with_reader(reader);
    let store_entries = event_entries(&drain(
        session
            .load(load_request(32, PresentationTarget::Store))
            .expect("store load"),
    ));
    let handle = store_entries
        .iter()
        .find(|entry| entry.path == RAW)
        .expect("raw")
        .content_handle
        .clone()
        .expect("handle");
    drain(
        session
            .load(load_request(
                33,
                PresentationTarget::Investigation {
                    project: "demo".into(),
                    path: INVESTIGATION.into(),
                },
            ))
            .expect("scoped load"),
    );

    let events = drain_content(
        session
            .fetch_content(content_request(32, handle))
            .expect("untouched content"),
    );
    assert!(matches!(events[1], PresentationContentEvent::Loaded { .. }));
}

#[test]
fn loaded_presentation_facts_match_complete_canonical_inputs() {
    let root = fixture();
    let store = crate::Store::open(root.path()).expect("store");
    let canonical = store.scan().expect("canonical scan");
    let derived = store.derive_snapshot(&canonical);
    let session = store.presentation_session();
    let events = drain(
        session
            .load(load_request(22, PresentationTarget::Store))
            .expect("presentation load"),
    );
    let entries = event_entries(&events);
    let catalogue = match &events[0] {
        PresentationEvent::Catalogue { catalogue, .. } => catalogue,
        other => panic!("catalogue was not first: {other:?}"),
    };
    assert_eq!(catalogue.activation, canonical.activation);
    assert_eq!(catalogue.projects[0].slug, "demo");
    let encoded = serde_json::to_string(&events).expect("presentation JSON");
    for forbidden in ["source_revision", "original_bytes", "snapshot"] {
        assert!(!encoded.contains(forbidden), "leaked {forbidden}");
    }

    for expected in &canonical.snapshot.entries {
        let actual = entries
            .iter()
            .rev()
            .find(|entry| entry.path == expected.path)
            .unwrap_or_else(|| panic!("missing presentation entry {}", expected.path));
        assert_eq!(actual.path, expected.path);
        assert_eq!(actual.kind, expected.kind);

        let expected_scope =
            canonical
                .scope_for_path(&expected.path)
                .map(|(project, investigation)| PresentationScope {
                    project: project.into(),
                    investigation: investigation.map(Into::into),
                });
        assert_eq!(actual.scope, expected_scope);
        if expected.kind == Some(Kind::Evidence) {
            assert_eq!(actual.classification, PresentationFact::Unavailable);
            assert_eq!(actual.summary, PresentationFact::Unavailable);
            assert_eq!(actual.body, PresentationFact::Unavailable);
            continue;
        }
        assert_eq!(
            actual.classification,
            PresentationFact::Available(expected.classification)
        );
        assert_eq!(
            actual.identity,
            PresentationFact::Available(expected.identity.clone())
        );
        if expected.kind.is_none() {
            assert_eq!(actual.body, PresentationFact::Unavailable);
            continue;
        }
        assert!(
            matches!(&actual.summary, PresentationFact::Available(summary) if summary.as_ref().map(|value| &value.record) == expected.summary.as_ref())
        );
        assert_eq!(
            actual.body,
            PresentationFact::Available(expected.original_bytes.clone())
        );
        let expected_diagnostics = canonical
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.path == expected.path)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            actual.diagnostics,
            PresentationFact::Available(expected_diagnostics)
        );
        let record = derived
            .records
            .iter()
            .find(|record| record.path == expected.path)
            .expect("derived record");
        assert_eq!(
            actual.progress,
            PresentationFact::Available(record.progress.clone())
        );
        if record.work_item.as_ref().is_none_or(|item| {
            item.decision_refs.is_empty()
                && item.related_tickets.is_empty()
                && item.supersedes.is_empty()
                && item.superseded_by.is_empty()
        }) {
            assert_eq!(
                actual.relationships,
                PresentationFact::Available(Vec::new())
            );
        }
        let boards = record
            .identity
            .as_ref()
            .map(|identity| {
                derived
                    .boards
                    .iter()
                    .filter(|board| &board.identity == identity)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(actual.boards, PresentationFact::Available(boards));
    }

    let evidence = entries
        .iter()
        .find(|entry| entry.kind == Some(Kind::Evidence))
        .expect("evidence");
    let content = drain_content(
        session
            .fetch_content(content_request(
                22,
                evidence.content_handle.clone().expect("evidence handle"),
            ))
            .expect("evidence fetch"),
    );
    let PresentationContentEvent::Loaded { entry, .. } = &content[1] else {
        panic!("evidence was not loaded");
    };
    let expected = canonical
        .snapshot
        .entries
        .iter()
        .find(|expected| expected.path == entry.path)
        .expect("canonical evidence");
    assert_eq!(
        entry.classification,
        PresentationFact::Available(expected.classification)
    );
    assert!(
        matches!(&entry.summary, PresentationFact::Available(summary) if summary.as_ref().map(|value| &value.record) == expected.summary.as_ref())
    );
    assert_eq!(
        entry.body,
        PresentationFact::Available(expected.original_bytes.clone())
    );
    let expected_diagnostics = canonical
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path == expected.path)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        entry.diagnostics,
        PresentationFact::Available(expected_diagnostics)
    );
}

fn active_activation() -> Activation {
    Activation {
        schema_version: Some(1),
        projects: BTreeMap::from([(
            "demo".into(),
            Project {
                prefix: "HMD".into(),
                investigations: vec![INVESTIGATION.into()],
            },
        )]),
    }
}

fn insert_parent_directories(nodes: &mut BTreeMap<String, FakeNode>, path: &str) {
    let mut parent = path;
    while let Some((value, _)) = parent.rsplit_once('/') {
        nodes.entry(value.into()).or_insert(FakeNode::Directory);
        parent = value;
    }
}

fn not_found() -> StoreError {
    io::Error::new(io::ErrorKind::NotFound, "fake path missing").into()
}

fn load_request(generation: u64, target: PresentationTarget) -> PresentationLoadRequest {
    PresentationLoadRequest { generation, target }
}

fn content_request(
    generation: u64,
    handle: PresentationContentHandle,
) -> PresentationContentRequest {
    PresentationContentRequest {
        generation,
        target: PresentationTarget::Store,
        selector: PresentationContentSelector::Handle { handle },
    }
}

fn drain(stream: PresentationStream) -> Vec<PresentationEvent> {
    let mut events = Vec::new();
    while let Ok(event) = stream.recv() {
        let finished = matches!(
            event,
            PresentationEvent::Complete { .. } | PresentationEvent::Failure { .. }
        );
        events.push(event);
        if finished {
            break;
        }
    }
    events
}

fn drain_content(stream: PresentationContentStream) -> Vec<PresentationContentEvent> {
    let mut events = Vec::new();
    while let Ok(event) = stream.recv() {
        let finished = matches!(
            event,
            PresentationContentEvent::Loaded { .. } | PresentationContentEvent::Failure { .. }
        );
        events.push(event);
        if finished {
            break;
        }
    }
    events
}

fn event_entries(events: &[PresentationEvent]) -> Vec<PresentationEntry> {
    events
        .iter()
        .filter_map(|event| match event {
            PresentationEvent::Entries { entries, .. } => Some(entries.clone()),
            _ => None,
        })
        .flatten()
        .map(|entry| entry.as_ref().clone())
        .collect()
}

fn loaded_fake() -> (Arc<FakeReader>, PresentationSession, Vec<PresentationEntry>) {
    let reader = FakeReader::active();
    let session = PresentationSession::with_reader(reader.clone());
    let events = drain(
        session
            .load(load_request(7, PresentationTarget::Store))
            .expect("load"),
    );
    (reader, session, event_entries(&events))
}

fn body_reads(reader: &FakeReader, path: &str) -> usize {
    reader
        .operations()
        .iter()
        .filter(|operation| matches!(operation, Operation::Body(value) if value == path))
        .count()
}

fn fixture() -> TempDir {
    let temporary = TempDir::new().expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimum")
            .as_path(),
        temporary.path(),
    );
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

#[test]
fn production_saves_atomic_replacements_and_deletions_after_enumeration_stay_local() {
    use std::sync::Barrier;
    struct PausedReader {
        reader: FsPresentationReader,
        entered: Barrier,
        resume: Barrier,
    }
    impl PresentationReader for PausedReader {
        fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
            self.reader.activation()
        }
        fn read_dir(&self, path: &str, cancelled: &AtomicBool) -> Result<Vec<String>, StoreError> {
            self.reader.read_dir(path, cancelled)
        }
        fn metadata(&self, path: &str) -> Result<ReaderMetadata, StoreError> {
            self.reader.metadata(path)
        }
        fn read(&self, path: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, StoreError> {
            if path == TICKET {
                self.entered.wait();
                self.resume.wait();
            }
            self.reader.read(path, cancelled)
        }
    }
    for change in ["save", "replace", "remove"] {
        let root = fixture();
        let ticket = root.path().join(TICKET);
        let mut replacement = fs::read(&ticket).unwrap();
        replacement.extend_from_slice(b"\n<!-- live edit -->\n");
        let reader = Arc::new(PausedReader {
            reader: FsPresentationReader {
                root: root.path().to_path_buf(),
            },
            entered: Barrier::new(2),
            resume: Barrier::new(2),
        });
        let session = PresentationSession::with_reader(reader.clone());
        let stream = session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap();
        reader.entered.wait();
        match change {
            "save" => fs::write(&ticket, &replacement).unwrap(),
            "replace" => {
                let saved = ticket.with_extension("saved");
                fs::write(&saved, &replacement).unwrap();
                fs::rename(saved, &ticket).unwrap();
            }
            "remove" => fs::remove_file(&ticket).unwrap(),
            _ => unreachable!(),
        }
        reader.resume.wait();
        let events = drain(stream);
        assert!(matches!(
            events.last(),
            Some(PresentationEvent::Complete { .. })
        ));
        let entries = event_entries(&events);
        assert!(entries.iter().any(|entry| entry.path == REVIEW));
        if change == "remove" {
            assert!(!entries.iter().any(|entry| entry.path == TICKET));
        } else {
            assert_eq!(
                entries
                    .iter()
                    .find(|entry| entry.path == TICKET)
                    .unwrap()
                    .body,
                PresentationFact::Available(replacement)
            );
        }
    }
}

struct PausedRead {
    reader: Arc<FakeReader>,
    path: String,
    first: AtomicBool,
    gate: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl PausedRead {
    fn new(reader: Arc<FakeReader>, path: &str) -> Arc<Self> {
        Arc::new(Self {
            reader,
            path: path.into(),
            first: AtomicBool::new(true),
            gate: Mutex::new((false, false)),
            changed: Condvar::new(),
        })
    }
    fn wait(&self) {
        let mut gate = self.gate.lock().unwrap();
        while !gate.0 {
            gate = self.changed.wait(gate).unwrap();
        }
    }
    fn release(&self) {
        self.gate.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

impl PresentationReader for PausedRead {
    fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
        self.reader.activation()
    }
    fn read_dir(&self, path: &str, cancelled: &AtomicBool) -> Result<Vec<String>, StoreError> {
        self.reader.read_dir(path, cancelled)
    }
    fn metadata(&self, path: &str) -> Result<ReaderMetadata, StoreError> {
        self.reader.metadata(path)
    }
    fn read(&self, path: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, StoreError> {
        let bytes = self.reader.read(path, cancelled)?;
        if path == self.path && self.first.swap(false, Ordering::AcqRel) {
            let mut gate = self.gate.lock().unwrap();
            gate.0 = true;
            self.changed.notify_all();
            while !gate.1 {
                gate = self.changed.wait(gate).unwrap();
            }
        }
        Ok(bytes)
    }
}

#[test]
fn newer_nonmonotonic_load_wins_and_late_old_events_cannot_replace_handles() {
    let reader = FakeReader::active();
    let paused = PausedRead::new(reader.clone(), TICKET);
    let session = PresentationSession::with_reader(paused.clone());
    let old = session
        .load(load_request(99, PresentationTarget::Store))
        .unwrap();
    assert!(matches!(
        old.recv().unwrap(),
        PresentationEvent::Catalogue { .. }
    ));
    paused.wait();
    let source = match &reader.state.lock().unwrap().nodes[TICKET] {
        FakeNode::File { bytes, .. } => bytes.clone(),
        _ => unreachable!(),
    };
    reader.replace(TICKET, [source, b"\n<!-- new load -->\n".to_vec()].concat());
    let new = event_entries(&drain(
        session
            .load(load_request(2, PresentationTarget::Store))
            .unwrap(),
    ));
    let handle = new
        .iter()
        .find(|entry| entry.path == RAW)
        .unwrap()
        .content_handle
        .clone()
        .unwrap();
    paused.release();
    assert!(!drain(old).iter().any(|event| {
        match event {
            PresentationEvent::Entries { entries, .. } => entries
                .iter()
                .any(|entry| entry.path.starts_with(INVESTIGATION)),
            PresentationEvent::Complete { .. } => true,
            _ => false,
        }
    }));
    let final_entries = event_entries(&drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    ));
    assert!(
        matches!(&final_entries.iter().find(|entry| entry.path == TICKET).unwrap().body, PresentationFact::Available(bytes) if bytes.ends_with(b"<!-- new load -->\n"))
    );
    assert_eq!(
        final_entries
            .iter()
            .find(|entry| entry.path == RAW)
            .unwrap()
            .content_handle
            .as_ref(),
        Some(&handle)
    );
    assert!(matches!(
        drain_content(session.fetch_content(content_request(0, handle)).unwrap()).last(),
        Some(PresentationContentEvent::Loaded { .. })
    ));
}

#[test]
fn disjoint_target_loads_do_not_cancel_each_other_or_invalidate_received_handles() {
    let reader = FakeReader::active();
    let other = "projects/demo/investigations/other";
    reader
        .state
        .lock()
        .unwrap()
        .activation
        .1
        .projects
        .get_mut("demo")
        .unwrap()
        .investigations
        .push(other.into());
    let other_raw = format!("{other}/kept.raw");
    reader.insert_file(&other_raw, b"other content".to_vec());
    let paused = PausedRead::new(reader.clone(), TICKET);
    let session = PresentationSession::with_reader(paused.clone());
    let first_target = PresentationTarget::Investigation {
        project: "demo".into(),
        path: INVESTIGATION.into(),
    };
    let other_target = PresentationTarget::Investigation {
        project: "demo".into(),
        path: other.into(),
    };
    let first = session.load(load_request(99, first_target)).unwrap();
    first.recv().unwrap();
    paused.wait();
    let received = event_entries(&drain(
        session.load(load_request(1, other_target.clone())).unwrap(),
    ));
    let handle = received
        .iter()
        .find(|entry| entry.path == other_raw)
        .unwrap()
        .content_handle
        .clone()
        .unwrap();
    paused.release();
    assert!(matches!(
        drain(first).last(),
        Some(PresentationEvent::Complete { .. })
    ));
    let request = PresentationContentRequest {
        generation: 0,
        target: other_target,
        selector: PresentationContentSelector::Handle { handle },
    };
    assert!(
        matches!(drain_content(session.fetch_content(request).unwrap()).last(), Some(PresentationContentEvent::Loaded { entry, .. }) if matches!(&entry.body, PresentationFact::Available(bytes) if bytes == b"other content"))
    );
}

#[test]
fn cancellation_after_the_final_read_does_not_publish_or_cache_that_scope() {
    let reader = FakeReader::active();
    let paused = PausedRead::new(reader.clone(), TICKET);
    let session = PresentationSession::with_reader(paused.clone());
    let target = PresentationTarget::Investigation {
        project: "demo".into(),
        path: INVESTIGATION.into(),
    };
    let stream = session.load(load_request(1, target.clone())).unwrap();
    stream.recv().unwrap();
    paused.wait();
    stream.cancel();
    paused.release();
    assert!(!drain(stream).iter().any(|event| matches!(
        event,
        PresentationEvent::Entries { .. } | PresentationEvent::Complete { .. }
    )));
    reader
        .state
        .lock()
        .unwrap()
        .fail_reads
        .insert(TICKET.into());
    let next = event_entries(&drain(session.load(load_request(0, target)).unwrap()));
    assert!(
        matches!(&next.iter().find(|entry| entry.path == TICKET).unwrap().diagnostics, PresentationFact::Available(diagnostics) if diagnostics.iter().any(|diagnostic| diagnostic.code == "presentation_read"))
    );
}

#[test]
fn interrupted_reads_retry_but_cancellation_never_returns_partial_content() {
    struct InterruptedThenCancel {
        cancelled: Arc<AtomicBool>,
        interrupted: bool,
    }
    impl Read for InterruptedThenCancel {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            bytes.fill(b'x');
            self.cancelled.store(true, Ordering::Release);
            Ok(bytes.len())
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut reader = InterruptedThenCancel {
        cancelled: cancelled.clone(),
        interrupted: false,
    };
    assert!(matches!(
        read_cancellable(&mut reader, &cancelled),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn lazy_handles_survive_unrelated_edits_but_changed_and_deleted_paths_are_invalidated() {
    let reader = FakeReader::active();
    let session = PresentationSession::with_reader(reader.clone());
    let first = event_entries(&drain(
        session
            .load(load_request(10, PresentationTarget::Store))
            .unwrap(),
    ));
    let handle = |entries: &[PresentationEntry], path: &str| {
        entries
            .iter()
            .find(|entry| entry.path == path)
            .unwrap()
            .content_handle
            .clone()
            .unwrap()
    };
    let raw = handle(&first, RAW);
    let evidence = handle(&first, EVIDENCE);
    reader.insert_file(
        "projects/demo/investigations/sample/elsewhere.raw",
        b"new unrelated".to_vec(),
    );
    let second = event_entries(&drain(
        session
            .load(load_request(2, PresentationTarget::Store))
            .unwrap(),
    ));
    assert_eq!(handle(&second, RAW), raw);
    assert_eq!(handle(&second, EVIDENCE), evidence);
    reader.replace(RAW, b"changed raw".to_vec());
    reader.remove(EVIDENCE);
    let third = event_entries(&drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    ));
    assert_ne!(handle(&third, RAW), raw);
    assert!(!third.iter().any(|entry| entry.path == EVIDENCE));
    for old in [raw, evidence] {
        assert!(matches!(
            drain_content(session.fetch_content(content_request(0, old)).unwrap()).last(),
            Some(PresentationContentEvent::Failure { .. })
        ));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn unrepresentable_native_names_fail_explicitly_instead_of_aliasing_a_replacement_name() {
    use std::os::unix::ffi::OsStringExt;
    let root = fixture();
    let path = root.path().join(INVESTIGATION);
    fs::write(
        path.join(std::ffi::OsString::from_vec(b"bad-\xff.raw".to_vec())),
        b"native one",
    )
    .unwrap();
    fs::write(
        path.join(std::ffi::OsString::from_vec(b"bad-\xfe.raw".to_vec())),
        b"native two",
    )
    .unwrap();
    fs::write(path.join("bad-�.raw"), b"different legitimate UTF-8 file").unwrap();
    let session = crate::Store::open(root.path())
        .unwrap()
        .presentation_session();
    let events = drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    );
    assert!(
        matches!(events.last(), Some(PresentationEvent::Failure { message, .. }) if message.contains("not UTF-8"))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PresentationEvent::Complete { .. }))
    );
}

#[test]
fn one_ticket_membership_edit_updates_progress_diagnostics_and_shares_unrelated_entries() {
    let reader = FakeReader::active();
    let log = format!("{INVESTIGATION}/progress/log.toml");
    reader.insert_file(&log, b"schema_version=1\n[[entries]]\nid='start'\nrecorded_at='2026-09-30T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-011'\nkind='transition'\nfrom='unknown'\nto='in_progress'\n".to_vec());
    let session = PresentationSession::with_reader(reader.clone());
    let first = drain(
        session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap(),
    );
    let find = |events: &[PresentationEvent], path: &str| {
        events
            .iter()
            .find_map(|event| match event {
                PresentationEvent::Entries { entries, .. } => {
                    entries.iter().find(|entry| entry.path == path).cloned()
                }
                _ => None,
            })
            .unwrap()
    };
    let ticket = find(&first, TICKET);
    assert!(
        matches!(&ticket.progress, PresentationFact::Available(Some(progress)) if progress.status == casefile_core::ProgressStatus::InProgress)
    );
    let review = find(&first, REVIEW);
    let evidence = find(&first, EVIDENCE).content_handle.clone().unwrap();
    let source = match &reader.state.lock().unwrap().nodes[TICKET] {
        FakeNode::File { bytes, .. } => bytes.clone(),
        _ => unreachable!(),
    };
    let provisional = TICKET.replace("/accepted/", "/provisional/");
    reader.remove(TICKET);
    reader.insert_file(
        &provisional,
        String::from_utf8(source)
            .unwrap()
            .replace("status: accepted", "status: provisional")
            .into_bytes(),
    );
    let second = drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    );
    assert!(
        matches!(&find(&second, &log).diagnostics, PresentationFact::Available(diagnostics) if diagnostics.iter().any(|diagnostic| diagnostic.code == "invalid_progress_ticket"))
    );
    assert_eq!(
        find(&second, &provisional).progress,
        PresentationFact::Available(None)
    );
    assert!(
        Arc::ptr_eq(&review, &find(&second, REVIEW)),
        "unchanged public immutable entry must be shared"
    );
    assert_eq!(
        find(&second, EVIDENCE).content_handle.as_ref(),
        Some(&evidence)
    );
}

#[test]
fn nested_activation_uses_deepest_scope_and_membership_change_reclassifies_cached_paths() {
    let reader = FakeReader::active();
    let nested = format!("{INVESTIGATION}/nested");
    reader
        .state
        .lock()
        .unwrap()
        .activation
        .1
        .projects
        .get_mut("demo")
        .unwrap()
        .investigations
        .push(nested.clone());
    let ticket = format!("{nested}/tickets/accepted/HMD-012.md");
    let source = include_bytes!(
        "../../tests/fixtures/minimum/projects/demo/investigations/sample/tickets/accepted/HMD-011.md"
    );
    reader.insert_file(
        &ticket,
        String::from_utf8(source.to_vec())
            .unwrap()
            .replace("HMD-011", "HMD-012")
            .replace("investigation: sample", "investigation: sample/nested")
            .into_bytes(),
    );
    let session = PresentationSession::with_reader(reader.clone());
    let entries = event_entries(&drain(
        session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap(),
    ));
    let child = entries.iter().find(|entry| entry.path == ticket).unwrap();
    assert_eq!(
        child.scope.as_ref().unwrap().investigation.as_deref(),
        Some("sample/nested")
    );
    assert_eq!(child.kind, Some(Kind::Ticket));
    reader
        .state
        .lock()
        .unwrap()
        .activation
        .1
        .projects
        .get_mut("demo")
        .unwrap()
        .investigations
        .retain(|scope| scope != &nested);
    let next = event_entries(&drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    ));
    let child = next.iter().find(|entry| entry.path == ticket).unwrap();
    assert_eq!(
        child.scope.as_ref().unwrap().investigation.as_deref(),
        Some("sample")
    );
    assert_eq!(child.kind, None);
    assert_eq!(child.body, PresentationFact::Unavailable);
}

#[cfg(unix)]
#[test]
fn full_refresh_after_ancestor_replacement_reacquires_descriptors_without_an_anchoring_promise() {
    let root = fixture();
    let path = root.path().join(INVESTIGATION);
    fs::write(path.join("lazy.raw"), b"old source").unwrap();
    let store = crate::Store::open(root.path()).unwrap();
    let session = store.presentation_session();
    let entries = event_entries(&drain(
        session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap(),
    ));
    let old = entries
        .iter()
        .find(|entry| entry.path.ends_with("lazy.raw"))
        .unwrap()
        .content_handle
        .clone()
        .unwrap();
    let archived = TempDir::new().unwrap();
    let moved = archived.path().join("moved-parent");
    fs::rename(&path, &moved).unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("lazy.raw"), b"outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), &path).unwrap();
    assert!(matches!(
        drain_content(
            session
                .fetch_content(content_request(1, old.clone()))
                .unwrap()
        )
        .last(),
        Some(PresentationContentEvent::Failure { .. })
    ));
    fs::remove_file(&path).unwrap();
    fs::create_dir_all(&path).unwrap();
    copy_tree(&moved, &path);
    fs::write(path.join("lazy.raw"), b"replacement inside root").unwrap();
    let refreshed = event_entries(&drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    ));
    let current = refreshed
        .iter()
        .find(|entry| entry.path.ends_with("lazy.raw"))
        .unwrap()
        .content_handle
        .clone()
        .unwrap();
    assert_ne!(current, old);
    assert!(matches!(
        drain_content(session.fetch_content(content_request(0, old)).unwrap()).last(),
        Some(PresentationContentEvent::Failure { .. })
    ));
    assert!(
        matches!(drain_content(session.fetch_content(content_request(0, current)).unwrap()).last(), Some(PresentationContentEvent::Loaded { entry, .. }) if matches!(&entry.body, PresentationFact::Available(bytes) if bytes == b"replacement inside root"))
    );
    let new_session = store.presentation_session();
    assert!(matches!(
        drain(
            new_session
                .load(load_request(0, PresentationTarget::Store))
                .unwrap()
        )
        .last(),
        Some(PresentationEvent::Complete { .. })
    ));
}

#[test]
fn note_only_edit_shares_board_entry_but_transition_updates_cards() {
    let reader = FakeReader::active();
    let board = format!("{INVESTIGATION}/boards/progress.toml");
    reader.insert_file(&board, b"schema_version=1\nid='HMD-progress'\ntitle='Progress'\nstatus_source='progress'\n[[columns]]\nname='Active'\nstatuses=['in_progress']\n[[columns]]\nname='Done'\nstatuses=['complete']\n".to_vec());
    let log = format!("{INVESTIGATION}/progress/log.toml");
    let start = "schema_version=1\n[[entries]]\nid='start'\nrecorded_at='2026-09-30T10:00:00Z'\nrecorded_by='root'\nticket_id='HMD-011'\nkind='transition'\nfrom='unknown'\nto='in_progress'\n";
    reader.insert_file(&log, start.as_bytes().to_vec());
    let session = PresentationSession::with_reader(reader.clone());
    let find = |events: &[PresentationEvent], path: &str| {
        events
            .iter()
            .find_map(|event| match event {
                PresentationEvent::Entries { entries, .. } => {
                    entries.iter().find(|entry| entry.path == path).cloned()
                }
                _ => None,
            })
            .unwrap()
    };
    let first = drain(
        session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap(),
    );
    let old_board = find(&first, &board);
    assert!(
        matches!(&old_board.boards, PresentationFact::Available(boards)
        if boards[0].columns[0].cards[0].identity.identity == "HMD-011"
        && boards[0].columns[1].cards.is_empty())
    );
    let note = format!(
        "{start}[[entries]]\nid='note'\nrecorded_at='2026-09-30T10:01:00Z'\nrecorded_by='root'\nticket_id='HMD-011'\nkind='note'\ncategory='quirk'\nmessage='Keep this message'\n"
    );
    reader.replace(&log, note.as_bytes().to_vec());
    let second = drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    );
    assert!(Arc::ptr_eq(&old_board, &find(&second, &board)));
    assert!(
        matches!(&find(&second, TICKET).progress, PresentationFact::Available(Some(progress))
        if progress.notes.len() == 1 && progress.notes[0].message == "Keep this message")
    );
    reader.replace(&log, format!("{note}[[entries]]\nid='done'\nrecorded_at='2026-09-30T10:02:00Z'\nrecorded_by='root'\nticket_id='HMD-011'\nkind='transition'\nfrom='in_progress'\nto='complete'\n").into_bytes());
    let third = drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    );
    assert!(
        matches!(&find(&third, &board).boards, PresentationFact::Available(boards)
        if boards[0].columns[0].cards.is_empty()
        && boards[0].columns[1].cards[0].identity.identity == "HMD-011")
    );
}

#[test]
fn activation_project_membership_invalidates_map_facts_not_unrelated_entries_or_handles() {
    let reader = FakeReader::active();
    reader.replace(
        "projects.toml",
        format!(
            "schema_version=1\n[projects]\ndemo={:?}\n",
            std::env::temp_dir().join("offline-demo").to_str().unwrap()
        )
        .into_bytes(),
    );
    let session = PresentationSession::with_reader(reader.clone());
    let find = |events: &[PresentationEvent], path: &str| {
        events
            .iter()
            .find_map(|event| match event {
                PresentationEvent::Entries { entries, .. } => {
                    entries.iter().find(|entry| entry.path == path).cloned()
                }
                _ => None,
            })
            .unwrap()
    };
    let first = drain(
        session
            .load(load_request(1, PresentationTarget::Store))
            .unwrap(),
    );
    let ticket = find(&first, TICKET);
    let handle = find(&first, RAW).content_handle.clone();
    assert_eq!(
        find(&first, "projects.toml").classification,
        PresentationFact::Available(Classification::Governed)
    );
    reader.state.lock().unwrap().activation.1.projects.insert(
        "other".into(),
        Project {
            prefix: "OTH".into(),
            investigations: Vec::new(),
        },
    );
    let second = drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    );
    assert_eq!(
        find(&second, "projects.toml").classification,
        PresentationFact::Available(Classification::Invalid)
    );
    assert!(matches!(&find(&second, "projects.toml").diagnostics,
        PresentationFact::Available(diagnostics) if !diagnostics.is_empty()));
    assert!(Arc::ptr_eq(&ticket, &find(&second, TICKET)));
    assert_eq!(find(&second, RAW).content_handle, handle);
    reader
        .state
        .lock()
        .unwrap()
        .activation
        .1
        .projects
        .remove("other");
    let third = drain(
        session
            .load(load_request(0, PresentationTarget::Store))
            .unwrap(),
    );
    assert_eq!(
        find(&third, "projects.toml").classification,
        PresentationFact::Available(Classification::Governed)
    );
    assert!(matches!(&find(&third, "projects.toml").diagnostics,
        PresentationFact::Available(diagnostics) if diagnostics.is_empty()));
}
