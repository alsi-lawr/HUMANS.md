use anyhow::{Context, Result, bail};
use casefile_core::ChangeRequest;
use casefile_store::{
    ActivationState, DefaultBoardPreview, PROVIDER_PROTOCOL_VERSION, ProgressOperation, Provider,
    ProviderApprovalPolicy, ProviderBatchPreview, ProviderCapabilities, ProviderMutationState,
    ProviderOperation, ProviderPreview, ProviderProgressPreview, ProviderStrategyTransitionPreview,
    ProviderWriterBindingPreview, Store, StrategyTransitionRequest, WriterBindingRequest,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
    "record_index",
    "record_detail",
    "boards",
    "strategy_transitions",
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
    validate_baseline(&baseline.capabilities, baseline.activation, &required)?;
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
    required: &BTreeSet<String>,
) -> Result<()> {
    if activation != ActivationState::Active {
        bail!("explicit planning root is unactivated, invalid, or unsupported");
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
    if capabilities.approval_policy != ProviderApprovalPolicy::RecordDeletesOnly {
        bail!("provider approval policy is incompatible");
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
        ProviderOperation::RecordIndex => "record_index",
        ProviderOperation::RecordDetail => "record_detail",
        ProviderOperation::Boards => "boards",
        ProviderOperation::StrategyTransitions => "strategy_transitions",
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

#[derive(Default)]
struct PreviewVault {
    order: VecDeque<String>,
    values: BTreeMap<String, StoredPreview>,
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
                "instructions": "Casefile tools operate only on the explicit planning root. Read in order: snapshot catalogue, select one exact project and investigation, request its record_index, then request only necessary exact record_detail identities. Never request unscoped or bulk record bodies and never mix revisions. Every preview states approval_required. Request external approval only when true; apply an exact live-session preview with preview_id."
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
            Err(error) => success_response(request_id, tool_error(&format!("{error:#}"))),
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
            "casefile_apply_record" => match self.preview_by_id(arguments)? {
                StoredPreview::Record(preview) => serialize(self.provider.apply_record(preview)?),
                StoredPreview::RecordBatch(preview) => {
                    serialize(self.provider.apply_record_batch(preview)?)
                }
                _ => bail!("preview was produced by a different Casefile tool"),
            },
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
                let preview = match self.preview_by_id(arguments)? {
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
                let preview = match self.preview_by_id(arguments)? {
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
                let preview = match self.preview_by_id(arguments)? {
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
                let preview = match self.preview_by_id(arguments)? {
                    StoredPreview::WriterBinding(preview) => preview,
                    _ => bail!("preview was produced by a different Casefile tool"),
                };
                serialize(self.provider.apply_writer_binding(preview)?)
            }
            _ => bail!("unknown Casefile tool {name}"),
        }
    }

    fn publish_preview(&self, internal: StoredPreview) -> Result<Value> {
        let public = review_envelope(&internal)?;
        let preview_id = public
            .get("preview_id")
            .and_then(Value::as_str)
            .context("provider preview is missing preview_id")?
            .to_owned();
        let mut vault = self.previews.lock().expect("MCP preview vault");
        vault.order.push_back(preview_id.clone());
        vault.values.insert(preview_id, internal);
        while vault.order.len() > PREVIEW_LIMIT {
            if let Some(expired) = vault.order.pop_front() {
                vault.values.remove(&expired);
            }
        }
        Ok(public)
    }

    fn preview_by_id(&self, arguments: Value) -> Result<StoredPreview> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Arguments {
            preview_id: String,
        }
        let preview_id = parse::<Arguments>(arguments)?.preview_id;
        let vault = self.previews.lock().expect("MCP preview vault");
        vault
            .values
            .get(&preview_id)
            .cloned()
            .context("provider preview is unknown or expired")
    }
}

#[derive(Serialize)]
struct ReviewOperation {
    operation: &'static str,
    path: String,
}

fn review_envelope(preview: &StoredPreview) -> Result<Value> {
    let (preview_id, approval_required, no_op, operations, diagnostics, diffs) = match preview {
        StoredPreview::Record(preview) => (
            preview.preview_id.as_str(),
            preview.approval_required,
            preview.no_op,
            vec![record_review_operation(&preview.canonical.request)],
            serialize(&preview.canonical.diagnostics)?,
            vec![preview.canonical.diff.as_str()],
        ),
        StoredPreview::RecordBatch(preview) => (
            preview.preview_id.as_str(),
            preview.approval_required,
            preview.no_op,
            preview
                .canonical
                .requests
                .iter()
                .map(record_review_operation)
                .collect(),
            serialize(&preview.canonical.diagnostics)?,
            vec![preview.canonical.diff.as_str()],
        ),
        StoredPreview::Progress(preview) => (
            preview.preview_id.as_str(),
            preview.approval_required,
            preview.canonical.no_op,
            vec![ReviewOperation {
                operation: match preview.operation {
                    ProgressOperation::Bootstrap { .. } => "bootstrap",
                    ProgressOperation::Append { .. } => "append",
                },
                path: preview.canonical.path.clone(),
            }],
            serialize(&preview.canonical.diagnostics)?,
            vec![preview.canonical.diff.as_str()],
        ),
        StoredPreview::Board(preview) => (
            preview.preview_id.as_str(),
            preview.approval_required,
            preview.no_op,
            vec![record_review_operation(&preview.canonical.request)],
            serialize(&preview.canonical.diagnostics)?,
            vec![preview.canonical.diff.as_str()],
        ),
        StoredPreview::StrategyTransition(preview) => (
            preview.preview_id.as_str(),
            preview.approval_required,
            preview.canonical.no_op,
            preview
                .canonical
                .changes
                .iter()
                .map(|change| ReviewOperation {
                    operation: governed_review_operation(
                        change.expected_target_revision.is_some(),
                        change.proposed_target_revision.is_some(),
                    ),
                    path: change.path.clone(),
                })
                .collect(),
            serialize(&preview.canonical.diagnostics)?,
            preview
                .canonical
                .changes
                .iter()
                .map(|change| change.diff.as_str())
                .collect(),
        ),
        StoredPreview::WriterBinding(preview) => (
            preview.preview_id.as_str(),
            preview.approval_required,
            preview.canonical.no_op,
            preview
                .canonical
                .changes
                .iter()
                .map(|change| ReviewOperation {
                    operation: governed_review_operation(
                        change.expected_target_revision.is_some(),
                        change.proposed_target_revision.is_some(),
                    ),
                    path: change.path.clone(),
                })
                .collect(),
            serialize(&preview.canonical.diagnostics)?,
            preview
                .canonical
                .changes
                .iter()
                .map(|change| change.diff.as_str())
                .collect(),
        ),
    };
    let mut operation_counts = BTreeMap::new();
    for operation in &operations {
        *operation_counts
            .entry(operation.operation)
            .or_insert(0_usize) += 1;
    }
    Ok(json!({
        "preview_id": preview_id,
        "approval_required": approval_required,
        "no_op": no_op,
        "operation_counts": operation_counts,
        "operations": operations,
        "diagnostics": diagnostics,
        "diff": diff_summary(&diffs),
    }))
}

fn record_review_operation(request: &ChangeRequest) -> ReviewOperation {
    ReviewOperation {
        operation: match request {
            ChangeRequest::Create { .. } => "create",
            ChangeRequest::Replace { .. } => "replace",
            ChangeRequest::Delete { .. } => "delete",
        },
        path: request.path().to_owned(),
    }
}

fn governed_review_operation(expected: bool, proposed: bool) -> &'static str {
    match (expected, proposed) {
        (false, true) => "create",
        (true, false) => "delete",
        _ => "replace",
    }
}

fn diff_summary(diffs: &[&str]) -> Value {
    let mut hasher = Sha256::new();
    let mut bytes = 0_usize;
    for diff in diffs {
        bytes += diff.len();
        hasher.update(diff.as_bytes());
    }
    json!({
        "bytes": bytes,
        "sha256": format!("sha256:{:x}", hasher.finalize()),
    })
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
            "Read only bounded root capabilities, metadata revision, diagnostic coverage counts, and project/investigation catalogue. Then select one exact investigation before querying records.",
            object_schema(json!({}), &[]),
        ),
        tool(
            "casefile_query",
            "Read one exact investigation-scoped record index, one exact record detail, scoped boards, or scoped strategy transitions. Never mix returned revisions.",
            query_schema(),
        ),
        tool(
            "casefile_preview_record",
            "Preview canonical ticket, epic, or board changes without writing. Put one change under request, or an atomic set that must validate together under requests. Returns a compact review envelope retained under preview_id; approval_required is true only when the request contains a delete.",
            one_of_object(vec![
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
            ]),
        ),
        tool(
            "casefile_apply_record",
            "Apply one exact live-session record or record-batch preview by preview_id. Obtain external approval first only when its review envelope's approval_required is true.",
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
            "Apply one exact live-session progress preview by preview_id without a separate approval interruption.",
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
            "Apply one exact live-session default-board preview by preview_id without a separate approval interruption.",
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
            "Apply one exact live-session strategy-transition preview by preview_id after the human has selected the strategy, without a second apply confirmation.",
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
            "Apply one exact live-session writer-binding preview by preview_id after the human has selected the binding, without a second apply confirmation.",
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

/// A discriminated union of object variants.
///
/// The explicit `type` is required: MCP clients reject a tool whose `inputSchema`
/// omits it, and a rejected schema drops the whole `tools/list` response.
fn one_of_object(variants: Vec<Value>) -> Value {
    json!({"type": "object", "oneOf": variants})
}

fn query_schema() -> Value {
    let scope = || {
        object_schema(
            json!({
                "project": non_empty_string(),
                "investigation": non_empty_string(),
            }),
            &["project", "investigation"],
        )
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
    one_of_object(vec![
        scoped("record_index"),
        object_schema(
            json!({
                "query": {"const": "record_detail"},
                "identity": object_schema(
                    json!({
                        "scope": scope(),
                        "identity": non_empty_string(),
                    }),
                    &["scope", "identity"],
                ),
            }),
            &["query", "identity"],
        ),
        scoped("boards"),
        scoped("strategy_transitions"),
    ])
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
    one_of_object(vec![
        with_draft("create"),
        with_draft("replace"),
        object_schema(
            json!({
                "operation": {"const": "delete"},
                "path": non_empty_string(),
            }),
            &["operation", "path"],
        ),
    ])
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
    one_of_object(vec![
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
    ])
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
    let entry = one_of_object(vec![
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
    ]);
    one_of_object(vec![
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
    ])
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
            "preview_id": {
                "type": "string",
                "minLength": 1,
                "description": "The preview_id returned by the matching preview tool in this live MCP session.",
            },
        }),
        &["preview_id"],
    )
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": tool_output_schema(name),
    })
}

fn tool_output_schema(name: &str) -> Value {
    if name == "casefile_snapshot" {
        return object_schema(
            json!({
                "capabilities": capabilities_schema(),
                "activation": {"type": "string", "enum": ["unactivated", "active", "invalid"]},
                "revision": non_empty_string(),
                "diagnostic_coverage": object_schema(
                    json!({
                        "catalogue": object_schema(
                            json!({"count": {"type": "integer", "minimum": 0}}),
                            &["count"],
                        ),
                        "records": {"const": "not_loaded"},
                    }),
                    &["catalogue", "records"],
                ),
                "catalogue": object_schema(
                    json!({
                        "projects": {"type": "array", "items": project_schema()},
                    }),
                    &["projects"],
                ),
                "cache": cache_schema(),
            }),
            &[
                "capabilities",
                "activation",
                "revision",
                "diagnostic_coverage",
                "catalogue",
                "cache",
            ],
        );
    }
    if name == "casefile_query" {
        return query_output_schema();
    }
    if name.starts_with("casefile_preview_") {
        return preview_output_schema();
    }
    apply_output_schema(name)
}

fn capabilities_schema() -> Value {
    let operations = [
        "snapshot",
        "record_index",
        "record_detail",
        "boards",
        "strategy_transitions",
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
    object_schema(
        json!({
            "protocol_version": {"const": 2},
            "planning_format_versions": {"type": "array", "items": {"type": "integer"}},
            "mutation": one_of_object(vec![
                object_schema(json!({"state": {"const": "read_write"}}), &["state"]),
                object_schema(
                    json!({"state": {"const": "read_only"}, "reason": non_empty_string()}),
                    &["state", "reason"],
                ),
            ]),
            "operations": {"type": "array", "items": {"type": "string", "enum": operations}},
            "approval_policy": {"const": "record_deletes_only"},
            "writes_require_external_approval": {"type": "boolean"},
        }),
        &[
            "protocol_version",
            "planning_format_versions",
            "mutation",
            "operations",
            "approval_policy",
            "writes_require_external_approval",
        ],
    )
}

fn project_schema() -> Value {
    object_schema(
        json!({
            "name": {"type": "string"},
            "source_root": {"type": "string"},
            "governed": {"type": "boolean"},
            "prefix": non_empty_string(),
            "investigations": {
                "type": "array",
                "items": object_schema(
                    json!({"identity": non_empty_string(), "path": non_empty_string()}),
                    &["identity", "path"],
                ),
            },
        }),
        &["name", "governed", "investigations"],
    )
}

fn scope_schema() -> Value {
    object_schema(
        json!({
            "project": non_empty_string(),
            "investigation": non_empty_string(),
        }),
        &["project", "investigation"],
    )
}

fn scoped_identity_schema() -> Value {
    object_schema(
        json!({"scope": scope_schema(), "identity": non_empty_string()}),
        &["scope", "identity"],
    )
}

fn query_output_schema() -> Value {
    one_of_object(vec![
        object_schema(
            json!({
                "result": {"const": "record_index"},
                "revision": non_empty_string(),
                "scope": scope_schema(),
                "diagnostic_coverage": object_schema(
                    json!({
                        "scope": scope_schema(),
                        "kind": {"const": "local_and_investigation"},
                    }),
                    &["scope", "kind"],
                ),
                "records": {"type": "array", "items": record_index_entry_schema()},
            }),
            &[
                "result",
                "revision",
                "scope",
                "diagnostic_coverage",
                "records",
            ],
        ),
        object_schema(
            json!({
                "result": {"const": "record_detail"},
                "revision": non_empty_string(),
                "identity": scoped_identity_schema(),
                "record": nullable(record_detail_schema()),
            }),
            &["result", "revision", "identity", "record"],
        ),
        object_schema(
            json!({
                "result": {"const": "boards"},
                "revision": non_empty_string(),
                "scope": scope_schema(),
                "boards": {"type": "array", "items": board_output_schema()},
            }),
            &["result", "revision", "scope", "boards"],
        ),
        object_schema(
            json!({
                "result": {"const": "strategy_transitions"},
                "revision": non_empty_string(),
                "scope": scope_schema(),
                "transitions": {"type": "array", "items": transition_output_schema()},
            }),
            &["result", "revision", "scope", "transitions"],
        ),
    ])
}

fn record_index_entry_schema() -> Value {
    object_schema(
        json!({
            "path": non_empty_string(),
            "classification": classification_schema(),
            "kind": nullable(json!({"type": "string", "enum": ["ticket", "epic"]})),
            "identity": non_empty_string(),
            "title": {"type": "string"},
            "status": {"type": "string"},
            "rank": {"type": "integer", "minimum": 0},
            "progress": object_schema(
                json!({
                    "status": progress_status_schema(),
                    "note_count": {"type": "integer", "minimum": 0},
                }),
                &["status", "note_count"],
            ),
            "diagnostic_count": {"type": "integer", "minimum": 0},
        }),
        &["path", "classification", "kind", "diagnostic_count"],
    )
}

fn record_detail_schema() -> Value {
    object_schema(
        json!({
            "path": non_empty_string(),
            "classification": classification_schema(),
            "kind": {"type": "string", "enum": ["ticket", "epic"]},
            "identity": scoped_identity_schema(),
            "draft": record_draft_schema(),
            "progress": progress_detail_schema(),
            "diagnostics": {"type": "array", "items": diagnostic_schema()},
        }),
        &[
            "path",
            "classification",
            "kind",
            "identity",
            "draft",
            "diagnostics",
        ],
    )
}

fn progress_detail_schema() -> Value {
    object_schema(
        json!({
            "status": progress_status_schema(),
            "last_transition": object_schema(
                json!({
                    "id": non_empty_string(),
                    "recorded_at": non_empty_string(),
                    "recorded_by": non_empty_string(),
                    "from": progress_status_schema(),
                    "to": progress_status_schema(),
                }),
                &["id", "recorded_at", "recorded_by", "from", "to"],
            ),
            "notes": {
                "type": "array",
                "items": object_schema(
                    json!({
                        "id": non_empty_string(),
                        "recorded_at": non_empty_string(),
                        "recorded_by": non_empty_string(),
                        "category": {"type": "string", "enum": ["deviation", "quirk"]},
                        "message": {"type": "string"},
                    }),
                    &["id", "recorded_at", "recorded_by", "category", "message"],
                ),
            },
        }),
        &["status", "notes"],
    )
}

fn classification_schema() -> Value {
    json!({"type": "string", "enum": ["governed", "ungoverned", "invalid", "raw"]})
}

fn progress_status_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["unknown", "in_progress", "in_review", "verifying", "blocked", "complete"],
    })
}

fn board_output_schema() -> Value {
    let record_scope = object_schema(
        json!({"project": non_empty_string(), "investigation": non_empty_string()}),
        &["project", "investigation"],
    );
    let identity = object_schema(
        json!({"scope": record_scope, "identity": non_empty_string()}),
        &["scope", "identity"],
    );
    let card = object_schema(
        json!({
            "identity": identity.clone(),
            "kind": {"type": "string", "enum": ["ticket", "epic"]},
            "title": {"type": "string"},
            "status": {"type": "string"},
            "rank": nullable(json!({"type": "integer", "minimum": 0})),
        }),
        &["identity", "kind", "title", "status", "rank"],
    );
    object_schema(
        json!({
            "identity": identity,
            "title": {"type": "string"},
            "status_source": {"type": "string", "enum": ["disposition", "progress"]},
            "filter_statuses": nullable(string_array()),
            "filter_kinds": nullable(string_array()),
            "columns": {
                "type": "array",
                "items": object_schema(
                    json!({
                        "name": {"type": "string"},
                        "statuses": string_array(),
                        "cards": {"type": "array", "items": card},
                    }),
                    &["name", "statuses", "cards"],
                ),
            },
        }),
        &[
            "identity",
            "title",
            "status_source",
            "filter_statuses",
            "filter_kinds",
            "columns",
        ],
    )
}

fn transition_output_schema() -> Value {
    let record = object_schema(
        json!({
            "operation_id": non_empty_string(),
            "recorded_at": non_empty_string(),
            "phase": non_empty_string(),
            "previous_strategy_id": non_empty_string(),
            "selected_strategy_id": non_empty_string(),
            "selected_matrix_origin": non_empty_string(),
            "selected_matrix_sha256": non_empty_string(),
            "expected_store_revision": non_empty_string(),
            "expected_matrix_revision": non_empty_string(),
            "proposed_matrix_revision": non_empty_string(),
            "root_binding": non_empty_string(),
            "governed_state_updated": {"type": "boolean"},
            "rationale": {"type": "string"},
            "available_capabilities": string_array(),
            "preserved_work_paths": string_array(),
            "active_ownership": {
                "type": "array",
                "items": object_schema(
                    json!({
                        "owner": non_empty_string(),
                        "paths": string_array(),
                    }),
                    &["owner", "paths"],
                ),
            },
        }),
        &[
            "operation_id",
            "recorded_at",
            "phase",
            "previous_strategy_id",
            "selected_strategy_id",
            "selected_matrix_origin",
            "selected_matrix_sha256",
            "expected_store_revision",
            "expected_matrix_revision",
            "proposed_matrix_revision",
            "root_binding",
            "governed_state_updated",
            "rationale",
            "available_capabilities",
            "preserved_work_paths",
            "active_ownership",
        ],
    );
    object_schema(
        json!({
            "path": non_empty_string(),
            "scope": scope_schema(),
            "record": record,
        }),
        &["path", "scope", "record"],
    )
}

fn preview_output_schema() -> Value {
    object_schema(
        json!({
            "preview_id": non_empty_string(),
            "approval_required": {"type": "boolean"},
            "no_op": {"type": "boolean"},
            "operation_counts": {
                "type": "object",
                "properties": {
                    "create": {"type": "integer", "minimum": 0},
                    "replace": {"type": "integer", "minimum": 0},
                    "delete": {"type": "integer", "minimum": 0},
                    "bootstrap": {"type": "integer", "minimum": 0},
                    "append": {"type": "integer", "minimum": 0},
                },
                "additionalProperties": false,
            },
            "operations": {
                "type": "array",
                "items": object_schema(
                    json!({
                        "operation": {"type": "string", "enum": ["create", "replace", "delete", "bootstrap", "append"]},
                        "path": non_empty_string(),
                    }),
                    &["operation", "path"],
                ),
            },
            "diagnostics": {"type": "array", "items": diagnostic_schema()},
            "diff": object_schema(
                json!({
                    "bytes": {"type": "integer", "minimum": 0},
                    "sha256": {"type": "string", "pattern": "^sha256:[0-9a-f]{64}$"},
                }),
                &["bytes", "sha256"],
            ),
        }),
        &[
            "preview_id",
            "approval_required",
            "no_op",
            "operation_counts",
            "operations",
            "diagnostics",
            "diff",
        ],
    )
}

fn apply_output_schema(name: &str) -> Value {
    let single = object_schema(
        json!({
            "path": non_empty_string(),
            "resulting_target_revision": nullable(non_empty_string()),
            "resulting_store_revision": non_empty_string(),
            "diff": {"type": "string"},
            "no_op": {"type": "boolean"},
        }),
        &[
            "path",
            "resulting_target_revision",
            "resulting_store_revision",
            "diff",
            "no_op",
        ],
    );
    let batch = object_schema(
        json!({
            "paths": {"type": "array", "items": {"type": "string"}},
            "resulting_target_revisions": revision_map_schema(),
            "resulting_store_revision": non_empty_string(),
            "diff": {"type": "string"},
            "no_op": {"type": "boolean"},
        }),
        &[
            "paths",
            "resulting_target_revisions",
            "resulting_store_revision",
            "diff",
            "no_op",
        ],
    );
    let governed = object_schema(
        json!({
            "operation": {"type": "string", "enum": ["strategy_transition", "writer_binding"]},
            "paths": {"type": "array", "items": {"type": "string"}},
            "resulting_store_revision": non_empty_string(),
            "resulting_target_revisions": revision_map_schema(),
            "diffs": {"type": "object", "additionalProperties": {"type": "string"}},
            "no_op": {"type": "boolean"},
        }),
        &[
            "operation",
            "paths",
            "resulting_store_revision",
            "resulting_target_revisions",
            "diffs",
            "no_op",
        ],
    );
    let result = match name {
        "casefile_apply_record" => one_of_object(vec![single, batch]),
        "casefile_apply_progress" | "casefile_apply_default_delivery_board" => single,
        "casefile_apply_strategy_transition" | "casefile_apply_writer_binding" => governed,
        _ => unreachable!("every Casefile tool has an explicit output schema"),
    };
    object_schema(
        json!({
            "result": result,
            "cache": cache_schema(),
        }),
        &["result", "cache"],
    )
}

fn revision_map_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": nullable(non_empty_string()),
    })
}

fn diagnostic_schema() -> Value {
    object_schema(
        json!({
            "schema_version": {"type": "integer"},
            "code": non_empty_string(),
            "path": {"type": "string"},
            "field": {"type": "string"},
            "section": {"type": "string"},
            "message": non_empty_string(),
        }),
        &["schema_version", "code", "path", "message"],
    )
}

fn cache_schema() -> Value {
    one_of_object(vec![
        object_schema(json!({"state": {"const": "not_configured"}}), &["state"]),
        object_schema(json!({"state": {"const": "missing"}}), &["state"]),
        object_schema(
            json!({
                "state": {"const": "stale"},
                "indexed_revision": non_empty_string(),
                "current_revision": non_empty_string(),
            }),
            &["state", "indexed_revision", "current_revision"],
        ),
        object_schema(
            json!({
                "state": {"const": "current"},
                "source_revision": non_empty_string(),
            }),
            &["state", "source_revision"],
        ),
        object_schema(
            json!({
                "state": {"const": "degraded"},
                "message": non_empty_string(),
            }),
            &["state", "message"],
        ),
    ])
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

fn tool_error(message: &str) -> Value {
    json!({
        "content": [{"type": "text", "text": message}],
        "isError": true,
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
