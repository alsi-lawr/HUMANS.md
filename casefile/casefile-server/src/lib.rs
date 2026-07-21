use anyhow::{Context, Result, bail};
use casefile_core::{ApplyResult, ChangeRequest, Preview, Revision};
use casefile_store::{DerivedIndex, Indexed, RecordScope, ScopedIdentity, Store};
use casefile_store_sqlite::SqliteIndex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const INDEX_HTML: &str = include_str!("index.html");
const CAPABILITY_HEADER: &str = "X-Casefile-Write-Capability";

#[derive(Deserialize)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
enum Query {
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
struct ApplyResponse {
    result: ApplyResult,
    index_error: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

struct Host {
    store: Store,
    index: SqliteIndex,
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
            })
            .expect("error response serializes"),
        }
    }
}

impl ApiError {
    fn request(error: impl ToString) -> Self {
        Self {
            status: 400,
            message: error.to_string(),
        }
    }
    fn forbidden(message: &str) -> Self {
        Self {
            status: 403,
            message: message.into(),
        }
    }
    fn internal(error: impl ToString) -> Self {
        Self {
            status: 500,
            message: error.to_string(),
        }
    }
}

impl Host {
    fn handle(&self, mut request: Request) -> Result<()> {
        let reply = self.route(&mut request).unwrap_or_else(Reply::error);
        let content_type =
            Header::from_bytes("Content-Type", reply.content_type).expect("static header is valid");
        request.respond(
            Response::from_data(reply.body)
                .with_status_code(StatusCode(reply.status))
                .with_header(content_type),
        )?;
        Ok(())
    }

    fn route(&self, request: &mut Request) -> Result<Reply, ApiError> {
        self.validate_authority(request)?;
        let method = request.method().clone();
        let path = request.url().to_owned();
        match (method, path.as_str()) {
            (Method::Get, "/") => Ok(Reply {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: INDEX_HTML.as_bytes().to_vec(),
            }),
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
            (_, "/" | "/api/query" | "/api/preview" | "/api/apply") => Err(ApiError {
                status: 405,
                message: "method not allowed".into(),
            }),
            _ => Err(ApiError {
                status: 404,
                message: "route not found".into(),
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
        if let Some(origin) = header(request, "Origin") {
            let authority = origin
                .strip_prefix("http://")
                .or_else(|| origin.strip_prefix("https://"));
            if authority.is_none_or(|authority| !authority.eq_ignore_ascii_case(host)) {
                return Err(ApiError::forbidden("cross-origin requests are not allowed"));
            }
        }
        Ok(())
    }

    fn query(&self, body: &str) -> Result<Reply, ApiError> {
        let query: Query = serde_json::from_str(body).map_err(ApiError::request)?;
        let revision = self.refresh().map_err(ApiError::internal)?;
        let body = match query {
            Query::Records { scope, search } => serde_json::to_vec(
                &self
                    .index
                    .records(&revision, scope.as_ref(), search.as_deref())
                    .map_err(ApiError::internal)?,
            ),
            Query::Relationships { identity } => serde_json::to_vec(
                &self
                    .index
                    .relationships(&revision, &identity)
                    .map_err(ApiError::internal)?,
            ),
            Query::Boards { scope } => serde_json::to_vec(
                &self
                    .index
                    .boards(&revision, &scope)
                    .map_err(ApiError::internal)?,
            ),
            Query::Diagnostics => serde_json::to_vec(
                &self
                    .index
                    .diagnostics(&revision)
                    .map_err(ApiError::internal)?,
            ),
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
        Reply::json(&self.store.preview(request).map_err(ApiError::request)?)
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
        let preview: Preview = serde_json::from_str(body).map_err(ApiError::request)?;
        let result = self.store.apply(preview).map_err(ApiError::request)?;
        let index_error = self.refresh().err().map(|error| error.to_string());
        Reply::json(&ApplyResponse {
            result,
            index_error,
        })
    }

    fn refresh(&self) -> Result<Revision> {
        let snapshot = self.store.derived_snapshot()?;
        match self.index.state(&snapshot.source_revision)? {
            Indexed::Current { .. } => {}
            Indexed::Missing | Indexed::Stale { .. } => {
                let prepared = self.index.prepare(&snapshot)?;
                if !matches!(
                    self.index.publish(prepared, &self.store)?,
                    Indexed::Current { .. }
                ) {
                    bail!("canonical content changed during index refresh");
                }
            }
        }
        Ok(snapshot.source_revision)
    }
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn capability() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn default_index_path(root: &Path) -> Result<PathBuf> {
    let directory = std::env::temp_dir().join("casefile-sqlite-indexes");
    fs::create_dir_all(&directory)?;
    let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
    let key = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(directory.join(format!("{key}.sqlite")))
}

pub fn serve(root: &Path, port: u16, index: Option<&Path>, write: bool) -> Result<()> {
    let root = fs::canonicalize(root).context("canonicalize planning root")?;
    let index_path = match index {
        Some(path) if path.is_absolute() => path.to_owned(),
        Some(path) => std::env::current_dir()?.join(path),
        None => default_index_path(&root)?,
    };
    let index = SqliteIndex::open(&index_path, &root)?;
    let server = Server::http(("127.0.0.1", port)).map_err(|error| anyhow::anyhow!(error))?;
    let port = server
        .server_addr()
        .to_ip()
        .context("server did not bind an IP socket")?
        .port();
    let capability = capability()?;
    println!("Casefile server: http://127.0.0.1:{port}");
    println!("Casefile root: {}", root.display());
    println!("Casefile index: {}", index_path.display());
    println!("Casefile write capability: {capability}");
    std::io::stdout().flush()?;
    let host = Host {
        store: Store::open(root)?,
        index,
        port,
        write,
        capability,
    };
    for request in server.incoming_requests() {
        if let Err(error) = host.handle(request) {
            eprintln!("HTTP response failed: {error}");
        }
    }
    Ok(())
}
