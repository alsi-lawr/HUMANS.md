use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

const OPERATIONS: &str = "snapshot,record_index,diagnostics,record_detail,boards,strategy_transitions,preview_record_draft,apply_record_draft,bootstrap_progress,preview_progress,apply_progress,preview_default_delivery_board,apply_default_delivery_board,preview_strategy_transition,apply_strategy_transition,preview_writer_binding,apply_writer_binding";

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
        .args(["--expected-provider-protocol", "6"])
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
    assert_eq!(value["provider_protocol_version"], 6);
    assert_eq!(
        value["required_provider_operations"]
            .as_array()
            .expect("operations")
            .len(),
        17
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
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"casefile_query","arguments":{"query":"diagnostics","scope":{"project":"demo","investigation":"sample"}}}}),
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
    assert_eq!(responses.len(), 7);
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
            "strategy_transitions",
            "diagnostics"
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
        output_schema("casefile_apply_progress")["oneOf"][0]["required"],
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
    assert_eq!(snapshot["capabilities"]["protocol_version"], 6);
    assert_eq!(snapshot["catalogue"]["projects"][0]["name"], "demo");
    assert!(snapshot.get("projections").is_none());
    let query = &response(4)["result"]["structuredContent"];
    assert_eq!(query["result"], "record_index");
    assert_eq!(query["records"].as_array().expect("records").len(), 2);
    assert_eq!(
        response(5)["result"]["structuredContent"]["record"]["identity"]["identity"],
        "HMD-011"
    );
    let diagnostics = &response(7)["result"]["structuredContent"];
    assert_eq!(diagnostics["result"], "diagnostics");
    assert_eq!(snapshot["freshness"]["kind"], "catalogue");
    assert_eq!(diagnostics["freshness"]["kind"], "scope_read");
    assert_eq!(diagnostics["freshness"]["target"]["query"], "diagnostics");
    assert_eq!(
        diagnostics["freshness"]["target"]["scope"],
        diagnostics["scope"]
    );
    assert_eq!(
        diagnostics["scope"],
        json!({"project":"demo", "investigation":"sample"})
    );
    assert_eq!(diagnostics["total_count"], 0);
    assert_eq!(diagnostics["diagnostics"], json!([]));
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
            "5",
            "requires provider protocol 5",
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
    assert_eq!(preview["diff"].as_object().unwrap().len(), 1);
    assert!(preview["diff"]["bytes"].as_u64().is_some());
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

#[test]
fn native_restarted_provider_refuses_the_previous_process_id_without_mutating() {
    let root = fixture();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let initialize = json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{"protocolVersion":"2025-06-18"}});
    let path = "projects/demo/investigations/sample/boards/restart.toml";
    let preview = json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":"casefile_preview_record", "arguments":{"request":{
        "operation":"create", "path":path, "draft":{"kind":"board", "id":"HMD-restart", "title":"Restart preview", "status_source":"disposition", "columns":[{"name":"Accepted", "statuses":["accepted"]}]}
    }}}});
    let responses = |output: std::process::Output| {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    };
    let first = responses(session(root.path(), &[initialize.clone(), preview.clone()]));
    let old_id = first.iter().find(|value| value["id"] == 2).unwrap()["result"]["structuredContent"]["preview_id"].clone();
    assert!(old_id.as_str().is_some());
    let second = responses(session(
        root.path(),
        &[
            initialize,
            preview,
            json!({"jsonrpc":"2.0", "id":3, "method":"tools/call", "params":{"name":"casefile_apply_record", "arguments":{"preview_id":old_id}}}),
        ],
    ));
    let new_id = &second.iter().find(|value| value["id"] == 2).unwrap()["result"]["structuredContent"]
        ["preview_id"];
    assert_ne!(&old_id, new_id);
    assert_eq!(
        second.iter().find(|value| value["id"] == 3).unwrap()["result"]["isError"],
        true
    );
    assert!(!root.path().join(path).exists());
}

#[test]
fn framing_failure_closes_without_accepting_an_unterminated_or_oversized_request() {
    let root = fixture();
    for bytes in [
        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":71,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap(),
        vec![b' '; 8 * 1024 * 1024 + 1],
    ] {
        let mut child = command(root.path()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let _ = child.stdin.take().unwrap().write_all(&bytes);
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn escaped_detail_outer_overflow_returns_original_id_and_allows_next_request() {
    let root = fixture();
    let ticket = root
        .path()
        .join("projects/demo/investigations/sample/tickets/accepted/HMD-011.md");
    let source = fs::read_to_string(&ticket)
        .unwrap()
        .replace("Required.", &"\\\"".repeat(750_000));
    fs::write(ticket, source).unwrap();
    let request_id = json!("escaped-\"\\-detail");
    let mut child = command(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut exchange = |request: Value| {
        serde_json::to_writer(&mut input, &request).unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        assert!(output.read_line(&mut line).unwrap() > 0);
        assert!(line.len() <= 8 * 1024 * 1024);
        serde_json::from_str::<Value>(&line).unwrap()
    };
    assert_eq!(
        exchange(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})
        )["id"],
        1
    );
    let overflow = exchange(
        json!({"jsonrpc":"2.0","id":request_id,"method":"tools/call","params":{"name":"casefile_query","arguments":{"query":"record_detail","identity":{"scope":{"project":"demo","investigation":"sample"},"identity":"HMD-011"}}}}),
    );
    assert_eq!(overflow["id"], request_id);
    assert_eq!(overflow["error"]["code"], -32603);
    assert!(overflow.get("result").is_none());
    let following = exchange(json!({"jsonrpc":"2.0","id":3,"method":"ping"}));
    assert_eq!(following["id"], 3);
    assert_eq!(following["result"], json!({}));
    drop(input);
    use std::io::Read;
    let mut remainder = Vec::new();
    output.read_to_end(&mut remainder).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(remainder.is_empty());
}

#[test]
fn unfit_correlated_id_closes_without_partial_response_or_truncation() {
    let root = fixture();
    let mut child = command(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    serde_json::to_writer(&mut input, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap();
    input.write_all(b"\n").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], 1);
    let request =
        json!({"jsonrpc":"2.0","id":"a".repeat(8 * 1024 * 1024 - 90),"method":"tools/list"});
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    assert!(bytes.len() < 8 * 1024 * 1024);
    input.write_all(&bytes).unwrap();
    drop(input);
    use std::io::Read;
    let mut remainder = Vec::new();
    output.read_to_end(&mut remainder).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert!(remainder.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot fit a bounded response"));
}

#[test]
fn unfit_worker_id_closes_with_stdin_open_without_accepting_following_ping() {
    let root = fixture();
    let mut child = command(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    serde_json::to_writer(&mut input, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}})).unwrap();
    input.write_all(b"\n").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], 1);
    let mut request = json!({"jsonrpc":"2.0","id":"","method":"tools/call","params":{"name":"casefile_snapshot","arguments":{}}});
    let frame_length = 8 * 1024 * 1024 - 24;
    let id_length = frame_length - serde_json::to_vec(&request).unwrap().len() - 1;
    request["id"] = json!("a".repeat(id_length));
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    assert_eq!(bytes.len(), frame_length);
    input.write_all(&bytes).unwrap();
    input.flush().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let closed = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if std::time::Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    // Watchdog cleanup is test-only: the session must close itself while stdin stays open.
    if !closed {
        child.kill().unwrap();
    }
    let following = input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n");
    drop(input);
    use std::io::Read;
    let mut remainder = Vec::new();
    output.read_to_end(&mut remainder).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(closed, "worker fatal output waited for EOF");
    assert!(
        following.is_err(),
        "closed session accepted another request"
    );
    assert!(!result.status.success());
    assert!(
        remainder.is_empty(),
        "partial or unexpected following response"
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot fit a bounded response"));
}

#[cfg(unix)]
mod investigation_isolation {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        process::{Child, ChildStdin, ChildStdout},
    };

    const SELECTED: &str = "projects/demo/investigations/sample";
    const OTHER: &str = "projects/demo/investigations/other";
    const NESTED: &str = "projects/demo/investigations/sample/review/private";

    struct Denied {
        path: PathBuf,
        permissions: fs::Permissions,
    }
    impl Denied {
        fn new(path: PathBuf) -> Option<Self> {
            let directory = path.is_dir();
            let permissions = fs::metadata(&path).unwrap().permissions();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
            let denied = Self { path, permissions };
            let error = if directory {
                fs::read_dir(&denied.path).err()
            } else {
                fs::read(&denied.path).err()
            };
            let Some(error) = error else {
                eprintln!("permission assertions UNEXERCISED: process bypasses mode 000");
                return None;
            };
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            Some(denied)
        }
    }
    impl Drop for Denied {
        fn drop(&mut self) {
            fs::set_permissions(&self.path, self.permissions.clone()).unwrap();
        }
    }

    struct Mcp {
        child: Child,
        input: ChildStdin,
        output: BufReader<ChildStdout>,
        id: u64,
    }
    impl Mcp {
        fn start(root: &Path) -> Self {
            let mut child = package_command(root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut session = Self {
                input: child.stdin.take().unwrap(),
                output: BufReader::new(child.stdout.take().unwrap()),
                child,
                id: 0,
            };
            let initialized =
                session.request("initialize", json!({"protocolVersion":"2025-11-25"}));
            assert_eq!(
                initialized["result"]["protocolVersion"], "2025-11-25",
                "{initialized}"
            );
            serde_json::to_writer(
                &mut session.input,
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            )
            .unwrap();
            session.input.write_all(b"\n").unwrap();
            session
        }
        fn request(&mut self, method: &str, params: Value) -> Value {
            self.id += 1;
            serde_json::to_writer(
                &mut self.input,
                &json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}),
            )
            .unwrap();
            self.input.write_all(b"\n").unwrap();
            self.input.flush().unwrap();
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "MCP closed before response {}",
                self.id
            );
            let result: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(result["id"], self.id, "{result}");
            result
        }
        fn call(&mut self, name: &str, arguments: Value) -> Value {
            self.request("tools/call", json!({"name":name,"arguments":arguments}))["result"].clone()
        }
        fn success(&mut self, name: &str, arguments: Value) -> Value {
            let result = self.call(name, arguments);
            assert_eq!(result["isError"], false, "{name}: {result}");
            result["structuredContent"].clone()
        }
        fn query(&mut self, query: &str) -> Value {
            self.success(
                "casefile_query",
                json!({"query":query,"scope":{"project":"demo","investigation":"sample"}}),
            )
        }
        fn apply(&mut self, name: &str, arguments: Value) {
            let preview = self.success(&format!("casefile_preview_{name}"), arguments);
            assert_eq!(preview["approval_required"], false);
            assert_eq!(preview["diagnostics"], json!([]));
            self.success(
                &format!("casefile_apply_{name}"),
                json!({"preview_id":preview["preview_id"]}),
            );
        }
    }
    impl Drop for Mcp {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn write(root: &Path, path: &str, text: impl AsRef<[u8]>) {
        fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        fs::write(root.join(path), text).unwrap();
    }
    fn isolated_fixture() -> TempDir {
        let root = fixture();
        write(
            root.path(),
            "casefile.toml",
            format!(
                "schema_version=1\n[projects.demo]\nprefix='HMD'\ninvestigations=['{SELECTED}','{OTHER}','{NESTED}']\n"
            ),
        );
        let source = fs::read_to_string(
            root.path()
                .join(format!("{SELECTED}/tickets/accepted/HMD-011.md")),
        )
        .unwrap();
        for (base, id) in [(OTHER, "HMD-099"), (NESTED, "HMD-098")] {
            write(
                root.path(),
                &format!("{base}/tickets/accepted/{id}.md"),
                source
                    .replace("HMD-011", id)
                    .replace(
                        "investigation: \"sample\"",
                        &format!(
                            "investigation: \"{}\"",
                            base.strip_prefix("projects/demo/investigations/").unwrap()
                        ),
                    )
                    .replace("decision_refs: [HMD-D-001]", "decision_refs: []"),
            );
        }
        write(
            root.path(),
            &format!("{OTHER}/epics/accepted/HMD-E-099.md"),
            source
                .replace("HMD-011", "HMD-E-099")
                .replace("investigation: \"sample\"", "investigation: \"other\"")
                .replace("decision_refs: [HMD-D-001]", "decision_refs: []"),
        );
        write(
            root.path(),
            &format!("{OTHER}/decision-log/HMD-D-099-private.md"),
            "# HMD-D-099\n\n## Status\naccepted\n\n## Decision\nPrivate.\n",
        );
        write(
            root.path(),
            &format!("{OTHER}/boards/private.toml"),
            "private malformed board\n",
        );
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../adapters/codex/matrices/casefile-implement-ticket-batch.toml"),
            root.path()
                .join(format!("{SELECTED}/strategy/implementation.toml")),
        )
        .unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "casefile@example.test"],
            &["config", "user.name", "Casefile Test"],
            &["add", "."],
            &["commit", "-qm", "fixture"],
        ] {
            assert!(
                Command::new("git")
                    .current_dir(root.path())
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        root
    }

    #[test]
    fn mcp_scoped_reads_and_all_mutation_families_ignore_denied_sibling_support_and_directories() {
        for directory in [false, true] {
            let root = isolated_fixture();
            let sibling_before = directory_state(&root.path().join(OTHER));
            let nested_before = directory_state(&root.path().join(NESTED));
            let mut denied = Vec::new();
            let Some(nested) = Denied::new(root.path().join(NESTED)) else {
                return;
            };
            denied.push(nested);
            let paths = if directory {
                vec![OTHER.to_owned()]
            } else {
                vec![
                    format!("{OTHER}/tickets/accepted/HMD-099.md"),
                    format!("{OTHER}/epics/accepted/HMD-E-099.md"),
                    format!("{OTHER}/decision-log/HMD-D-099-private.md"),
                ]
            };
            for path in paths {
                let Some(guard) = Denied::new(root.path().join(path)) else {
                    return;
                };
                denied.push(guard);
            }
            let mut mcp = Mcp::start(root.path());
            assert!(
                mcp.request("tools/list", json!({}))["result"]["tools"]
                    .as_array()
                    .unwrap()
                    .len()
                    > 1
            );
            let catalogue = mcp.success("casefile_snapshot", json!({}));
            assert_eq!(
                catalogue["catalogue"]["projects"][0]["investigations"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            assert_eq!(
                mcp.query("record_index")["records"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            mcp.success("casefile_query", json!({"query":"record_detail","identity":{"scope":{"project":"demo","investigation":"sample"},"identity":"HMD-011"}}));
            mcp.query("boards");
            mcp.query("strategy_transitions");
            let diagnostics = mcp.query("diagnostics");
            assert_eq!(diagnostics["diagnostics"], json!([]));
            assert_eq!(
                diagnostics["diagnostic_coverage"],
                "investigation_and_explicit_dependencies"
            );
            let selected = format!("{SELECTED}/tickets/accepted/HMD-011.md");
            let source = fs::read_to_string(root.path().join(&selected)).unwrap();
            let mut draft =
                casefile_core::parse_draft(&selected, casefile_core::Kind::Ticket, &source)
                    .unwrap();
            if let casefile_core::RecordDraft::Ticket(item) = &mut draft {
                item.id = "HMD-100".into();
                item.title = "Isolated MCP mutation".into();
            }
            let created = format!("{SELECTED}/tickets/accepted/HMD-100.md");
            mcp.apply(
                "record",
                json!({"request":{"operation":"create","path":created,"draft":draft}}),
            );
            assert!(
                fs::read_to_string(root.path().join(&created))
                    .unwrap()
                    .contains("Isolated MCP mutation")
            );
            mcp.apply(
                "progress",
                json!({"operation":{"operation":"bootstrap","investigation":SELECTED}}),
            );
            mcp.apply("default_delivery_board", json!({"investigation":SELECTED}));
            let matrix = fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../adapters/codex/matrices/casefile-implement-pipeline.toml"),
            )
            .unwrap();
            mcp.apply("strategy_transition", json!({"request":{"investigation":SELECTED,"operation_id":"isolation-pipeline","recorded_at":"2026-10-22T01:00:00Z","selected_matrix_origin":"adapters/codex/matrices/casefile-implement-pipeline.toml","selected_matrix_source":matrix,"available_capabilities":["exclusive_writer","shared_writable_planning","subagents"],"preserved_work_paths":[],"active_ownership":[],"rationale":"Selected pipeline in isolated fixture."}}));
            let binding = "schema_version=1\nadapter='codex'\nrole='implementation-writer'\nmodel='gpt-6.1-sol'\nreasoning_effort='xhigh'\n[resolution]\nmode='runtime_override'\nvalue='pipeline=writer'\n";
            mcp.apply(
                "writer_binding",
                json!({"request":{"investigation":SELECTED,"binding_source":binding}}),
            );
            mcp.apply("progress", json!({"operation":{"operation":"append","investigation":SELECTED,"entries":[{"kind":"transition","id":"isolation-start","recorded_at":"2026-10-22T01:00:01Z","recorded_by":"test","ticket_id":"HMD-011","from":"unknown","to":"in_progress"}]}}));
            let index = mcp.query("record_index");
            let ticket = index["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|record| record["identity"] == "HMD-011")
                .unwrap();
            assert_eq!(ticket["progress"]["status"], "in_progress");
            assert_eq!(mcp.query("diagnostics")["diagnostics"], json!([]));
            drop(mcp);
            drop(denied);
            assert_eq!(directory_state(&root.path().join(OTHER)), sibling_before);
            assert_eq!(directory_state(&root.path().join(NESTED)), nested_before);
        }
    }

    #[test]
    fn mcp_explicit_local_decision_avoids_bare_ids_required_project_lookup() {
        let root = isolated_fixture();
        let project_log = "projects/demo/decision-log";
        write(
            root.path(),
            &format!("{project_log}/HMD-D-099-shared.md"),
            "# HMD-D-099\n\n## Status\naccepted\n\n## Decision\nShared.\n",
        );
        let before = directory_state(&root.path().join(project_log));
        let Some(denied) = Denied::new(root.path().join(project_log)) else {
            return;
        };
        let selected = format!("{SELECTED}/tickets/accepted/HMD-011.md");
        let original = fs::read_to_string(root.path().join(&selected)).unwrap();
        let mut mcp = Mcp::start(root.path());
        mcp.query("record_index");
        let failed = mcp.call(
            "casefile_query",
            json!({"query":"diagnostics","scope":{"project":"demo","investigation":"sample"}}),
        );
        assert_eq!(failed["isError"], true);
        assert!(failed.to_string().contains(project_log), "{failed}");
        let draft =
            casefile_core::parse_draft(&selected, casefile_core::Kind::Ticket, &original).unwrap();
        let failed = mcp.call(
            "casefile_preview_record",
            json!({"request":{"operation":"replace","path":selected,"draft":draft}}),
        );
        assert_eq!(failed["isError"], true);
        assert!(failed.to_string().contains(project_log), "{failed}");
        assert_eq!(
            fs::read_to_string(root.path().join(&selected)).unwrap(),
            original
        );
        let local = format!("{SELECTED}/decision-log/HMD-D-001-scope.md");
        for path in [
            selected.clone(),
            format!("{SELECTED}/epics/accepted/HMD-E-001.md"),
        ] {
            let source = fs::read_to_string(root.path().join(&path)).unwrap();
            write(
                root.path(),
                &path,
                source.replace(
                    "decision_refs: [HMD-D-001]",
                    &format!("decision_refs: [{local}]"),
                ),
            );
        }
        assert_eq!(mcp.query("diagnostics")["diagnostics"], json!([]));
        let source = fs::read_to_string(root.path().join(&selected)).unwrap();
        let mut draft =
            casefile_core::parse_draft(&selected, casefile_core::Kind::Ticket, &source).unwrap();
        if let casefile_core::RecordDraft::Ticket(item) = &mut draft {
            item.title = "Explicit local decision".into();
        }
        mcp.apply(
            "record",
            json!({"request":{"operation":"replace","path":selected,"draft":draft}}),
        );
        assert!(
            fs::read_to_string(root.path().join(&selected))
                .unwrap()
                .contains("Explicit local decision")
        );
        drop(mcp);
        drop(denied);
        assert_eq!(directory_state(&root.path().join(project_log)), before);
        write(
            root.path(),
            &format!("{project_log}/HMD-D-001-duplicate.md"),
            "# HMD-D-001\n\n## Status\naccepted\n\n## Decision\nAmbiguous.\n",
        );
        write(root.path(), &selected, original);
        let mut mcp = Mcp::start(root.path());
        assert!(
            mcp.query("diagnostics")["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic["code"] == "unresolved_reference")
        );
    }

    #[test]
    fn mcp_denied_selected_and_required_paths_and_stale_previews_remain_failures() {
        for directory in [false, true] {
            let root = isolated_fixture();
            let selected = format!("{SELECTED}/tickets/accepted/HMD-011.md");
            let dependency = format!("{OTHER}/tickets/accepted/HMD-099.md");
            let original = fs::read_to_string(root.path().join(&selected)).unwrap();
            let Some(denied) =
                Denied::new(
                    root.path()
                        .join(if directory { OTHER } else { &dependency }),
                )
            else {
                return;
            };
            let mut mcp = Mcp::start(root.path());
            let Some(selected_denied) = Denied::new(root.path().join(&selected)) else {
                return;
            };
            let failed = mcp.call(
                "casefile_query",
                json!({"query":"diagnostics","scope":{"project":"demo","investigation":"sample"}}),
            );
            assert_eq!(failed["isError"], true);
            assert!(failed.to_string().contains(&selected), "{failed}");
            drop(selected_denied);
            write(
                root.path(),
                &selected,
                original.replace("related_tickets: []", "related_tickets: [HMD-099]"),
            );
            let bare = mcp.query("diagnostics");
            assert!(
                bare["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic["code"] == "unresolved_reference")
            );
            let explicit = original.replace(
                "related_tickets: []",
                &format!("related_tickets: [{dependency}]"),
            );
            write(root.path(), &selected, &explicit);
            let failed = mcp.call(
                "casefile_query",
                json!({"query":"diagnostics","scope":{"project":"demo","investigation":"sample"}}),
            );
            assert_eq!(failed["isError"], true);
            assert!(failed.to_string().contains(&dependency), "{failed}");
            let draft =
                casefile_core::parse_draft(&selected, casefile_core::Kind::Ticket, &explicit)
                    .unwrap();
            let failed = mcp.call(
                "casefile_preview_record",
                json!({"request":{"operation":"replace","path":selected,"draft":draft}}),
            );
            assert_eq!(failed["isError"], true);
            assert!(failed.to_string().contains(&dependency), "{failed}");
            assert_eq!(
                fs::read_to_string(root.path().join(&selected)).unwrap(),
                explicit
            );
            write(root.path(), &selected, original);
            let target = format!("{SELECTED}/boards/stale.toml");
            let preview = mcp.success("casefile_preview_record", json!({"request":{"operation":"create","path":target,"draft":{"kind":"board","id":"HMD-stale","title":"Preview","status_source":"disposition","columns":[{"name":"Accepted","statuses":["accepted"]}]}}}));
            let external = "schema_version=1\nid='HMD-stale'\ntitle='External edit'\n[[columns]]\nname='Accepted'\nstatuses=['accepted']\n";
            write(root.path(), &target, external);
            let failed = mcp.call(
                "casefile_apply_record",
                json!({"preview_id":preview["preview_id"]}),
            );
            assert_eq!(failed["isError"], true);
            assert_eq!(
                fs::read_to_string(root.path().join(&target)).unwrap(),
                external
            );
            drop(mcp);
            drop(denied);
        }
    }
}
