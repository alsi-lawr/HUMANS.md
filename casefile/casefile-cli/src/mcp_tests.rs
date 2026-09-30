use super::*;
use std::{cell::RefCell, process::Command, time::Duration};
pub(super) type DispatchHook = Arc<dyn Fn(&str) + Send + Sync>;

thread_local! { static DISPATCH: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None); }
thread_local! { static DISPATCH_FAILURE: RefCell<Option<anyhow::Error>> = const { RefCell::new(None) }; }
pub(super) fn dispatch_boundary() -> Result<()> {
    if let Some(hook) = DISPATCH.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
    if let Some(error) = DISPATCH_FAILURE.with(|slot| slot.borrow_mut().take()) {
        return Err(error);
    }
    Ok(())
}

#[test]
fn same_mcp_session_dispatches_a_disjoint_apply_while_another_apply_is_active() {
    let root = tempfile::tempdir().unwrap();
    let base = "projects/demo/investigations/sample";
    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"{base}\"]\n"
        ),
    )
    .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let tools = Session::new(Provider::without_cache(Store::open(root.path()).unwrap())).tools;
    let preview = |name: &str| {
        let value = tools.dispatch("casefile_preview_record", json!({"request":{
            "operation":"create","path":format!("{base}/boards/{name}.toml"),"draft":{
                "kind":"board","id":format!("HMD-{name}"),"title":name,"status_source":"progress",
                "columns":[{"name":"TODO","statuses":["unknown"]}]
            }
        }})).unwrap();
        value["preview_id"].clone()
    };
    let first = preview("first");
    let second = preview("second");
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let first_tools = tools.clone();
    let pending = thread::spawn(move || {
        DISPATCH.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                entered.send(()).unwrap();
                released
                    .recv_timeout(Duration::from_secs(20))
                    .expect("concurrent dispatch deadlock watchdog");
            }))
        });
        first_tools.call_tool(
            json!(1),
            Some(&json!({"name":"casefile_apply_record","arguments":{"preview_id":first}})),
        )
    });
    waiting.recv_timeout(Duration::from_secs(20)).unwrap();
    let second = tools.call_tool(
        json!(2),
        Some(&json!({"name":"casefile_apply_record","arguments":{"preview_id":second}})),
    );
    assert_eq!(second["result"]["isError"], false, "{second}");
    release.send(()).unwrap();
    let first = pending.join().unwrap();
    assert_eq!(first["result"]["isError"], false, "{first}");
    for name in ["first", "second"] {
        assert!(
            root.path()
                .join(format!("{base}/boards/{name}.toml"))
                .exists()
        );
    }
}

#[test]
fn mcp_apply_rollback_failure_keeps_error_flag_and_declared_structured_details() {
    use casefile_store::{
        IncompleteRollback, ProviderError, RollbackCause, RollbackErrorCode, RollbackPathState,
        RollbackReason, RollbackRemainingState, StoreError,
    };
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::new(Provider::without_cache(Store::open(root.path()).unwrap()));
    session.handle(json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{"protocolVersion":"2025-06-18"}})).unwrap();
    let list = session
        .handle(json!({"jsonrpc":"2.0", "id":2, "method":"tools/list"}))
        .unwrap()
        .unwrap();
    let schema = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "casefile_apply_record")
        .unwrap()["outputSchema"]
        .clone();
    let details = IncompleteRollback {
        code: RollbackErrorCode::IncompleteRollback,
        operation: "record batch verification".into(),
        cause: RollbackCause::Io,
        affected_paths: vec![
            RollbackPathState {
                path: "tickets/accepted/HMD-1.md".into(),
                remaining: RollbackRemainingState::Regular {
                    revision: casefile_core::Revision("fsmeta-v1:observed".into()),
                },
                reason: RollbackReason::ExternalChange,
            },
            RollbackPathState {
                path: "tickets/accepted/HMD-2.md".into(),
                remaining: RollbackRemainingState::Unknown,
                reason: RollbackReason::ObservationFailed,
            },
        ],
    };
    DISPATCH_FAILURE.with(|slot| {
        *slot.borrow_mut() = Some(
            ProviderError::Store(StoreError::IncompleteRollback {
                details: details.clone(),
                cause: Box::new(StoreError::Invalid("PRIVATE TOML EXCERPT".into())),
            })
            .into(),
        )
    });
    let response = session.handle(json!({"jsonrpc":"2.0", "id":3, "method":"tools/call", "params":{"name":"casefile_apply_record", "arguments":{"preview_id":"session-preview"}}})).unwrap().unwrap();
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        serde_json::from_value::<IncompleteRollback>(
            response["result"]["structuredContent"].clone()
        )
        .unwrap(),
        details
    );
    assert_eq!(
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        response["result"]["structuredContent"]
    );
    assert!(!response.to_string().contains("PRIVATE TOML EXCERPT"));
    if let Some(path) = std::env::var_os("CASEFILE_ERROR_ARTIFACT") {
        fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"schema":schema, "response":response, "tools_list":list}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn mcp_rejects_old_body_and_foreign_tool_ids_before_applying_retained_original() {
    let root = tempfile::tempdir().unwrap();
    let base = "projects/demo/investigations/sample";
    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"{base}\"]\n"
        ),
    )
    .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let tools = Session::new(Provider::without_cache(Store::open(root.path()).unwrap())).tools;
    let path = format!("{base}/boards/review.toml");
    let preview = tools.dispatch("casefile_preview_record", json!({"request":{
        "operation":"create", "path":path, "draft":{"kind":"board", "id":"HMD-review", "title":"Retained original", "status_source":"disposition", "columns":[{"name":"Accepted", "statuses":["accepted"]}]}
    }})).unwrap();
    let id = &preview["preview_id"];
    let obsolete = json!({"name":"casefile_apply_record", "arguments":{"preview_id":id, "canonical":{"request":{"operation":"delete","path":path}}}});
    let refusal = tools.call_tool(json!(1), Some(&obsolete));
    assert_eq!(refusal["result"]["isError"], true);
    assert!(!root.path().join(&path).exists());
    let wrong_family = tools.call_tool(
        json!(2),
        Some(&json!({"name":"casefile_apply_progress", "arguments":{"preview_id":id}})),
    );
    assert_eq!(wrong_family["result"]["isError"], true);
    assert!(!root.path().join(&path).exists());
    let outcome = tools.call_tool(
        json!(3),
        Some(&json!({"name":"casefile_apply_record", "arguments":{"preview_id":id}})),
    );
    assert_eq!(outcome["result"]["isError"], false, "{outcome}");
    assert!(
        fs::read_to_string(root.path().join(path))
            .unwrap()
            .contains("Retained original")
    );
}

#[test]
fn fragmented_framing_preserves_complete_frames_and_refuses_partial_eof() {
    use std::io::{BufReader, Cursor};
    let mut input = BufReader::with_capacity(1, Cursor::new(b"{\"id\":1}\n{\"id\":2}\nunfinished"));
    let mut frame = Vec::new();
    assert!(read_frame(&mut input, &mut frame).unwrap());
    assert_eq!(serde_json::from_slice::<Value>(&frame).unwrap()["id"], 1);
    assert!(read_frame(&mut input, &mut frame).unwrap());
    assert_eq!(serde_json::from_slice::<Value>(&frame).unwrap()["id"], 2);
    assert!(read_frame(&mut input, &mut frame).is_err());
    assert!(frame.len() <= MAX_MESSAGE_BYTES);
}

#[test]
fn fatal_worker_output_drains_an_already_admitted_canonical_apply() {
    let root = tempfile::tempdir().unwrap();
    let base = "projects/demo/investigations/sample";
    fs::write(
        root.path().join("casefile.toml"),
        format!(
            "schema_version = 1\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [\"{base}\"]\n"
        ),
    )
    .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let mut session = Session::new(Provider::without_cache(Store::open(root.path()).unwrap()));
    session.initialized = true;
    let path = format!("{base}/boards/drained.toml");
    let preview = session.tools.dispatch("casefile_preview_record", json!({"request":{
        "operation":"create", "path":path, "draft":{"kind":"board", "id":"HMD-drained", "title":"Drained original", "status_source":"disposition", "columns":[{"name":"Accepted", "statuses":["accepted"]}]}
    }})).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    session.tools.before_dispatch = Some(Arc::new(move |name| {
        if name == "casefile_apply_record" {
            entered.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(20))
                .unwrap();
        }
    }));
    let output = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&output);
    let (ready, input) = mpsc::channel();
    let (finished, completion) = mpsc::channel();
    let controller = thread::spawn(move || {
        let result = session.run_with_input(
            |events, stopped| {
                ready.send((events, stopped)).unwrap();
            },
            output,
        );
        finished.send(result).unwrap();
    });
    let (events, stopped) = input.recv_timeout(Duration::from_secs(20)).unwrap();
    let frame = |value: Value| SessionEvent::Request(Ok(value));
    events.send(frame(json!({"jsonrpc":"2.0", "id":7, "method":"tools/call", "params":{"name":"casefile_apply_record", "arguments":{"preview_id":preview["preview_id"]}}}))).unwrap();
    waiting.recv_timeout(Duration::from_secs(20)).unwrap();
    let mut overflowing = json!({"jsonrpc":"2.0", "id":"", "method":"tools/call", "params":{"name":"casefile_snapshot", "arguments":{}}});
    let id_length = MAX_MESSAGE_BYTES - 24 - serde_json::to_vec(&overflowing).unwrap().len() - 1;
    overflowing["id"] = json!("a".repeat(id_length));
    events.send(frame(overflowing)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !stopped.load(Ordering::Acquire) {
        assert!(
            std::time::Instant::now() < deadline,
            "worker failure did not wake controller"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        completion.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    // Wait for controller closure of input, without EOF, before releasing the admitted apply.
    while events
        .send(frame(json!({"jsonrpc":"2.0", "id":8, "method":"ping"})))
        .is_ok()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "controller kept receiving after fatal output"
        );
    }
    release.send(()).unwrap();
    let result = completion.recv_timeout(Duration::from_secs(20)).unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("cannot fit a bounded response")
    );
    controller.join().unwrap();
    assert!(
        fs::read_to_string(root.path().join(path))
            .unwrap()
            .contains("Drained original")
    );
    let bytes = captured.lock().unwrap();
    let responses = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["id"], 7);
    assert_eq!(responses[0]["result"]["isError"], false);
}
