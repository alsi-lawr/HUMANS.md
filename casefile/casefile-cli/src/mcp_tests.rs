use super::*;
use std::{cell::RefCell, process::Command, time::Duration};
thread_local! { static DISPATCH: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None); }
pub(super) fn dispatch_boundary() {
    if let Some(hook) = DISPATCH.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
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
