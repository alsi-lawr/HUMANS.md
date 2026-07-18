use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct TemporaryRoot(PathBuf);

impl TemporaryRoot {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> TemporaryRoot {
    static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);
    let temporary = TemporaryRoot(std::env::temp_dir().join(format!(
        "casefile-cli-check-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&temporary.0).expect("temporary root");
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../casefile-store/tests/fixtures/minimum")
            .as_path(),
        &temporary.0,
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

fn check(root: &Path, require_activation: bool) -> (bool, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_casefile"));
    command.args(["--root"]).arg(root).arg("check");
    if require_activation {
        command.arg("--require-activation");
    }
    let output = command.output().expect("check command");
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout).expect("check JSON"),
    )
}

fn assert_result(value: &Value, activation: &str, valid: Value, diagnostics: Value) {
    let revision = value["revision"].as_str().expect("revision");
    assert!(revision.starts_with("sha256:"), "{revision}");
    assert_eq!(
        json!({
            "activation": activation,
            "valid": valid,
            "revision": revision,
            "diagnostics": diagnostics,
        }),
        *value
    );
}

#[test]
fn check_has_exact_json_and_exit_contract_for_each_activation_state() {
    let root = fixture();
    let (success, value) = check(root.path(), false);
    assert!(success);
    assert_result(&value, "active", json!(true), json!([]));

    fs::write(
        root.path()
            .join("projects/demo/investigations/sample/boards/main.toml"),
        "schema_version = 1\nid = 'HMD-board'\ntitle = 'bad'\n",
    )
    .expect("invalid board");
    let (success, value) = check(root.path(), false);
    assert!(!success);
    let diagnostics = value["diagnostics"].clone();
    assert!(
        diagnostics
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert_result(&value, "active", json!(false), diagnostics);

    fs::remove_file(root.path().join("casefile.toml")).expect("remove activation");
    let (success, value) = check(root.path(), false);
    assert!(success);
    assert_result(&value, "unactivated", Value::Null, json!([]));
    let (success, value) = check(root.path(), true);
    assert!(!success);
    assert_result(&value, "unactivated", Value::Null, json!([]));

    fs::write(root.path().join("casefile.toml"), "not = [valid").expect("bad activation");
    let (success, value) = check(root.path(), false);
    assert!(!success);
    let diagnostics = value["diagnostics"].clone();
    assert!(diagnostics.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["code"] == "invalid_activation")
    }));
    assert_result(&value, "invalid", json!(false), diagnostics);
}

#[test]
fn check_ignores_unlisted_history_in_an_activated_root() {
    let root = fixture();
    fs::write(root.path().join("legacy-history.txt"), "unmanaged").expect("legacy history");
    let (success, value) = check(root.path(), false);
    assert!(success);
    assert_result(&value, "active", json!(true), json!([]));
}
