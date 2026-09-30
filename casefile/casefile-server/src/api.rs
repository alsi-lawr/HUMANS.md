use crate::{assets, workbench::Workbench};
use anyhow::Result;
use casefile_core::ChangeRequest;
use casefile_store::{ProviderError, RecordScope, ScopedIdentity};
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request, Response, StatusCode};

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
    fn respond(self, request: Request) -> Result<()> {
        let content_type =
            Header::from_bytes("Content-Type", self.content_type).expect("static header is valid");
        request.respond(
            Response::from_data(self.body)
                .with_status_code(StatusCode(self.status))
                .with_header(content_type),
        )?;
        Ok(())
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
        }
    }

    pub(crate) fn handle(&self, mut request: Request) -> Result<()> {
        let reply = self.route(&mut request).unwrap_or_else(Reply::error);
        reply.respond(request)
    }

    fn route(&self, request: &mut Request) -> Result<Reply, ApiError> {
        self.validate_authority(request)?;
        let method = request.method().clone();
        let path = request.url().to_owned();
        match (method, path.as_str()) {
            (Method::Get, "/") => Ok(asset_reply("/", "text/html; charset=utf-8")),
            (Method::Get, "/assets/app.js") => Ok(asset_reply(
                "/assets/app.js",
                "text/javascript; charset=utf-8",
            )),
            (Method::Get, "/assets/app.css") => {
                Ok(asset_reply("/assets/app.css", "text/css; charset=utf-8"))
            }
            (Method::Post, path @ ("/api/query" | "/api/preview" | "/api/apply")) => {
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
                let mut body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut body)
                    .map_err(ApiError::request)?;
                match path {
                    "/api/query" => self.query(&body),
                    "/api/preview" => self.preview(&body),
                    _ => self.apply(request, &body),
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

    fn validate_authority(&self, request: &Request) -> Result<(), ApiError> {
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

    fn apply(&self, request: &Request, body: &str) -> Result<Reply, ApiError> {
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
        }
        let request: ApplyRequest = serde_json::from_str(body).map_err(ApiError::request)?;
        let outcome = self
            .workbench
            .apply(&request.preview_id)
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

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
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
        let host = Host::new(
            Workbench::new(provider, SqliteIndex::open(&database, root.path()).unwrap()),
            0,
            false,
            String::new(),
        );
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
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = server.server_addr().to_ip().unwrap();
        let pending = std::thread::spawn(move || {
            let request = server.recv().unwrap();
            Reply::error(ApiError::provider(error))
                .respond(request)
                .unwrap();
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
}
