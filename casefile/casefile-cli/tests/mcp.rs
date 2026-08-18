use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

const OPERATIONS: &str = "snapshot,record_index,record_detail,boards,strategy_transitions,preview_record_draft,apply_record_draft,bootstrap_progress,preview_progress,apply_progress,preview_default_delivery_board,apply_default_delivery_board,preview_strategy_transition,apply_strategy_transition,preview_writer_binding,apply_writer_binding";

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

fn fixture() -> TempDir {
    let root = TempDir::new().expect("root");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../casefile-store/tests/fixtures/minimum"),
        root.path(),
    );
    root
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_casefile"));
    command
        .arg("mcp-stdio")
        .arg("--planning-root")
        .arg(root)
        .arg("--expected-root")
        .arg(root)
        .args(["--expected-provider-protocol", "2"])
        .args(["--required-provider-operations", OPERATIONS]);
    command
}

fn package_command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_casefile"));
    command.arg("mcp-package").arg("--planning-root").arg(root);
    command
}

fn session(root: &Path, requests: &[Value]) -> std::process::Output {
    let mut child = command(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP process");
    {
        let input = child.stdin.as_mut().expect("stdin");
        for request in requests {
            serde_json::to_writer(&mut *input, request).expect("request");
            input.write_all(b"\n").expect("newline");
        }
    }
    drop(child.stdin.take());
    child.wait_with_output().expect("MCP output")
}

#[test]
fn compatibility_contract_is_machine_readable_and_complete() {
    let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
        .arg("mcp-compatibility")
        .output()
        .expect("compatibility");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(value["identity"], "casefile");
    assert_eq!(value["provider_protocol_version"], 2);
    assert_eq!(
        value["required_provider_operations"]
            .as_array()
            .expect("operations")
            .len(),
        16
    );
}

#[test]
fn fixed_root_session_negotiates_and_exposes_canonical_snapshot_and_query() {
    let root = fixture();
    let before = directory_state(root.path());
    let output = session(
        root.path(),
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"casefile_snapshot","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"casefile_query","arguments":{"query":"record_index","scope":{"project":"demo","investigation":"sample"}}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"casefile_query","arguments":{"query":"record_detail","identity":{"scope":{"project":"demo///","investigation":"sample\\\\"},"identity":"HMD-011"}}}}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"casefile_query","arguments":{"query":"record_index","scope":{"project":"C:demo","investigation":"sample"}}}}),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).expect("response"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 6);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    let tools = responses[1]["result"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 12);
    assert!(
        tools
            .iter()
            .all(|tool| tool["inputSchema"]["additionalProperties"] != true),
        "tool argument roots must not be unconstrained objects"
    );
    assert!(tools.iter().all(|tool| tool.get("outputSchema").is_some()));
    assert!(
        tools
            .iter()
            .all(|tool| tool["outputSchema"] != json!({"type": "object"})),
        "every success shape must be explicitly bounded"
    );
    assert!(
        tools
            .iter()
            .all(|tool| tool["inputSchema"]["type"] == "object"),
        "every tool argument root must declare type object; strict MCP clients \
         reject the whole tools/list response when one schema omits it"
    );
    let schema = |name: &str| {
        &tools
            .iter()
            .find(|tool| tool["name"] == name)
            .expect("named tool")["inputSchema"]
    };
    assert!(
        schema("casefile_query").get("oneOf").is_none(),
        "a root-level oneOf collapses under MCP client schema flattening when \
         variants share a discriminant property; keep the query root flat"
    );
    assert_eq!(
        schema("casefile_query")["properties"]["query"]["enum"],
        json!([
            "record_index",
            "record_detail",
            "boards",
            "strategy_transitions"
        ])
    );
    assert_eq!(schema("casefile_query")["required"], json!(["query"]));
    assert_eq!(
        schema("casefile_preview_record")["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        schema("casefile_preview_progress")["required"],
        json!(["operation"])
    );
    assert_eq!(
        schema("casefile_apply_progress")["required"],
        json!(["preview_id"])
    );
    let output_schema = |name: &str| {
        &tools
            .iter()
            .find(|tool| tool["name"] == name)
            .expect("named tool")["outputSchema"]
    };
    assert_eq!(
        output_schema("casefile_preview_progress")["additionalProperties"],
        false
    );
    assert_eq!(
        output_schema("casefile_apply_progress")["required"],
        json!(["result", "cache"])
    );
    let project_schema = &output_schema("casefile_snapshot")["properties"]["catalogue"]["properties"]
        ["projects"]["items"];
    assert_eq!(
        project_schema["properties"]["name"],
        json!({"type": "string"})
    );
    assert_eq!(
        project_schema["properties"]["source_root"],
        json!({"type": "string"})
    );
    let response = |id: i64| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .expect("response ID")
    };
    let snapshot = &response(3)["result"]["structuredContent"];
    assert_eq!(snapshot["activation"], "active");
    assert_eq!(snapshot["capabilities"]["protocol_version"], 2);
    assert_eq!(snapshot["catalogue"]["projects"][0]["name"], "demo");
    assert!(snapshot.get("projections").is_none());
    let query = &response(4)["result"]["structuredContent"];
    assert_eq!(query["result"], "record_index");
    assert_eq!(query["records"].as_array().expect("records").len(), 2);
    assert_eq!(
        response(5)["result"]["structuredContent"]["record"]["identity"]["identity"],
        "HMD-011"
    );
    assert_eq!(response(6)["result"]["isError"], true);
    assert!(response(6)["result"].get("structuredContent").is_none());
    assert_eq!(
        directory_state(root.path()),
        before,
        "read session mutated planning root"
    );
}

#[test]
fn root_protocol_and_capability_refusals_happen_before_tool_service() {
    let root = fixture();
    let sibling = fixture();
    let mismatch = command(root.path());
    let args = mismatch
        .get_args()
        .map(|item| item.to_owned())
        .collect::<Vec<_>>();
    let expected_index = args
        .iter()
        .position(|item| item == "--expected-root")
        .expect("flag")
        + 1;
    let mut rewritten = args;
    rewritten[expected_index] = sibling.path().as_os_str().to_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
        .args(rewritten)
        .output()
        .expect("mismatch");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicts with launcher contract"));

    for (flag, value, diagnostic) in [
        (
            "--expected-provider-protocol",
            "1",
            "requires provider protocol 1",
        ),
        (
            "--required-provider-operations",
            "snapshot",
            "capability contract differs",
        ),
    ] {
        let mut args = command(root.path())
            .get_args()
            .map(|item| item.to_owned())
            .collect::<Vec<_>>();
        let index = args.iter().position(|item| item == flag).expect("flag") + 1;
        args[index] = value.into();
        let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
            .args(args)
            .output()
            .expect("refusal");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic));
        assert!(output.stdout.is_empty());
    }

    fs::remove_file(root.path().join("casefile.toml")).expect("unactivate");
    let output = command(root.path()).output().expect("unactivated");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unactivated"));
    assert!(output.stdout.is_empty());

    fs::write(root.path().join("casefile.toml"), "not = [valid").expect("malformed");
    let output = command(root.path()).output().expect("invalid");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid"));
    assert!(output.stdout.is_empty());

    fs::write(root.path().join("casefile.toml"), "schema_version = 2\n").expect("unsupported");
    let output = command(root.path()).output().expect("unsupported");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported"));
    assert!(output.stdout.is_empty());
}

#[test]
fn adapter_never_infers_root_or_accepts_incompatible_mcp_protocol() {
    let root = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
        .current_dir(root.path())
        .arg("mcp-stdio")
        .output()
        .expect("missing root");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--planning-root"));

    let output = session(
        root.path(),
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        ],
    );
    assert!(output.status.success());
    let responses = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).expect("response"))
        .collect::<Vec<_>>();
    assert!(
        responses[0]["error"]["message"]
            .as_str()
            .expect("message")
            .contains("unsupported MCP protocol")
    );
    assert_eq!(responses[1]["error"]["code"], -32002);
}

#[test]
fn packaged_command_internalizes_the_launcher_contract() {
    let root = fixture();
    let input = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    ]
    .into_iter()
    .map(|value| serde_json::to_string(&value).expect("request"))
    .collect::<Vec<_>>()
    .join("\n")
        + "\n";
    let mut child = package_command(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP process");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("requests");
    let output = child.wait_with_output().expect("output");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).expect("response"))
        .collect::<Vec<_>>();
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "casefile");
    assert_eq!(
        responses[1]["result"]["tools"]
            .as_array()
            .expect("tools")
            .len(),
        12
    );
}

#[test]
fn provider_preview_and_apply_remain_one_session_exact_operations() {
    let root = fixture();
    for arguments in [
        &["init", "-q"][..],
        &["config", "user.email", "casefile@example.test"],
        &["config", "user.name", "Casefile Test"],
        &["add", "."],
        &["commit", "-qm", "fixture"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(root.path())
                .args(arguments)
                .status()
                .expect("git")
                .success()
        );
    }
    let mut child = command(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP process");
    let mut input = child.stdin.take().expect("stdin");
    let mut output = BufReader::new(child.stdout.take().expect("stdout"));
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}});
    serde_json::to_writer(&mut input, &initialize).expect("initialize");
    input.write_all(b"\n").expect("newline");
    input.flush().expect("flush");
    let mut line = String::new();
    output.read_line(&mut line).expect("initialize response");
    assert_eq!(serde_json::from_str::<Value>(&line).expect("JSON")["id"], 1);

    let preview_request = json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"casefile_preview_progress",
            "arguments":{"operation":{"operation":"bootstrap","investigation":"projects\\\\demo//investigations\\\\sample///"}}
        }
    });
    serde_json::to_writer(&mut input, &preview_request).expect("preview request");
    input.write_all(b"\n").expect("newline");
    input.flush().expect("flush");
    line.clear();
    output.read_line(&mut line).expect("preview response");
    let preview_response: Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(
        preview_response["result"]["isError"], false,
        "{preview_response}"
    );
    let preview = preview_response["result"]["structuredContent"].clone();
    let preview_id = preview["preview_id"].as_str().expect("preview ID");
    assert_eq!(preview["approval_required"], false);
    assert_eq!(preview["operation_counts"], json!({"bootstrap": 1}));
    assert_eq!(
        preview["operations"],
        json!([{
            "operation": "bootstrap",
            "path": "projects/demo/investigations/sample/progress/log.toml"
        }])
    );
    assert!(preview.get("canonical").is_none());
    assert!(
        preview["diff"]["sha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:"))
    );
    assert!(
        !preview_response["result"]["content"][0]["text"]
            .as_str()
            .expect("preview text")
            .contains("proposed_bytes")
    );
    let apply_request = json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"casefile_apply_progress","arguments":{"preview_id":preview_id}
        }
    });
    serde_json::to_writer(&mut input, &apply_request).expect("apply request");
    input.write_all(b"\n").expect("newline");
    drop(input);
    line.clear();
    output.read_line(&mut line).expect("apply response");
    let apply_response: Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(apply_response["result"]["isError"], false);
    assert!(
        root.path()
            .join("projects/demo/investigations/sample/progress/log.toml")
            .is_file()
    );
    let result = child.wait_with_output().expect("exit");
    assert!(result.status.success());
    assert!(
        result.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn directory_state(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn visit(base: &Path, current: &Path, files: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(current).expect("entries") {
            let entry = entry.expect("entry");
            if entry.file_type().expect("type").is_dir() {
                visit(base, &entry.path(), files);
            } else {
                files.push((
                    entry
                        .path()
                        .strip_prefix(base)
                        .expect("relative")
                        .display()
                        .to_string(),
                    fs::read(entry.path()).expect("bytes"),
                ));
            }
        }
    }
    let mut files = Vec::new();
    visit(root, root, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}
