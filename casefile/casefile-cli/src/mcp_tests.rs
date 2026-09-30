use super::*;
use std::{cell::RefCell, process::Command, time::Duration};
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
