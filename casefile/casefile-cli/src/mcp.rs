use anyhow::{Context, Result, bail};
use casefile_core::ChangeRequest;
use casefile_store::{
    ActivationState, DefaultBoardPreview, PROVIDER_PROTOCOL_VERSION, ProgressOperation, Provider,
    ProviderCapabilities, ProviderMutationState, ProviderOperation, ProviderPreview,
    ProviderProgressPreview, ProviderStrategyTransitionPreview, ProviderWriterBindingPreview,
    Store, StrategyTransitionRequest, WriterBindingRequest,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

const MCP_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
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
    provider: Provider,
    initialized: bool,
}

impl Session {
    fn new(provider: Provider) -> Self {
        Self {
            provider,
            initialized: false,
        }
    }

    fn run(mut self) -> Result<()> {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let stdout = io::stdout();
        let mut output = stdout.lock();
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = input
                .read_line(&mut line)
                .context("read MCP stdio request")?;
            if bytes == 0 {
                return Ok(());
            }
            if bytes > MAX_MESSAGE_BYTES {
                bail!("MCP stdio request exceeds {MAX_MESSAGE_BYTES} bytes");
            }
            let request: Value = match serde_json::from_str(line.trim_end()) {
                Ok(request) => request,
                Err(error) => {
                    write_message(
                        &mut output,
                        error_response(Value::Null, -32700, &format!("parse error: {error}")),
                    )?;
                    continue;
                }
            };
            if let Some(response) = self.handle(request)? {
                write_message(&mut output, response)?;
            }
        }
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
                Ok(Some(self.call_tool(request_id, object.get("params"))))
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
                "instructions": "Casefile tools operate only on the explicit planning root bound to this process. Apply tools require a provider-produced preview and external human approval."
            }),
        )
    }

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
        match self.dispatch(name, arguments) {
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
                struct Arguments {
                    request: ChangeRequest,
                }
                serialize(
                    self.provider
                        .preview_record(parse::<Arguments>(arguments)?.request)?,
                )
            }
            "casefile_apply_record" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: ProviderPreview,
                }
                serialize(
                    self.provider
                        .apply_record(parse::<Arguments>(arguments)?.preview)?,
                )
            }
            "casefile_preview_progress" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    operation: ProgressOperation,
                }
                serialize(
                    self.provider
                        .preview_progress(parse::<Arguments>(arguments)?.operation)?,
                )
            }
            "casefile_apply_progress" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: ProviderProgressPreview,
                }
                serialize(
                    self.provider
                        .apply_progress(parse::<Arguments>(arguments)?.preview)?,
                )
            }
            "casefile_preview_default_delivery_board" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    investigation: String,
                }
                serialize(
                    self.provider.preview_default_delivery_board(
                        parse::<Arguments>(arguments)?.investigation,
                    )?,
                )
            }
            "casefile_apply_default_delivery_board" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: DefaultBoardPreview,
                }
                serialize(
                    self.provider
                        .apply_default_delivery_board(parse::<Arguments>(arguments)?.preview)?,
                )
            }
            "casefile_preview_strategy_transition" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    request: StrategyTransitionRequest,
                }
                serialize(
                    self.provider
                        .preview_strategy_transition(parse::<Arguments>(arguments)?.request)?,
                )
            }
            "casefile_apply_strategy_transition" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: ProviderStrategyTransitionPreview,
                }
                serialize(
                    self.provider
                        .apply_strategy_transition(parse::<Arguments>(arguments)?.preview)?,
                )
            }
            "casefile_preview_writer_binding" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    request: WriterBindingRequest,
                }
                serialize(
                    self.provider
                        .preview_writer_binding(parse::<Arguments>(arguments)?.request)?,
                )
            }
            "casefile_apply_writer_binding" => {
                #[derive(serde::Deserialize)]
                struct Arguments {
                    preview: ProviderWriterBindingPreview,
                }
                serialize(
                    self.provider
                        .apply_writer_binding(parse::<Arguments>(arguments)?.preview)?,
                )
            }
            _ => bail!("unknown Casefile tool {name}"),
        }
    }
}

fn tool_definitions() -> Vec<Value> {
    let open = || json!({"type": "object", "additionalProperties": true});
    vec![
        tool(
            "casefile_snapshot",
            "Read the canonical provider snapshot, capabilities, diagnostics, and projections.",
            json!({"type":"object","additionalProperties":false}),
        ),
        tool(
            "casefile_query",
            "Run a typed canonical provider query.",
            open(),
        ),
        tool(
            "casefile_preview_record",
            "Preview a canonical record draft change without writing.",
            open(),
        ),
        tool(
            "casefile_apply_record",
            "Apply one exact provider-produced record preview after external approval.",
            open(),
        ),
        tool(
            "casefile_preview_progress",
            "Preview a progress bootstrap or append without writing.",
            open(),
        ),
        tool(
            "casefile_apply_progress",
            "Apply one exact provider-produced progress preview after external approval.",
            open(),
        ),
        tool(
            "casefile_preview_default_delivery_board",
            "Preview the canonical default delivery board.",
            open(),
        ),
        tool(
            "casefile_apply_default_delivery_board",
            "Apply one exact provider-produced default-board preview after external approval.",
            open(),
        ),
        tool(
            "casefile_preview_strategy_transition",
            "Preview a governed strategy transition without writing.",
            open(),
        ),
        tool(
            "casefile_apply_strategy_transition",
            "Apply one exact governed strategy-transition preview after external approval.",
            open(),
        ),
        tool(
            "casefile_preview_writer_binding",
            "Preview a progress-gated writer binding without writing.",
            open(),
        ),
        tool(
            "casefile_apply_writer_binding",
            "Apply one exact writer-binding preview after external approval.",
            open(),
        ),
    ]
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
