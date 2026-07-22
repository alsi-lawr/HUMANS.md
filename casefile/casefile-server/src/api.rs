use crate::{assets, workbench::Workbench};
use anyhow::Result;
use casefile_core::{ApplyResult, ChangeRequest, Preview};
use casefile_store::{RecordScope, ScopedIdentity};
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request, Response, StatusCode};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
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
            code: None,
        }
    }
    fn store(error: casefile_store::StoreError) -> Self {
        let stale = matches!(
            error,
            casefile_store::StoreError::StaleStoreRevision
                | casefile_store::StoreError::StaleTargetRevision
        );
        Self {
            status: if stale { 409 } else { 400 },
            message: error.to_string(),
            code: stale.then_some("stale_revision"),
        }
    }
    fn forbidden(message: &str) -> Self {
        Self {
            status: 403,
            message: message.into(),
            code: None,
        }
    }
    fn internal(error: impl ToString) -> Self {
        Self {
            status: 500,
            message: error.to_string(),
            code: None,
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
            }),
            _ => Err(ApiError {
                status: 404,
                message: "route not found".into(),
                code: None,
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
        Reply::json(&self.workbench.preview(request).map_err(ApiError::request)?)
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
        let outcome = self.workbench.apply(preview).map_err(ApiError::store)?;
        Reply::json(&ApplyResponse {
            result: outcome.result,
            index_error: outcome.index_error.map(|error| error.to_string()),
        })
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
