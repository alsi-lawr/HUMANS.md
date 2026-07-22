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

#[test]
fn rust_validator_accepts_every_shipped_adapter_matrix_including_solo() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for adapter in ["codex", "claude"] {
        let matrices = source.join("adapters").join(adapter).join("matrices");
        for entry in fs::read_dir(&matrices).expect("matrices") {
            let matrix = entry.expect("matrix").path();
            let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
                .args(["validate-matrix", "--matrix"])
                .arg(&matrix)
                .output()
                .expect("validator");
            assert!(
                output.status.success(),
                "{}: {}",
                matrix.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

const BINDING: &str = "schema_version = 1\nadapter = \"codex\"\nrole = \"implementation-writer\"\nmodel = \"gpt-5.6-sol\"\nreasoning_effort = \"high\"\n[resolution]\nmode = \"named_profile\"\nvalue = \"writer\"\n";

fn replace_binding(root: &Path, source: &Path, active: bool) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_casefile"))
        .args(["--root"])
        .arg(root)
        .args([
            "replace-strategy-binding",
            "--investigation",
            "projects/demo/investigations/sample",
            "--source",
        ])
        .arg(source)
        .args(["--implementation-active", &active.to_string()])
        .output()
        .expect("replace binding command")
}

fn project_binding(root: &Path, strategy_id: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_casefile"))
        .args(["--root"])
        .arg(root)
        .args([
            "project-writer-binding",
            "--investigation",
            "projects/demo/investigations/sample",
            "--strategy-id",
            strategy_id,
        ])
        .output()
        .expect("project writer binding command")
}

#[test]
fn binding_cli_delegates_validation_activity_gate_and_single_file_replacement_to_store() {
    let root = fixture();
    let source = root.path().join("binding-source.toml");
    let target = root
        .path()
        .join("projects/demo/investigations/sample/strategy/bindings.toml");

    fs::write(&source, BINDING).expect("binding source");
    let active = replace_binding(root.path(), &source, true);
    assert!(!active.status.success());
    assert!(String::from_utf8_lossy(&active.stderr).contains("implementation work is active"));
    assert!(!target.exists());

    let invalid_source = root.path().join("invalid-binding.toml");
    fs::write(&invalid_source, "not = [toml").expect("invalid source");
    let invalid = replace_binding(root.path(), &invalid_source, false);
    assert!(!invalid.status.success());
    assert!(!target.exists());

    let created = replace_binding(root.path(), &source, false);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert_eq!(
        BINDING,
        fs::read_to_string(&target).expect("binding target")
    );

    let replacement = BINDING.replace("gpt-5.6-sol", "gpt-5.6-terra");
    fs::write(&source, &replacement).expect("replacement source");
    let replaced = replace_binding(root.path(), &source, false);
    assert!(
        replaced.status.success(),
        "{}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    assert_eq!(
        replacement,
        fs::read_to_string(&target).expect("replacement target")
    );
    let strategy = root
        .path()
        .join("projects/demo/investigations/sample/strategy");
    assert!(!strategy.join("binding-history").exists());
    assert!(!strategy.join(".binding-transaction.toml").exists());
    assert!(
        !fs::read_dir(strategy)
            .expect("strategy entries")
            .any(|entry| {
                entry
                    .expect("strategy entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".bindings.toml.tmp-")
            })
    );
}

#[test]
fn writer_projection_uses_canonical_matrix_and_binding_states() {
    let root = fixture();
    let implementation = root
        .path()
        .join("projects/demo/investigations/sample/strategy/implementation.toml");
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../adapters/codex/matrices/casefile-implement-ticket-batch.toml");
    let historical = fs::read_to_string(shipped)
        .expect("shipped matrix")
        .replacen("model = \"gpt-5.6-sol\"", "model = \"gpt-5.6-terra\"", 1);
    fs::write(&implementation, historical).expect("historical implementation matrix");

    let absent = project_binding(root.path(), "casefile-implement-ticket-batch");
    assert!(
        absent.status.success(),
        "{}",
        String::from_utf8_lossy(&absent.stderr)
    );
    assert_eq!(
        json!({
            "strategy_id": "casefile-implement-ticket-batch",
            "adapter": "codex",
            "binding": {
                "state": "absent",
                "effective": {
                    "model": "gpt-5.6-terra",
                    "reasoning_effort": "high",
                    "source": "matrix"
                }
            }
        }),
        serde_json::from_slice::<Value>(&absent.stdout).expect("absent projection")
    );

    let binding = root.path().join("binding-source.toml");
    fs::write(&binding, BINDING).expect("binding source");
    assert!(
        replace_binding(root.path(), &binding, false)
            .status
            .success()
    );
    let resolved = project_binding(root.path(), "casefile-implement-ticket-batch");
    assert!(resolved.status.success());
    assert_eq!(
        json!({
            "state": "resolved",
            "effective": {
                "model": "gpt-5.6-sol",
                "reasoning_effort": "high",
                "source": "binding"
            }
        }),
        serde_json::from_slice::<Value>(&resolved.stdout).expect("resolved projection")["binding"]
    );

    fs::write(&binding, BINDING.replacen("codex", "claude", 1)).expect("mismatch source");
    assert!(
        replace_binding(root.path(), &binding, false)
            .status
            .success()
    );
    let unresolved = project_binding(root.path(), "casefile-implement-ticket-batch");
    assert!(unresolved.status.success());
    assert_eq!(
        json!({"state": "unresolved"}),
        serde_json::from_slice::<Value>(&unresolved.stdout).expect("unresolved projection")["binding"]
    );

    let target = root
        .path()
        .join("projects/demo/investigations/sample/strategy/bindings.toml");
    fs::write(&target, "not = [toml").expect("invalid binding");
    let invalid = project_binding(root.path(), "casefile-implement-ticket-batch");
    assert!(invalid.status.success());
    assert_eq!(
        json!({"state": "invalid"}),
        serde_json::from_slice::<Value>(&invalid.stdout).expect("invalid projection")["binding"]
    );

    fs::write(&implementation, "not = [toml").expect("invalid implementation");
    let ungraphable = project_binding(root.path(), "casefile-implement-ticket-batch");
    assert!(!ungraphable.status.success());
    assert!(String::from_utf8_lossy(&ungraphable.stderr).contains("invalid or ungraphable"));
}
