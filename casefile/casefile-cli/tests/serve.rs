use casefile_core::{ChangeRequest, Kind, Preview, RecordDraft};
use casefile_store::Store;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tempfile::TempDir;

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
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../casefile-store/tests/fixtures/minimum")
            .as_path(),
        root.path(),
    );
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
                .expect("git")
                .success()
        );
    }
    root
}

struct Running {
    child: Child,
    port: u16,
    capability: String,
    index: PathBuf,
}

impl Running {
    fn start(root: &Path, index: Option<&Path>, write: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_casefile"));
        command
            .args(["--root"])
            .arg(root)
            .arg("serve")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(index) = index {
            command.arg("--index").arg(index);
        }
        if write {
            command.arg("--write");
        }
        let mut child = command.spawn().expect("serve process");
        let mut output = BufReader::new(child.stdout.take().expect("server stdout"));
        let mut lines = Vec::new();
        for _ in 0..4 {
            let mut line = String::new();
            assert_ne!(output.read_line(&mut line).expect("launch line"), 0);
            lines.push(line.trim().to_owned());
        }
        assert_eq!(
            lines[1],
            format!(
                "Casefile root: {}",
                fs::canonicalize(root).expect("canonical root").display()
            )
        );
        let launched_index = PathBuf::from(
            lines[2]
                .strip_prefix("Casefile index: ")
                .expect("index path"),
        );
        if let Some(index) = index {
            assert_eq!(launched_index, index);
        } else {
            assert!(!launched_index.starts_with(fs::canonicalize(root).expect("canonical root")));
        }
        let port = lines[0]
            .strip_prefix("Casefile server: http://127.0.0.1:")
            .expect("server address")
            .parse()
            .expect("server port");
        let capability = lines[3]
            .strip_prefix("Casefile write capability: ")
            .expect("capability")
            .to_owned();
        assert_eq!(capability.len(), 64);
        Self {
            child,
            port,
            capability,
            index: launched_index,
        }
    }

    fn authority(&self, host: &str) -> String {
        format!("{host}:{}", self.port)
    }
    fn origin(&self, host: &str) -> String {
        format!("http://{host}:{}", self.port)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct HttpResponse {
    status: u16,
    headers: String,
    body: String,
}

fn request(
    server: &Running,
    method: &str,
    path: &str,
    host: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    )
    .expect("request line");
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").expect("request header");
    }
    write!(stream, "\r\n{body}").expect("request body");
    stream.flush().expect("flush request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("response");
    let (head, body) = response.split_once("\r\n\r\n").expect("HTTP response");
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .expect("status");
    let headers = head.to_ascii_lowercase();
    assert!(!headers.contains("access-control-"));
    HttpResponse {
        status,
        headers,
        body: body.into(),
    }
}

fn json_request(server: &Running, path: &str, body: &Value) -> HttpResponse {
    request(
        server,
        "POST",
        path,
        &server.authority("127.0.0.1"),
        &[("Content-Type", "application/json")],
        &body.to_string(),
    )
}

#[test]
fn serve_exposes_only_the_fixed_read_contract() {
    let root = fixture();
    let server = Running::start(root.path(), None, false);

    let static_page = request(&server, "GET", "/", &server.authority("localhost"), &[], "");
    assert_eq!(static_page.status, 200);
    assert!(static_page.headers.contains("content-type: text/html"));
    assert!(static_page.body.contains("Casefile loopback host"));
    assert_eq!(
        request(
            &server,
            "GET",
            "/projects.toml",
            &server.authority("127.0.0.1"),
            &[],
            ""
        )
        .status,
        404
    );
    assert_eq!(
        request(
            &server,
            "GET",
            "/api/query",
            &server.authority("127.0.0.1"),
            &[],
            ""
        )
        .status,
        405
    );
    assert_eq!(
        request(
            &server,
            "POST",
            "/api/query",
            "example.test",
            &[("Content-Type", "application/json")],
            "{}"
        )
        .status,
        400
    );
    assert_eq!(
        request(
            &server,
            "POST",
            "/api/query",
            &server.authority("127.0.0.1"),
            &[],
            "{}"
        )
        .status,
        415
    );

    for query in [
        json!({"query":"records", "scope":{"project":"demo", "investigation":"sample"}, "search":"Minimum"}),
        json!({"query":"relationships", "identity":{"scope":{"project":"demo", "investigation":"sample"}, "identity":"HMD-011"}}),
        json!({"query":"boards", "scope":{"project":"demo", "investigation":"sample"}}),
        json!({"query":"diagnostics"}),
    ] {
        let response = json_request(&server, "/api/query", &query);
        assert_eq!(response.status, 200, "{}", response.body);
        assert!(serde_json::from_str::<Value>(&response.body).is_ok());
    }
    assert!(server.index.is_file());
    assert_eq!(
        json_request(
            &server,
            "/api/query",
            &json!({"query":"records", "root":"/tmp"})
        )
        .status,
        400
    );
    assert_eq!(json_request(&server, "/api/apply", &json!({})).status, 403);
    let default_index = server.index.clone();
    drop(server);
    let again = Running::start(root.path(), None, false);
    assert_eq!(again.index, default_index);
    drop(again);
    fs::remove_file(default_index).expect("remove default index");
}

#[test]
fn serve_preserves_preview_and_gates_apply_with_origin_and_capability() {
    let root = fixture();
    let indexes = TempDir::new().expect("indexes");
    let index = indexes.path().join("write.sqlite");
    let server = Running::start(root.path(), Some(&index), true);
    let path = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
    let text = fs::read_to_string(root.path().join(path)).expect("ticket");
    let mut draft = casefile_core::parse_draft(path, Kind::Ticket, &text).expect("draft");
    let RecordDraft::Ticket(ticket) = &mut draft else {
        unreachable!()
    };
    ticket.title = "Updated through loopback".into();
    let change = ChangeRequest::Replace {
        path: path.into(),
        draft,
    };
    let expected = Store::open(root.path())
        .expect("store")
        .preview(change.clone())
        .expect("preview");
    let preview_response = request(
        &server,
        "POST",
        "/api/preview",
        &server.authority("localhost"),
        &[
            ("Content-Type", "application/json"),
            (
                "Origin",
                &format!("https://{}", server.authority("localhost")),
            ),
        ],
        &serde_json::to_string(&change).expect("change JSON"),
    );
    assert_eq!(preview_response.status, 200, "{}", preview_response.body);
    let preview: Preview = serde_json::from_str(&preview_response.body).expect("preview JSON");
    assert_eq!(preview, expected);

    let preview_json = serde_json::to_string(&preview).expect("preview JSON");
    assert_eq!(
        request(
            &server,
            "POST",
            "/api/apply",
            &server.authority("127.0.0.1"),
            &[("Content-Type", "application/json")],
            &preview_json
        )
        .status,
        403
    );
    assert_eq!(
        request(
            &server,
            "POST",
            "/api/apply",
            &server.authority("127.0.0.1"),
            &[
                ("Content-Type", "application/json"),
                ("Origin", &server.origin("localhost")),
                ("X-Casefile-Write-Capability", &server.capability)
            ],
            &preview_json
        )
        .status,
        403
    );
    assert!(
        !fs::read_to_string(root.path().join(path))
            .expect("unchanged")
            .contains("Updated through loopback")
    );

    let applied = request(
        &server,
        "POST",
        "/api/apply",
        &server.authority("127.0.0.1"),
        &[
            ("Content-Type", "application/json"),
            ("Origin", &server.origin("127.0.0.1")),
            ("X-Casefile-Write-Capability", &server.capability),
        ],
        &preview_json,
    );
    assert_eq!(applied.status, 200, "{}", applied.body);
    let value: Value = serde_json::from_str(&applied.body).expect("apply JSON");
    assert_eq!(value["result"]["path"], path);
    assert!(value["index_error"].is_null());
    assert!(
        fs::read_to_string(root.path().join(path))
            .expect("applied")
            .contains("Updated through loopback")
    );
    let refreshed = json_request(
        &server,
        "/api/query",
        &json!({"query":"records", "search":"Updated through loopback"}),
    );
    assert_eq!(refreshed.status, 200, "{}", refreshed.body);
    assert!(refreshed.body.contains("Updated through loopback"));

    let stale = request(
        &server,
        "POST",
        "/api/apply",
        &server.authority("127.0.0.1"),
        &[
            ("Content-Type", "application/json"),
            ("X-Casefile-Write-Capability", &server.capability),
        ],
        &preview_json,
    );
    assert_eq!(stale.status, 400);
    assert!(stale.body.contains("stale store revision"));
}
