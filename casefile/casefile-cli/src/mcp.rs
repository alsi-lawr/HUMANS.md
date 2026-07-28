use anyhow::{Context, Result, bail};
use casefile_core::ChangeRequest;
use casefile_store::{
    ActivationState, DefaultBoardPreview, PROVIDER_PROTOCOL_VERSION, ProgressOperation, Provider,
    ProviderBatchPreview, ProviderCapabilities, ProviderMutationState, ProviderOperation,
    ProviderPreview, ProviderProgressPreview, ProviderStrategyTransitionPreview,
    ProviderWriterBindingPreview, Store, StrategyTransitionRequest, WriterBindingRequest,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock, mpsc},
    thread,
};

const MCP_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const TOOL_WORKERS: usize = 16;
const PREVIEW_LIMIT: usize = 256;
const REQUIRED_PROVIDER_OPERATIONS: &[&str] = &[
    "snapshot",
    "query_tickets",
    "query_epics",
    "query_boards",
    "query_progress",
    "query_strategy_transitions",
    "preview_record_draft",
    "apply_record_draft",
    "bootstrap_progress",
    "preview_progress",
    "apply_progress",
    "preview_default_delivery_board",
    "apply_default_delivery_board",
    "preview_strategy_transition",
    "apply_strategy_transition",
    "preview_writer_binding",
    "apply_writer_binding",
];

#[derive(Serialize)]
struct Compatibility<'a> {
    identity: &'a str,
    adapter_protocol_version: u32,
    provider_protocol_version: u32,
    required_provider_operations: &'a [&'a str],
    mcp_protocol_versions: &'a [&'a str],
}

fn compatibility() -> Compatibility<'static> {
    Compatibility {
        identity: "casefile",
        adapter_protocol_version: 1,
        provider_protocol_version: PROVIDER_PROTOCOL_VERSION,
        required_provider_operations: REQUIRED_PROVIDER_OPERATIONS,
        mcp_protocol_versions: MCP_PROTOCOL_VERSIONS,
    }
}

pub(super) fn print_compatibility() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&compatibility())?);
    Ok(())
}

pub(super) fn serve_package(planning_root: &Path) -> Result<()> {
    let required = REQUIRED_PROVIDER_OPERATIONS.join(",");
    serve(
        planning_root,
        planning_root,
        PROVIDER_PROTOCOL_VERSION,
        &required,
    )
}

pub(super) fn serve(
    planning_root: &Path,
    expected_root: &Path,
    expected_provider_protocol: u32,
    required_provider_operations: &str,
) -> Result<()> {
    let root = fixed_root(planning_root, expected_root)?;
    let required = parse_required_operations(required_provider_operations)?;
    if expected_provider_protocol != PROVIDER_PROTOCOL_VERSION {
        bail!(
            "launcher requires provider protocol {expected_provider_protocol}, but this adapter supports {PROVIDER_PROTOCOL_VERSION}"
        );
    }
    let provider =
        Provider::without_cache(Store::open(&root).context("open explicit planning root")?);
    let baseline = provider
        .snapshot_for_protocol(expected_provider_protocol)
        .context("establish provider protocol baseline")?;
    validate_baseline(
        &baseline.capabilities,
        baseline.activation,
        &baseline.diagnostics,
        &required,
    )?;
    Session::new(provider).run()
}

fn fixed_root(planning_root: &Path, expected_root: &Path) -> Result<PathBuf> {
    if planning_root.as_os_str().is_empty() || expected_root.as_os_str().is_empty() {
        bail!("planning root and expected root must be non-empty");
    }
    if !planning_root.is_absolute() || !expected_root.is_absolute() {
        bail!("planning root and expected root must be absolute paths");
    }
    let planning = fs::canonicalize(planning_root)
        .with_context(|| format!("canonicalize planning root {}", planning_root.display()))?;
    let expected = fs::canonicalize(expected_root)
        .with_context(|| format!("canonicalize expected root {}", expected_root.display()))?;
    if planning != expected {
        bail!(
            "planning root {} conflicts with launcher contract {}",
            planning.display(),
            expected.display()
        );
    }
    if !planning.is_dir() {
        bail!("planning root {} is not a directory", planning.display());
    }
    Ok(planning)
}

fn parse_required_operations(value: &str) -> Result<BTreeSet<String>> {
    if value.is_empty() {
        bail!("launcher supplied no required provider operations");
    }
    let items = value.split(',').map(str::trim).collect::<Vec<_>>();
    if items.iter().any(|item| item.is_empty()) {
        bail!("launcher supplied an empty required provider operation");
    }
    let required = items
        .iter()
        .map(|item| (*item).to_owned())
        .collect::<BTreeSet<_>>();
    if required.len() != items.len() {
        bail!("launcher supplied duplicate required provider operations");
    }
    let adapter = REQUIRED_PROVIDER_OPERATIONS
        .iter()
        .map(|item| (*item).to_owned())
        .collect::<BTreeSet<_>>();
    if required != adapter {
        bail!("launcher provider capability contract differs from this adapter");
    }
    Ok(required)
}

fn validate_baseline(
    capabilities: &ProviderCapabilities,
    activation: ActivationState,
    diagnostics: &[casefile_core::Diagnostic],
    required: &BTreeSet<String>,
) -> Result<()> {
    if activation != ActivationState::Active {
        bail!("explicit planning root is unactivated, invalid, or unsupported");
    }
    if !diagnostics.is_empty() {
        bail!("explicit planning root is invalid; resolve Casefile diagnostics before MCP startup");
    }
    if capabilities.protocol_version != PROVIDER_PROTOCOL_VERSION {
        bail!("provider protocol baseline is incompatible");
    }
    if !matches!(capabilities.mutation, ProviderMutationState::ReadWrite) {
        bail!("provider mutation capability is unavailable");
    }
    if !capabilities.writes_require_external_approval {
        bail!("provider external-approval capability is missing");
    }
    let advertised = capabilities
        .operations
        .iter()
        .map(operation_name)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let missing = required
        .difference(&advertised)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "provider is missing required operations: {}",
            missing.join(", ")
        );
    }
    Ok(())
}

fn operation_name(operation: &ProviderOperation) -> &'static str {
    match operation {
        ProviderOperation::Snapshot => "snapshot",
        ProviderOperation::QueryTickets => "query_tickets",
        ProviderOperation::QueryEpics => "query_epics",
        ProviderOperation::QueryBoards => "query_boards",
        ProviderOperation::QueryProgress => "query_progress",
        ProviderOperation::QueryStrategyTransitions => "query_strategy_transitions",
        ProviderOperation::PreviewRecordDraft => "preview_record_draft",
        ProviderOperation::ApplyRecordDraft => "apply_record_draft",
        ProviderOperation::BootstrapProgress => "bootstrap_progress",
        ProviderOperation::PreviewProgress => "preview_progress",
        ProviderOperation::ApplyProgress => "apply_progress",
        ProviderOperation::PreviewDefaultDeliveryBoard => "preview_default_delivery_board",
        ProviderOperation::ApplyDefaultDeliveryBoard => "apply_default_delivery_board",
        ProviderOperation::PreviewStrategyTransition => "preview_strategy_transition",
        ProviderOperation::ApplyStrategyTransition => "apply_strategy_transition",
        ProviderOperation::PreviewWriterBinding => "preview_writer_binding",
        ProviderOperation::ApplyWriterBinding => "apply_writer_binding",
    }
}

struct Session {
    tools: ToolService,
    initialized: bool,
}

#[derive(Clone)]
struct ToolService {
    provider: Arc<Provider>,
    access: Arc<RwLock<()>>,
    previews: Arc<Mutex<PreviewVault>>,
}

struct QueuedToolCall {
    request_id: Value,
    params: Option<Value>,
}

#[derive(Clone)]
enum StoredPreview {
    Record(ProviderPreview),
    RecordBatch(ProviderBatchPreview),
    Progress(ProviderProgressPreview),
    Board(DefaultBoardPreview),
    StrategyTransition(ProviderStrategyTransitionPreview),
    WriterBinding(ProviderWriterBindingPreview),
}

#[derive(Clone)]
struct PreviewEntry {
    public: Value,
    internal: StoredPreview,
}

#[derive(Default)]
struct PreviewVault {
    order: VecDeque<String>,
    values: BTreeMap<String, PreviewEntry>,
}

impl Session {
    fn new(provider: Provider) -> Self {
        Self {
            tools: ToolService {
                provider: Arc::new(provider),
                access: Arc::new(RwLock::new(())),
                previews: Arc::new(Mutex::new(PreviewVault::default())),
            },
            initialized: false,
        }
    }

    fn run(mut self) -> Result<()> {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let output = Arc::new(Mutex::new(io::stdout()));
        let (sender, receiver) = mpsc::channel::<QueuedToolCall>();
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..TOOL_WORKERS)
            .map(|_| {
                let receiver = Arc::clone(&receiver);
                let output = Arc::clone(&output);
                let tools = self.tools.clone();
                thread::spawn(move || -> Result<()> {
                    loop {
                        let call = {
                            let receiver = receiver.lock().expect("MCP tool receiver");
                            receiver.recv()
                        };
                        let Ok(call) = call else {
                            return Ok(());
                        };
                        let response = tools.call_tool(call.request_id, call.params.as_ref());
                        let mut output = output.lock().expect("MCP stdout");
                        write_message(&mut *output, response)?;
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut line = String::new();
        let read_result = loop {
            line.clear();
            let bytes = match input.read_line(&mut line) {
                Ok(bytes) => bytes,
                Err(error) => break Err(error).context("read MCP stdio request"),
            };
            if bytes == 0 {
                break Ok(());
            }
            if bytes > MAX_MESSAGE_BYTES {
                break Err(anyhow::anyhow!(
                    "MCP stdio request exceeds {MAX_MESSAGE_BYTES} bytes"
                ));
            }
            let request: Value = match serde_json::from_str(line.trim_end()) {
                Ok(request) => request,
                Err(error) => {
                    let mut output = output.lock().expect("MCP stdout");
                    write_message(
                        &mut *output,
                        error_response(Value::Null, -32700, &format!("parse error: {error}")),
                    )?;
                    continue;
                }
            };
            if self.initialized && is_tool_call(&request) {
                let object = request.as_object().expect("validated tool call");
                sender
                    .send(QueuedToolCall {
                        request_id: object.get("id").cloned().expect("validated tool call"),
                        params: object.get("params").cloned(),
                    })
                    .context("queue MCP tool call")?;
                continue;
            }
            if let Some(response) = self.handle(request)? {
                let mut output = output.lock().expect("MCP stdout");
                write_message(&mut *output, response)?;
            }
        };
        drop(sender);
        for worker in workers {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("MCP tool worker panicked"))??;
        }
        read_result
    }

    fn handle(&mut self, request: Value) -> Result<Option<Value>> {
        let Some(object) = request.as_object() else {
            return Ok(Some(error_response(
                Value::Null,
                -32600,
                "MCP request must be an object",
            )));
        };
        if object.get("jsonrpc") != Some(&Value::String("2.0".into())) {
            return Ok(Some(error_response(
                id(object),
                -32600,
                "JSON-RPC version must be 2.0",
            )));
        }
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(request_id) = object.get("id").cloned() else {
            return Ok(None);
        };
        match method {
            "initialize" => Ok(Some(self.initialize(request_id, object.get("params")))),
            "ping" if self.initialized => Ok(Some(success_response(request_id, json!({})))),
            "tools/list" if self.initialized => Ok(Some(success_response(
                request_id,
                json!({"tools": tool_definitions()}),
            ))),
            "tools/call" if self.initialized => {
                Ok(Some(self.tools.call_tool(request_id, object.get("params"))))
            }
            _ if !self.initialized => Ok(Some(error_response(
                request_id,
                -32002,
                "MCP session is not initialized",
            ))),
            _ => Ok(Some(error_response(request_id, -32601, "method not found"))),
        }
    }

    fn initialize(&mut self, request_id: Value, params: Option<&Value>) -> Value {
        if self.initialized {
            return error_response(request_id, -32600, "MCP session is already initialized");
        }
        let protocol = params
            .and_then(|value| value.get("protocolVersion"))
            .and_then(Value::as_str);
        let Some(protocol) = protocol else {
            return error_response(request_id, -32602, "initialize requires protocolVersion");
        };
        if !MCP_PROTOCOL_VERSIONS.contains(&protocol) {
            return error_response(
                request_id,
                -32602,
                &format!("unsupported MCP protocol version {protocol}"),
            );
        }
        self.initialized = true;
        success_response(
            request_id,
            json!({
                "protocolVersion": protocol,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "casefile", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Casefile tools operate only on the explicit planning root bound to this process. Follow each tool's input schema exactly. Apply tools require external human approval and the matching preview tool's entire structuredContent passed unchanged as the preview value; never pass only preview_id."
            }),
        )
    }
}

impl ToolService {
    fn call_tool(&self, request_id: Value, params: Option<&Value>) -> Value {
        let name = params
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str);
        let Some(name) = name else {
            return error_response(request_id, -32602, "tools/call requires a tool name");
        };
        let arguments = params
            .and_then(|value| value.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let result = if is_apply_tool(name) {
            let _exclusive = self.access.write().expect("MCP tool access");
            self.dispatch(name, arguments)
        } else {
            let _shared = self.access.read().expect("MCP tool access");
            self.dispatch(name, arguments)
        };
        match result {
            Ok(value) => success_response(request_id, tool_result(value, false)),
            Err(error) => success_response(
                request_id,
                tool_result(json!({"error": format!("{error:#}")}), true),
            ),
        }
    }

    fn dispatch(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "casefile_snapshot" => serialize(self.provider.snapshot()?),
            "casefile_query" => serialize(self.provider.query(parse(arguments)?)?),
            "casefile_preview_record" => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Arguments {
                    request: Option<ChangeRequest>,
                    requests: Option<Vec<ChangeRequest>>,
                }
                let arguments = parse::<Arguments>(arguments)?;
                match (arguments.request, arguments.requests) {
                    (Some(request), None) => self.publish_preview(StoredPreview::Record(
                        self.provider.preview_record(request)?,
                    )),
                    (None, Some(requests)) => self.publish_preview(StoredPreview::RecordBatch(
                        self.provider.preview_record_batch(requests)?,
                    )),
                    _ => bail!("pass exactly one of request or requests"),
                }
            }
            "casefile_apply_record" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: Value,
                }
                match self.approved_preview(parse::<Arguments>(arguments)?.preview)? {
                    StoredPreview::Record(preview) => {
                        serialize(self.provider.apply_record(preview)?)
                    }
                    StoredPreview::RecordBatch(preview) => {
                        serialize(self.provider.apply_record_batch(preview)?)
                    }
                    _ => bail!("preview was produced by a different Casefile tool"),
                }
            }
            "casefile_preview_progress" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    operation: ProgressOperation,
                }
                self.publish_preview(StoredPreview::Progress(
                    self.provider
                        .preview_progress(parse::<Arguments>(arguments)?.operation)?,
                ))
            }
            "casefile_apply_progress" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: Value,
                }
                let preview = match self.approved_preview(parse::<Arguments>(arguments)?.preview)? {
                    StoredPreview::Progress(preview) => preview,
                    _ => bail!("preview was produced by a different Casefile tool"),
                };
                serialize(self.provider.apply_progress(preview)?)
            }
            "casefile_preview_default_delivery_board" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    investigation: String,
                }
                self.publish_preview(StoredPreview::Board(
                    self.provider.preview_default_delivery_board(
                        parse::<Arguments>(arguments)?.investigation,
                    )?,
                ))
            }
            "casefile_apply_default_delivery_board" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: Value,
                }
                let preview = match self.approved_preview(parse::<Arguments>(arguments)?.preview)? {
                    StoredPreview::Board(preview) => preview,
                    _ => bail!("preview was produced by a different Casefile tool"),
                };
                serialize(self.provider.apply_default_delivery_board(preview)?)
            }
            "casefile_preview_strategy_transition" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    request: StrategyTransitionRequest,
                }
                self.publish_preview(StoredPreview::StrategyTransition(
                    self.provider
                        .preview_strategy_transition(parse::<Arguments>(arguments)?.request)?,
                ))
            }
            "casefile_apply_strategy_transition" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: Value,
                }
                let preview = match self.approved_preview(parse::<Arguments>(arguments)?.preview)? {
                    StoredPreview::StrategyTransition(preview) => preview,
                    _ => bail!("preview was produced by a different Casefile tool"),
                };
                serialize(self.provider.apply_strategy_transition(preview)?)
            }
            "casefile_preview_writer_binding" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    request: WriterBindingRequest,
                }
                self.publish_preview(StoredPreview::WriterBinding(
                    self.provider
                        .preview_writer_binding(parse::<Arguments>(arguments)?.request)?,
                ))
            }
            "casefile_apply_writer_binding" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: Value,
                }
                let preview = match self.approved_preview(parse::<Arguments>(arguments)?.preview)? {
                    StoredPreview::WriterBinding(preview) => preview,
                    _ => bail!("preview was produced by a different Casefile tool"),
                };
                serialize(self.provider.apply_writer_binding(preview)?)
            }
            _ => bail!("unknown Casefile tool {name}"),
        }
    }

    fn publish_preview(&self, internal: StoredPreview) -> Result<Value> {
        let mut public = match &internal {
            StoredPreview::Record(preview) => serialize(preview)?,
            StoredPreview::RecordBatch(preview) => serialize(preview)?,
            StoredPreview::Progress(preview) => serialize(preview)?,
            StoredPreview::Board(preview) => serialize(preview)?,
            StoredPreview::StrategyTransition(preview) => serialize(preview)?,
            StoredPreview::WriterBinding(preview) => serialize(preview)?,
        };
        remove_internal_bytes(&mut public);
        let preview_id = public
            .get("preview_id")
            .and_then(Value::as_str)
            .context("provider preview is missing preview_id")?
            .to_owned();
        let mut vault = self.previews.lock().expect("MCP preview vault");
        vault.order.push_back(preview_id.clone());
        vault.values.insert(
            preview_id,
            PreviewEntry {
                public: public.clone(),
                internal,
            },
        );
        while vault.order.len() > PREVIEW_LIMIT {
            if let Some(expired) = vault.order.pop_front() {
                vault.values.remove(&expired);
            }
        }
        Ok(public)
    }

    fn approved_preview(&self, public: Value) -> Result<StoredPreview> {
        let preview_id = public
            .get("preview_id")
            .and_then(Value::as_str)
            .context("preview must contain preview_id")?;
        let vault = self.previews.lock().expect("MCP preview vault");
        let entry = vault
            .values
            .get(preview_id)
            .filter(|entry| entry.public == public)
            .context("provider preview is unknown, expired, or was altered")?;
        Ok(entry.internal.clone())
    }
}

fn remove_internal_bytes(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                remove_internal_bytes(item);
            }
        }
        Value::Object(object) => {
            object.remove("rendered_bytes");
            object.remove("proposed_bytes");
            for item in object.values_mut() {
                remove_internal_bytes(item);
            }
        }
        _ => {}
    }
}

fn is_tool_call(request: &Value) -> bool {
    request.as_object().is_some_and(|object| {
        object.get("jsonrpc") == Some(&Value::String("2.0".into()))
            && object.get("method") == Some(&Value::String("tools/call".into()))
            && object.contains_key("id")
    })
}

fn is_apply_tool(name: &str) -> bool {
    matches!(
        name,
        "casefile_apply_record"
            | "casefile_apply_progress"
            | "casefile_apply_default_delivery_board"
            | "casefile_apply_strategy_transition"
            | "casefile_apply_writer_binding"
    )
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "casefile_snapshot",
            "Read the canonical provider snapshot, capabilities, diagnostics, and projections.",
            object_schema(json!({}), &[]),
        ),
        tool(
            "casefile_query",
            "Run a canonical provider query. Pass the query object directly, not under a request or arguments key.",
            query_schema(),
        ),
        tool(
            "casefile_preview_record",
            "Preview canonical ticket, epic, or board changes without writing. Put one change under request, or an atomic set that must validate together under requests.",
            json!({
                "oneOf": [
                    object_schema(json!({"request": change_request_schema()}), &["request"]),
                    object_schema(
                        json!({
                            "requests": {
                                "type": "array",
                                "minItems": 1,
                                "items": change_request_schema(),
                            }
                        }),
                        &["requests"],
                    ),
                ]
            }),
        ),
        tool(
            "casefile_apply_record",
            "Apply one exact provider-produced record or record-batch preview after external approval. Pass the matching preview tool's entire structuredContent unchanged as preview.",
            apply_schema(),
        ),
        tool(
            "casefile_preview_progress",
            "Preview a progress bootstrap or append without writing. Put the typed progress operation under operation.",
            object_schema(
                json!({"operation": progress_operation_schema()}),
                &["operation"],
            ),
        ),
        tool(
            "casefile_apply_progress",
            "Apply one exact provider-produced progress preview after external approval. Pass the matching preview tool's entire structuredContent unchanged as preview.",
            apply_schema(),
        ),
        tool(
            "casefile_preview_default_delivery_board",
            "Preview the canonical default delivery board. investigation is the planning-root-relative investigation directory.",
            object_schema(
                json!({"investigation": non_empty_string()}),
                &["investigation"],
            ),
        ),
        tool(
            "casefile_apply_default_delivery_board",
            "Apply one exact provider-produced default-board preview after external approval. Pass the matching preview tool's entire structuredContent unchanged as preview.",
            apply_schema(),
        ),
        tool(
            "casefile_preview_strategy_transition",
            "Preview a governed strategy transition without writing. Put the complete typed transition request under request.",
            object_schema(
                json!({"request": strategy_transition_request_schema()}),
                &["request"],
            ),
        ),
        tool(
            "casefile_apply_strategy_transition",
            "Apply one exact governed strategy-transition preview after external approval. Pass the matching preview tool's entire structuredContent unchanged as preview.",
            apply_schema(),
        ),
        tool(
            "casefile_preview_writer_binding",
            "Preview a progress-gated writer binding without writing. Put investigation and binding_source under request.",
            object_schema(
                json!({
                    "request": object_schema(
                        json!({
                            "investigation": non_empty_string(),
                            "binding_source": non_empty_string(),
                        }),
                        &["investigation", "binding_source"],
                    )
                }),
                &["request"],
            ),
        ),
        tool(
            "casefile_apply_writer_binding",
            "Apply one exact writer-binding preview after external approval. Pass the matching preview tool's entire structuredContent unchanged as preview.",
            apply_schema(),
        ),
    ]
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn non_empty_string() -> Value {
    json!({"type": "string", "minLength": 1})
}

fn string_array() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}

fn nullable(schema: Value) -> Value {
    json!({"anyOf": [schema, {"type": "null"}]})
}

fn query_schema() -> Value {
    let scope = || {
        nullable(object_schema(
            json!({
                "project": non_empty_string(),
                "investigation": non_empty_string(),
            }),
            &["project"],
        ))
    };
    let scoped = |query: &str| {
        object_schema(
            json!({
                "query": {"const": query},
                "scope": scope(),
            }),
            &["query"],
        )
    };
    let searchable = |query: &str| {
        object_schema(
            json!({
                "query": {"const": query},
                "scope": scope(),
                "search": nullable(json!({"type": "string"})),
            }),
            &["query"],
        )
    };
    json!({
        "oneOf": [
            searchable("tickets"),
            searchable("epics"),
            scoped("boards"),
            scoped("progress"),
            scoped("strategy_transitions"),
        ]
    })
}

fn change_request_schema() -> Value {
    let with_draft = |operation: &str| {
        object_schema(
            json!({
                "operation": {"const": operation},
                "path": non_empty_string(),
                "draft": record_draft_schema(),
            }),
            &["operation", "path", "draft"],
        )
    };
    json!({
        "oneOf": [
            with_draft("create"),
            with_draft("replace"),
            object_schema(
                json!({
                    "operation": {"const": "delete"},
                    "path": non_empty_string(),
                }),
                &["operation", "path"],
            ),
        ]
    })
}

fn record_draft_schema() -> Value {
    let work_item = |kind: &str| {
        object_schema(
            json!({
                "kind": {"const": kind},
                "id": non_empty_string(),
                "title": non_empty_string(),
                "project": non_empty_string(),
                "investigation": non_empty_string(),
                "status": {"type": "string", "enum": ["provisional", "accepted", "rejected"]},
                "reported_by_role": non_empty_string(),
                "reported_by_agent": non_empty_string(),
                "source_commit": non_empty_string(),
                "created_at": {"type": "string", "format": "date-time"},
                "updated_at": {"type": "string", "format": "date-time"},
                "confidence": {"type": "string", "enum": ["low", "medium", "high"]},
                "decision_refs": string_array(),
                "related_tickets": string_array(),
                "supersedes": string_array(),
                "superseded_by": string_array(),
                "rank": nullable(json!({"type": "integer", "minimum": 0})),
                "requirement_and_evidence": {"type": "string"},
                "impact": {"type": "string"},
                "resolution_boundary": {"type": "string"},
                "acceptance_criteria": {"type": "string"},
                "verification": {"type": "string"},
                "relationships_and_duplicate_analysis": {"type": "string"},
                "review_and_disposition_history": {"type": "string"},
            }),
            &[
                "kind",
                "id",
                "title",
                "project",
                "investigation",
                "status",
                "reported_by_role",
                "reported_by_agent",
                "source_commit",
                "created_at",
                "updated_at",
                "confidence",
                "decision_refs",
                "related_tickets",
                "supersedes",
                "superseded_by",
                "requirement_and_evidence",
                "impact",
                "resolution_boundary",
                "acceptance_criteria",
                "verification",
                "relationships_and_duplicate_analysis",
                "review_and_disposition_history",
            ],
        )
    };
    json!({
        "oneOf": [
            work_item("ticket"),
            work_item("epic"),
            object_schema(
                json!({
                    "kind": {"const": "board"},
                    "id": non_empty_string(),
                    "title": non_empty_string(),
                    "status_source": {"type": "string", "enum": ["disposition", "progress"]},
                    "filter_statuses": nullable(string_array()),
                    "filter_kinds": nullable(string_array()),
                    "columns": {
                        "type": "array",
                        "minItems": 1,
                        "items": object_schema(
                            json!({
                                "name": non_empty_string(),
                                "statuses": {
                                    "type": "array",
                                    "minItems": 1,
                                    "items": {"type": "string"},
                                },
                            }),
                            &["name", "statuses"],
                        ),
                    },
                }),
                &["kind", "id", "title", "columns"],
            ),
        ]
    })
}

fn progress_operation_schema() -> Value {
    let base_entry = |kind: &str, variant: Value, variant_required: &[&str]| {
        let mut required = vec!["kind", "id", "recorded_at", "recorded_by", "ticket_id"];
        required.extend_from_slice(variant_required);
        let mut properties = json!({
            "kind": {"const": kind},
            "id": non_empty_string(),
            "recorded_at": {"type": "string", "format": "date-time"},
            "recorded_by": non_empty_string(),
            "ticket_id": non_empty_string(),
        });
        properties
            .as_object_mut()
            .expect("fixed object")
            .extend(variant.as_object().expect("fixed object").clone());
        object_schema(properties, &required)
    };
    let entry = json!({
        "oneOf": [
            base_entry(
                "transition",
                json!({
                    "from": {
                        "type": "string",
                        "enum": ["unknown", "in_progress", "in_review", "verifying", "blocked", "complete"],
                    },
                    "to": {
                        "type": "string",
                        "enum": ["unknown", "in_progress", "in_review", "verifying", "blocked", "complete"],
                    },
                }),
                &["from", "to"],
            ),
            base_entry(
                "note",
                json!({
                    "category": {"type": "string", "enum": ["deviation", "quirk"]},
                    "message": non_empty_string(),
                }),
                &["category", "message"],
            ),
        ]
    });
    json!({
        "oneOf": [
            object_schema(
                json!({
                    "operation": {"const": "bootstrap"},
                    "investigation": non_empty_string(),
                }),
                &["operation", "investigation"],
            ),
            object_schema(
                json!({
                    "operation": {"const": "append"},
                    "investigation": non_empty_string(),
                    "entries": {"type": "array", "minItems": 1, "items": entry},
                }),
                &["operation", "investigation", "entries"],
            ),
        ]
    })
}

fn strategy_transition_request_schema() -> Value {
    object_schema(
        json!({
            "investigation": non_empty_string(),
            "operation_id": non_empty_string(),
            "recorded_at": {"type": "string", "format": "date-time"},
            "selected_matrix_origin": non_empty_string(),
            "selected_matrix_source": non_empty_string(),
            "available_capabilities": string_array(),
            "preserved_work_paths": string_array(),
            "active_ownership": {
                "type": "array",
                "items": object_schema(
                    json!({
                        "owner": non_empty_string(),
                        "paths": {
                            "type": "array",
                            "minItems": 1,
                            "items": {"type": "string", "minLength": 1},
                        },
                    }),
                    &["owner", "paths"],
                ),
            },
            "rationale": non_empty_string(),
        }),
        &[
            "investigation",
            "operation_id",
            "recorded_at",
            "selected_matrix_origin",
            "selected_matrix_source",
            "available_capabilities",
            "preserved_work_paths",
            "rationale",
        ],
    )
}

fn apply_schema() -> Value {
    object_schema(
        json!({
            "preview": {
                "type": "object",
                "description": "The complete structuredContent object returned by the matching preview tool. Pass it unchanged; do not construct it and do not pass preview_id alone.",
                "additionalProperties": true,
            }
        }),
        &["preview"],
    )
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": input_schema})
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).context("decode tool arguments")
}

fn serialize(value: impl Serialize) -> Result<Value> {
    serde_json::to_value(value).context("encode provider result")
}

fn tool_result(value: Value, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "provider result encoding failed".into())}],
        "structuredContent": value,
        "isError": is_error,
    })
}

fn id(object: &serde_json::Map<String, Value>) -> Value {
    object.get("id").cloned().unwrap_or(Value::Null)
}

fn success_response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn write_message(output: &mut impl Write, value: Value) -> Result<()> {
    serde_json::to_writer(&mut *output, &value).context("write MCP stdio response")?;
    output
        .write_all(b"\n")
        .context("terminate MCP stdio response")?;
    output.flush().context("flush MCP stdio response")
}
