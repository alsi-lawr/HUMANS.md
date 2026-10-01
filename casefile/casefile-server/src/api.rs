use crate::{assets, workbench::Workbench};
use anyhow::Result;
use bytes::Bytes;
use casefile_core::ChangeRequest;
use casefile_store::{ProviderError, RecordScope, ScopedIdentity};
use http_body_util::Full;
use hyper::{Method, Response, http::request::Parts};
use serde::{Deserialize, Serialize};

const CAPABILITY_HEADER: &str = "X-Casefile-Write-Capability";

#[derive(Deserialize)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
enum Query {
    Snapshot,
    Records {
        scope: Option<RecordScope>,
        search: Option<String>,
    },
    Relationships {
        identity: ScopedIdentity,
    },
    Boards {
        scope: RecordScope,
    },
    Diagnostics,
    Workspace {
        #[serde(flatten)]
        context: crate::workbench::workspace::WorkspaceContext,
    },
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<casefile_store::IncompleteRollback>,
}

pub(crate) struct Host {
    workbench: Workbench,
    port: u16,
    write: bool,
    capability: String,
    #[cfg(test)]
    before_apply: Option<Box<dyn Fn() + Send + Sync>>,
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

struct ApiError {
    status: u16,
    message: String,
    code: Option<&'static str>,
    details: Option<casefile_store::IncompleteRollback>,
}

impl Reply {
    fn json(value: &impl Serialize) -> Result<Self, ApiError> {
        serde_json::to_vec(value)
            .map(|body| Self {
                status: 200,
                content_type: "application/json",
                body,
            })
            .map_err(ApiError::internal)
    }

    fn error(error: ApiError) -> Self {
        Self {
            status: error.status,
            content_type: "application/json",
            body: serde_json::to_vec(&ErrorResponse {
                error: error.message,
                code: error.code,
                details: error.details,
            })
            .expect("error response serializes"),
        }
    }
    fn response(self) -> Response<Full<Bytes>> {
        Response::builder()
            .status(self.status)
            .header("Content-Type", self.content_type)
            .body(Full::new(Bytes::from(self.body)))
            .expect("valid response")
    }
}

impl ApiError {
    fn request(error: impl ToString) -> Self {
        Self {
            status: 400,
            message: error.to_string(),
            code: None,
            details: None,
        }
    }
    fn store(error: casefile_store::StoreError) -> Self {
        if let casefile_store::StoreError::IncompleteRollback { details, .. } = error {
            return Self {
                status: 409,
                message: "incomplete rollback".into(),
                code: Some("incomplete_rollback"),
                details: Some(details),
            };
        }
        let stale = matches!(error, casefile_store::StoreError::StaleTargetRevision);
        Self {
            status: if stale { 409 } else { 400 },
            message: error.to_string(),
            code: stale.then_some("stale_revision"),
            details: None,
        }
    }
    fn provider(error: ProviderError) -> Self {
        match error {
            ProviderError::Store(error) => Self::store(error),
            ProviderError::PreviewIntegrity => Self {
                status: 400,
                message: ProviderError::PreviewIntegrity.to_string(),
                code: Some("preview_integrity"),
                details: None,
            },
            other => Self {
                status: 400,
                message: other.to_string(),
                code: None,
                details: None,
            },
        }
    }
    fn forbidden(message: &str) -> Self {
        Self {
            status: 403,
            message: message.into(),
            code: None,
            details: None,
        }
    }
    fn internal(error: impl ToString) -> Self {
        Self {
            status: 500,
            message: error.to_string(),
            code: None,
            details: None,
        }
    }
}

impl Host {
    pub(crate) fn new(workbench: Workbench, port: u16, write: bool, capability: String) -> Self {
        Self {
            workbench,
            port,
            write,
            capability,
            #[cfg(test)]
            before_apply: None,
        }
    }

    pub(crate) fn handle(&self, request: Parts, bytes: Bytes) -> Response<Full<Bytes>> {
        let reply = std::str::from_utf8(&bytes)
            .map_err(ApiError::request)
            .and_then(|body| self.route(&request, body))
            .unwrap_or_else(Reply::error);
        reply.response()
    }

    fn route(&self, request: &Parts, body: &str) -> Result<Reply, ApiError> {
        self.validate_authority(request)?;
        let method = request.method.clone();
        let path = request
            .uri
            .path_and_query()
            .map_or("/", |value| value.as_str())
            .to_owned();
        match (method, path.as_str()) {
            (Method::GET, "/") => Ok(asset_reply("/", "text/html; charset=utf-8")),
            (Method::GET, "/assets/app.js") => Ok(asset_reply(
                "/assets/app.js",
                "text/javascript; charset=utf-8",
            )),
            (Method::GET, "/assets/app.css") => {
                Ok(asset_reply("/assets/app.css", "text/css; charset=utf-8"))
            }
            (Method::POST, path @ ("/api/query" | "/api/preview" | "/api/apply")) => {
                if !header(request, "Content-Type").is_some_and(|value| {
                    value
                        .split(';')
                        .next()
                        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
                }) {
                    return Err(ApiError {
                        status: 415,
                        message: "Content-Type must be application/json".into(),
                        code: None,
                        details: None,
                    });
                }
                match path {
                    "/api/query" => self.query(body),
                    "/api/preview" => self.preview(body),
                    _ => self.apply(request, body),
                }
            }
            (
                _,
                "/" | "/assets/app.js" | "/assets/app.css" | "/api/query" | "/api/preview"
                | "/api/apply",
            ) => Err(ApiError {
                status: 405,
                message: "method not allowed".into(),
                code: None,
                details: None,
            }),
            _ => Err(ApiError {
                status: 404,
                message: "route not found".into(),
                code: None,
                details: None,
            }),
        }
    }

    fn validate_authority(&self, request: &Parts) -> Result<(), ApiError> {
        let host = header(request, "Host").ok_or_else(|| ApiError::request("Host is required"))?;
        if ![
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ]
        .iter()
        .any(|accepted| accepted.eq_ignore_ascii_case(host))
        {
            return Err(ApiError::request(
                "Host is not the bound loopback authority",
            ));
        }
        Ok(())
    }

    fn query(&self, body: &str) -> Result<Reply, ApiError> {
        let query: Query = serde_json::from_str(body).map_err(ApiError::request)?;
        let body = match query {
            Query::Snapshot => {
                serde_json::to_vec(&self.workbench.snapshot().map_err(ApiError::internal)?)
            }
            Query::Records { scope, search } => serde_json::to_vec(
                &self
                    .workbench
                    .records(scope.as_ref(), search.as_deref())
                    .map_err(ApiError::internal)?,
            ),
            Query::Relationships { identity } => serde_json::to_vec(
                &self
                    .workbench
                    .relationships(&identity)
                    .map_err(ApiError::internal)?,
            ),
            Query::Boards { scope } => {
                serde_json::to_vec(&self.workbench.boards(&scope).map_err(ApiError::internal)?)
            }
            Query::Workspace { context } => serde_json::to_vec(
                &self
                    .workbench
                    .workspace(&context)
                    .map_err(ApiError::internal)?,
            ),
            Query::Diagnostics => {
                serde_json::to_vec(&self.workbench.diagnostics().map_err(ApiError::internal)?)
            }
        }
        .map_err(ApiError::internal)?;
        Ok(Reply {
            status: 200,
            content_type: "application/json",
            body,
        })
    }

    fn preview(&self, body: &str) -> Result<Reply, ApiError> {
        let request: ChangeRequest = serde_json::from_str(body).map_err(ApiError::request)?;
        Reply::json(
            &self
                .workbench
                .preview(request)
                .map_err(ApiError::provider)?,
        )
    }

    fn apply(&self, request: &Parts, body: &str) -> Result<Reply, ApiError> {
        if !self.write {
            return Err(ApiError::forbidden("writes were not granted at launch"));
        }
        if header(request, CAPABILITY_HEADER) != Some(self.capability.as_str()) {
            return Err(ApiError::forbidden(
                "write capability is missing or invalid",
            ));
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ApplyRequest {
            preview_id: String,
            context: Option<crate::workbench::workspace::WorkspaceContext>,
        }
        let request: ApplyRequest = serde_json::from_str(body).map_err(ApiError::request)?;
        #[cfg(test)]
        if let Some(before_apply) = &self.before_apply {
            before_apply();
        }
        let outcome = self
            .workbench
            .apply(&request.preview_id, request.context.as_ref())
            .map_err(ApiError::provider)?;
        Reply::json(&outcome)
    }
}

fn asset_reply(path: &str, content_type: &'static str) -> Reply {
    Reply {
        status: 200,
        content_type,
        body: assets::get(path)
            .expect("static asset route has an embedded asset")
            .to_vec(),
    }
}

fn header<'a>(request: &'a Parts, name: &'static str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    fn copy_tree(from: &Path, to: &Path) {
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                fs::create_dir_all(&target).unwrap();
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn records_query_preserves_full_editable_draft_rendered_html_and_body_search() {
        use super::*;
        use casefile_core::{Kind, RecordDraft, WorkItemDraft};
        use casefile_store::{Provider, Store};
        use casefile_store_sqlite::SqliteIndex;
        use tempfile::TempDir;
        let root = TempDir::new().unwrap();
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../casefile-store/tests/fixtures/minimum"),
            root.path(),
        );
        let external = TempDir::new().unwrap();
        let database = external.path().join("index.sqlite");
        let provider = Provider::new(
            Store::open(root.path()).unwrap(),
            SqliteIndex::open(&database, root.path()).unwrap(),
        );
        let host = Host::new(Workbench::new(provider), 0, false, String::new());
        let reply = host.query(r#"{"query":"records","scope":{"project":"demo","investigation":"sample"},"search":"rEqUiReD"}"#).unwrap_or_else(|error| panic!("{}", error.message));
        let json: serde_json::Value = serde_json::from_slice(&reply.body).unwrap();
        let records = json["Current"]["value"].as_array().unwrap();
        let record = records
            .iter()
            .find(|record| {
                record["path"]
                    .as_str()
                    .unwrap()
                    .ends_with("tickets/accepted/HMD-011.md")
            })
            .unwrap();
        let path = record["path"].as_str().unwrap();
        let source = fs::read_to_string(root.path().join(path)).unwrap();
        let RecordDraft::Ticket(expected) =
            casefile_core::parse_draft(path, Kind::Ticket, &source).unwrap()
        else {
            panic!("ticket")
        };
        let actual: WorkItemDraft = serde_json::from_value(record["work_item"].clone()).unwrap();
        assert_eq!(expected, actual);
        assert_eq!(record["content"], source);
        assert_eq!(
            record["rendered_markdown"],
            casefile_core::render_markdown_html(&source)
        );
        assert_eq!(
            record["search_text"],
            format!("{}\n{source}", expected.title)
        );
    }
    #[test]
    fn rollback_error_crosses_the_http_failure_channel_as_409_without_private_source_contents() {
        use super::*;
        use casefile_store::*;
        use std::io::{Read, Write};
        let details = IncompleteRollback {
            code: RollbackErrorCode::IncompleteRollback,
            operation: "record batch verification".into(),
            cause: RollbackCause::Io,
            affected_paths: vec![RollbackPathState {
                path: "tickets/accepted/HMD-1.md".into(),
                remaining: RollbackRemainingState::Unknown,
                reason: RollbackReason::ObservationFailed,
            }],
        };
        let error = ProviderError::Store(StoreError::IncompleteRollback {
            details: details.clone(),
            cause: Box::new(StoreError::Invalid("PRIVATE SOURCE CONTENTS".into())),
        });
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let pending = std::thread::spawn(move || {
            let runtime = crate::transport::runtime().unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let (socket, _) = listener.accept().await.unwrap();
                let response =
                    std::sync::Mutex::new(Some(Reply::error(ApiError::provider(error)).response()));
                let service = hyper::service::service_fn(move |_| {
                    let value = response.lock().unwrap().take().unwrap();
                    async move { Ok::<_, std::convert::Infallible>(value) }
                });
                hyper::server::conn::http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(socket), service)
                    .await
                    .unwrap();
            });
        });
        let mut client = std::net::TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        client.write_all(b"POST /api/apply HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        pending.join().unwrap();
        assert!(response.starts_with("HTTP/1.1 409 "));
        let (_, body) = response.split_once("\r\n\r\n").unwrap();
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(value["code"], "incomplete_rollback");
        assert_eq!(
            serde_json::from_value::<IncompleteRollback>(value["details"].clone()).unwrap(),
            details
        );
        assert!(!response.contains("PRIVATE SOURCE CONTENTS"));
    }
    struct Fixture {
        root: tempfile::TempDir,
        _indexes: tempfile::TempDir,
        database: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::TempDir::new().unwrap();
            copy_tree(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../casefile-store/tests/fixtures/minimum"),
                root.path(),
            );
            assert!(
                std::process::Command::new("git")
                    .args(["init", "-q"])
                    .current_dir(root.path())
                    .status()
                    .unwrap()
                    .success()
            );
            let indexes = tempfile::TempDir::new().unwrap();
            let database = indexes.path().join("index.sqlite");
            Self {
                root,
                _indexes: indexes,
                database,
            }
        }
        fn provider(&self) -> casefile_store::Provider<casefile_store_sqlite::SqliteIndex> {
            casefile_store::Provider::new(
                casefile_store::Store::open(self.root.path()).unwrap(),
                casefile_store_sqlite::SqliteIndex::open(&self.database, self.root.path()).unwrap(),
            )
        }
        fn host(&self, port: u16) -> super::Host {
            super::Host::new(
                crate::workbench::Workbench::new(self.provider()),
                port,
                true,
                "write-test".into(),
            )
        }
    }

    const TICKET: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";

    fn query(host: &super::Host, body: serde_json::Value) -> serde_json::Value {
        let reply = host
            .query(&body.to_string())
            .unwrap_or_else(|error| panic!("{}", error.message));
        serde_json::from_slice(&reply.body).unwrap()
    }

    #[test]
    fn workspace_tokens_distinguish_same_source_replacement_and_provider_restart() {
        use casefile_store::DerivedIndex;
        let fixture = Fixture::new();
        let host = fixture.host(0);
        let first = query(&host, serde_json::json!({"query":"workspace"}));
        assert_eq!(first["state"], "updated");
        let token = first["freshness"].clone();
        let unchanged = query(
            &host,
            serde_json::json!({"query":"workspace","known_token":token,"search":"rEqUiReD"}),
        );
        assert_eq!(unchanged["state"], "unchanged");
        assert!(unchanged.get("records").is_none());
        assert!(unchanged.get("diagnostics").is_none());
        assert!(
            unchanged["matching_paths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|path| path == TICKET)
        );
        let foreign = fixture.host(0);
        let other = query(
            &foreign,
            serde_json::json!({"query":"workspace","known_token":token}),
        );
        assert_eq!(other["state"], "updated");
        assert_eq!(
            other["freshness"]["source_revision"],
            token["source_revision"]
        );
        assert_ne!(
            other["freshness"]["provider_instance"],
            token["provider_instance"]
        );

        let store = casefile_store::Store::open(fixture.root.path()).unwrap();
        let index =
            casefile_store_sqlite::SqliteIndex::open(&fixture.database, fixture.root.path())
                .unwrap();
        let snapshot = store.derived_snapshot().unwrap();
        let bytes = fs::read(&fixture.database).unwrap();
        index
            .publish(index.prepare(&snapshot).unwrap(), &store)
            .unwrap();
        assert_eq!(bytes, fs::read(&fixture.database).unwrap());
        // Another query has already observed the new publication; an old browser still needs full data.
        let _ = query(&host, serde_json::json!({"query":"records"}));
        let replaced = query(
            &host,
            serde_json::json!({"query":"workspace","known_token":token}),
        );
        assert_eq!(replaced["state"], "updated");
        assert_eq!(
            replaced["freshness"]["source_revision"],
            token["source_revision"]
        );
        assert_ne!(
            replaced["freshness"]["publication_id"],
            token["publication_id"]
        );
        assert!(
            host.query(r#"{"query":"workspace","unexpected":true}"#)
                .is_err()
        );
    }

    #[test]
    fn callback_replacement_and_canonical_edit_refuse_partial_current_data() {
        use casefile_store::DerivedIndex;
        let fixture = Fixture::new();
        let provider = fixture.provider();
        provider
            .read_full_index(|_, _| Ok::<_, String>(()))
            .unwrap();
        let store = casefile_store::Store::open(fixture.root.path()).unwrap();
        let replacement =
            casefile_store_sqlite::SqliteIndex::open(&fixture.database, fixture.root.path())
                .unwrap();
        let snapshot = store.derived_snapshot().unwrap();
        assert!(
            provider
                .read_full_index(|_, _| {
                    replacement
                        .publish(replacement.prepare(&snapshot).unwrap(), &store)
                        .unwrap();
                    Ok::<_, String>("obsolete projection")
                })
                .is_err()
        );
        assert!(
            provider
                .read_full_index(|_, _| {
                    fs::write(
                        fixture.root.path().join("external.md"),
                        "external edit during query",
                    )
                    .unwrap();
                    Ok::<_, String>("obsolete projection")
                })
                .is_err()
        );
    }

    #[test]
    fn committed_apply_projection_failure_stays_success_and_old_payload_is_refused() {
        use casefile_core::{ChangeRequest, Kind};
        let fixture = Fixture::new();
        let provider = fixture.provider();
        let original = fs::read_to_string(fixture.root.path().join(TICKET)).unwrap();
        let mut draft = casefile_core::parse_draft(TICKET, Kind::Ticket, &original).unwrap();
        if let casefile_core::RecordDraft::Ticket(item) = &mut draft {
            item.title = "Committed despite projection failure".into();
        }
        let preview = provider
            .preview_record(ChangeRequest::Replace {
                path: TICKET.into(),
                draft,
            })
            .unwrap();
        let (outcome, projection) = provider
            .apply_record_with_index(&preview.preview_id, |_, _| {
                Err::<(), _>("projection unavailable")
            })
            .unwrap();
        assert!(matches!(
            outcome.cache,
            casefile_store::CacheState::Degraded { .. }
        ));
        assert!(projection.is_none());
        assert!(
            fs::read_to_string(fixture.root.path().join(TICKET))
                .unwrap()
                .contains("Committed despite projection failure")
        );
        let host = fixture.host(1);
        let parts = hyper::Request::builder()
            .method("POST")
            .uri("/api/apply")
            .header("Host", "127.0.0.1:1")
            .header("Content-Type", "application/json")
            .header(super::CAPABILITY_HEADER, "write-test")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        assert!(host.apply(&parts, &serde_json::json!({"preview_id":preview.preview_id,"canonical":"obsolete approval bytes"}).to_string()).is_err());
    }
    struct Loopback {
        runtime: tokio::runtime::Runtime,
        server: tokio::task::JoinHandle<anyhow::Result<()>>,
        address: std::net::SocketAddr,
    }
    impl Loopback {
        fn start(fixture: &Fixture, hook: Option<Box<dyn Fn() + Send + Sync>>) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let mut host = fixture.host(address.port());
            host.before_apply = hook;
            let runtime = crate::transport::runtime().unwrap();
            let server = {
                let _entered = runtime.enter();
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                runtime.spawn(crate::transport::serve(listener, std::sync::Arc::new(host)))
            };
            Self {
                runtime,
                server,
                address,
            }
        }
        fn connect(&self) -> std::net::TcpStream {
            let stream = std::net::TcpStream::connect(self.address).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(12)))
                .unwrap();
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(12)))
                .unwrap();
            stream
        }
        fn head(&self, path: &str, length: usize) -> String {
            format!(
                "POST {path} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nX-Casefile-Write-Capability: write-test\r\nContent-Length: {length}\r\n\r\n",
                self.address
            )
        }
        fn post(&self, path: &str, body: &str) -> (u16, serde_json::Value) {
            use std::io::{Read, Write};
            let mut stream = self.connect();
            stream
                .write_all(self.head(path, body.len()).as_bytes())
                .unwrap();
            stream.write_all(body.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            let (head, body) = response.split_once("\r\n\r\n").unwrap();
            let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
            (status, serde_json::from_str(body).unwrap())
        }
    }
    impl Drop for Loopback {
        fn drop(&mut self) {
            self.server.abort();
            let _ = self.runtime.block_on(&mut self.server);
        }
    }

    #[test]
    fn stalled_headers_and_body_expire_while_unrelated_and_large_requests_succeed() {
        use std::io::{Read, Write};
        let fixture = Fixture::new();
        let server = Loopback::start(&fixture, None);
        let mut header = server.connect();
        header
            .write_all(b"POST /api/query HTTP/1.1\r\nHost:")
            .unwrap();
        let mut body = server.connect();
        body.write_all(server.head("/api/query", 100).as_bytes())
            .unwrap();
        body.write_all(b"{").unwrap();
        let mut large = String::from(r#"{"query":"snapshot"}"#);
        large.push_str(&" ".repeat(8 * 1024 * 1024 + 1));
        assert_eq!(server.post("/api/query", &large).0, 200);
        assert_eq!(server.post("/api/query", r#"{"query":"snapshot"}"#).0, 200);
        std::thread::sleep(std::time::Duration::from_secs(32));
        let mut closed = Vec::new();
        assert_eq!(header.read_to_end(&mut closed).unwrap(), 0);
        closed.clear();
        body.read_to_end(&mut closed).unwrap();
        assert!(
            closed.is_empty()
                || String::from_utf8(closed)
                    .unwrap()
                    .starts_with("HTTP/1.0 400 ")
        );
    }

    #[test]
    fn progressing_body_may_take_longer_than_the_idle_deadline() {
        use std::io::{Read, Write};
        let fixture = Fixture::new();
        let server = Loopback::start(&fixture, None);
        let body = r#"{"query":"snapshot"}"#;
        let mut stream = server.connect();
        stream
            .write_all(server.head("/api/query", body.len()).as_bytes())
            .unwrap();
        let started = std::time::Instant::now();
        for chunk in body.as_bytes().chunks(5) {
            stream.write_all(chunk).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(8));
        }
        assert!(started.elapsed() > std::time::Duration::from_secs(30));
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.0 200 "));
        assert_eq!(server.post("/api/query", r#"{"query":"snapshot"}"#).0, 200);
    }

    #[test]
    fn admitted_apply_survives_disconnected_client_beyond_io_deadline() {
        use std::io::Write;
        let fixture = Fixture::new();
        let (admitted, observed) = std::sync::mpsc::sync_channel(1);
        let (release, resume) = std::sync::mpsc::sync_channel(1);
        let resume = std::sync::Mutex::new(resume);
        let first_apply = std::sync::atomic::AtomicBool::new(true);
        let server = Loopback::start(
            &fixture,
            Some(Box::new(move || {
                if first_apply.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    admitted.send(()).unwrap();
                    resume
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(60))
                        .unwrap();
                }
            })),
        );
        let original = fs::read_to_string(fixture.root.path().join(TICKET)).unwrap();
        let mut draft =
            casefile_core::parse_draft(TICKET, casefile_core::Kind::Ticket, &original).unwrap();
        if let casefile_core::RecordDraft::Ticket(item) = &mut draft {
            item.title = "Disconnected admitted apply".into();
        }
        let (status, preview) = server.post(
            "/api/preview",
            &serde_json::to_string(&casefile_core::ChangeRequest::Replace {
                path: TICKET.into(),
                draft,
            })
            .unwrap(),
        );
        assert_eq!(status, 200);
        let body = serde_json::json!({"preview_id":preview["preview_id"]}).to_string();
        let mut client = server.connect();
        client
            .write_all(server.head("/api/apply", body.len()).as_bytes())
            .unwrap();
        client.write_all(body.as_bytes()).unwrap();
        observed
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        drop(client);
        assert_eq!(server.post("/api/query", r#"{"query":"snapshot"}"#).0, 200);
        std::thread::sleep(std::time::Duration::from_secs(32));
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !fs::read_to_string(fixture.root.path().join(TICKET))
            .unwrap()
            .contains("Disconnected admitted apply")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "admitted canonical work was abandoned"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (status, records) = server.post(
            "/api/query",
            r#"{"query":"records","search":"Disconnected admitted apply"}"#,
        );
        assert_eq!(status, 200);
        assert!(
            records["Current"]["value"]
                .as_array()
                .unwrap()
                .iter()
                .any(|record| record["path"] == TICKET)
        );
        let (status, replay) = server.post("/api/apply", &body);
        assert_eq!(status, 409);
        assert_eq!(replay["code"], "stale_revision");
        assert!(
            fs::read_to_string(fixture.root.path().join(TICKET))
                .unwrap()
                .contains("Disconnected admitted apply")
        );
    }

    #[test]
    fn non_reader_write_timeout_does_not_hold_unrelated_requests() {
        use std::io::{Read, Write};
        let fixture = Fixture::new();
        fs::write(
            fixture.root.path().join("large.md"),
            "ordinary large response ".repeat(800_000),
        )
        .unwrap();
        let server = Loopback::start(&fixture, None);
        let body = r#"{"query":"records","search":"ordinary large response"}"#;
        let mut client = server.connect();
        client
            .write_all(server.head("/api/query", body.len()).as_bytes())
            .unwrap();
        client.write_all(body.as_bytes()).unwrap();
        assert_eq!(server.post("/api/query", r#"{"query":"snapshot"}"#).0, 200);
        std::thread::sleep(std::time::Duration::from_secs(36));
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let split = response
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap();
        let head = String::from_utf8(response[..split].to_vec()).unwrap();
        let length = head
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        assert!(
            response.len() - split - 4 < length,
            "non-reader unexpectedly received a complete large body"
        );
        assert_eq!(server.post("/api/query", r#"{"query":"snapshot"}"#).0, 200);
    }
    #[test]
    fn real_http_apply_returns_the_committed_workspace_without_another_refresh() {
        let fixture = Fixture::new();
        let server = Loopback::start(&fixture, None);
        let (status, baseline) = server.post("/api/query", r#"{"query":"workspace"}"#);
        assert_eq!(status, 200);
        let original = fs::read_to_string(fixture.root.path().join(TICKET)).unwrap();
        let mut draft =
            casefile_core::parse_draft(TICKET, casefile_core::Kind::Ticket, &original).unwrap();
        if let casefile_core::RecordDraft::Ticket(item) = &mut draft {
            item.title = "HTTP committed projection".into();
        }
        let (status, preview) = server.post(
            "/api/preview",
            &serde_json::to_string(&casefile_core::ChangeRequest::Replace {
                path: TICKET.into(),
                draft,
            })
            .unwrap(),
        );
        assert_eq!(status, 200);
        assert!(preview.get("request").is_none());
        assert!(preview.get("rendered_bytes").is_none());
        let (status, outcome) = server.post("/api/apply", &serde_json::json!({"preview_id":preview["preview_id"],"context":{"known_token":baseline["freshness"],"search":"HTTP committed projection"}}).to_string());
        assert_eq!(status, 200, "{outcome}");
        assert_eq!(outcome["workspace"]["state"], "updated", "{outcome}");
        assert!(
            outcome["workspace"]["matching_paths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|path| path == TICKET)
        );
        assert!(
            outcome["workspace"]["records"]
                .as_array()
                .unwrap()
                .iter()
                .any(|record| record["title"] == "HTTP committed projection")
        );
        let (status, _) = server.post(
            "/api/apply",
            &serde_json::json!({"preview_id":preview["preview_id"],"rendered_bytes":[1,2,3]})
                .to_string(),
        );
        assert_eq!(status, 400);
        assert!(
            fs::read_to_string(fixture.root.path().join(TICKET))
                .unwrap()
                .contains("HTTP committed projection")
        );
    }
}
