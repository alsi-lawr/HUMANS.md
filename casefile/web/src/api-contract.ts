import {
  type ApplyResponse,
  type Board,
  type BoardDraft,
  type BoardPayload,
  type BoardStatusSource,
  type Card,
  type ChangeRequest,
  type Classification,
  type Diagnostic,
  type EffectiveWriterBinding,
  type Identity,
  type Kind,
  type Preview,
  type Record,
  type Relationship,
  type Scope,
  type StrategyBindingRecord,
  type StrategyBindingState,
  type StrategyPipeline,
  type StrategyProjection,
  type StrategyWorker,
  type WorkItemDraft,
} from "./model";

type JsonObject = Readonly<{ [key: string]: unknown }>;
export type Decoder<T> = (value: unknown) => T;
export type Indexed<T> = Readonly<{ sourceRevision: string; value: T }>;
export type HostFailure = Readonly<{
  message: string;
  code: "stale_revision" | undefined;
}>;

const contractError = (label: string): never => {
  throw new Error(`The host returned an invalid ${label}.`);
};
const isObject = (value: unknown): value is JsonObject =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const object = (value: unknown, label: string): JsonObject =>
  isObject(value) ? value : contractError(label);
const string = (value: unknown, label: string): string =>
  typeof value === "string" ? value : contractError(label);
const number = (value: unknown, label: string): number =>
  typeof value === "number" && Number.isFinite(value) ? value : contractError(label);
const unsignedInteger = (value: unknown, label: string): number => {
  const decoded = number(value, label);
  return Number.isSafeInteger(decoded) && decoded >= 0 ? decoded : contractError(label);
};
const positiveInteger = (value: unknown, label: string): number => {
  const decoded = unsignedInteger(value, label);
  return decoded > 0 ? decoded : contractError(label);
};
const boolean = (value: unknown, label: string): boolean =>
  typeof value === "boolean" ? value : contractError(label);
const nonEmptyString = (value: unknown, label: string): string => {
  const decoded = string(value, label);
  return decoded.length > 0 ? decoded : contractError(label);
};
const optional = <T>(value: unknown, decode: Decoder<T>): T | undefined =>
  value === undefined ? undefined : decode(value);
const nullable = <T>(value: unknown, decode: Decoder<T>): T | null =>
  value === null ? null : decode(value);
const array = <T>(value: unknown, label: string, decode: Decoder<T>): ReadonlyArray<T> =>
  Array.isArray(value) ? value.map(decode) : contractError(label);
const strings = (value: unknown, label: string): ReadonlyArray<string> =>
  array(value, label, (item) => string(item, `${label} item`));

const decodeClassification = (value: unknown): Classification => {
  switch (value) {
    case "governed":
    case "ungoverned":
    case "invalid":
    case "raw":
      return value;
    default:
      return contractError("record classification");
  }
};
const decodeKind = (value: unknown): Kind => {
  switch (value) {
    case "activation":
    case "project_map":
    case "request":
    case "decision":
    case "evidence":
    case "review":
    case "plan":
    case "closeout":
    case "strategy":
    case "strategy_binding":
    case "progress":
    case "ticket":
    case "epic":
    case "board":
      return value;
    default:
      return contractError("record kind");
  }
};
const decodeEffectiveWriterBinding = (value: unknown): EffectiveWriterBinding => {
  const input = object(value, "effective writer binding");
  const source = input.source;
  if (source !== "matrix" && source !== "binding") return contractError("writer binding source");
  return {
    model: nonEmptyString(input.model, "effective writer model"),
    reasoning_effort: nonEmptyString(input.reasoning_effort, "effective reasoning effort"),
    source,
  };
};
const decodeStrategyBindingState = (value: unknown): StrategyBindingState => {
  const input = object(value, "strategy binding state");
  switch (input.state) {
    case "absent": {
      const effective = decodeEffectiveWriterBinding(input.effective);
      return effective.source === "matrix"
        ? { state: "absent", effective }
        : contractError("absent binding source");
    }
    case "pending":
      return { state: "pending" };
    case "resolved": {
      const effective = decodeEffectiveWriterBinding(input.effective);
      return effective.source === "binding"
        ? { state: "resolved", effective }
        : contractError("resolved binding source");
    }
    case "unresolved":
      return { state: "unresolved" };
    case "invalid":
      return { state: "invalid" };
    default:
      return contractError("strategy binding state");
  }
};
const decodeStrategyWorker = (value: unknown): StrategyWorker => {
  const input = object(value, "strategy worker");
  const model = optional(input.model, (item) => nonEmptyString(item, "worker model"));
  const reasoningEffort = optional(input.reasoning_effort, (item) =>
    nonEmptyString(item, "worker reasoning effort"),
  );
  if ((model === undefined) !== (reasoningEffort === undefined))
    return contractError("worker runtime pair");
  const minimumCount = positiveInteger(input.minimum_count, "worker minimum count");
  const maximumCount = positiveInteger(input.maximum_count, "worker maximum count");
  if (minimumCount > maximumCount) return contractError("worker count range");
  return {
    role: nonEmptyString(input.role, "worker role"),
    platform_profile: nonEmptyString(input.platform_profile, "worker platform profile"),
    runtime:
      model === undefined || reasoningEffort === undefined
        ? { tag: "unspecified" }
        : { tag: "declared", model, reasoning_effort: reasoningEffort },
    minimum_count: minimumCount,
    maximum_count: maximumCount,
    can_spawn_subagents: boolean(input.can_spawn_subagents, "worker spawn permission"),
  };
};
const decodeStrategyPipeline = (value: unknown): StrategyPipeline => {
  const input = object(value, "strategy pipeline");
  return {
    maximum_active_tickets: positiveInteger(input.maximum_active_tickets, "maximum active tickets"),
    look_ahead_read_only: boolean(input.look_ahead_read_only, "look-ahead mode"),
    require_dependency_independence: boolean(
      input.require_dependency_independence,
      "dependency independence gate",
    ),
    require_disjoint_write_paths: boolean(
      input.require_disjoint_write_paths,
      "disjoint write-path gate",
    ),
    immutable_review_commits: boolean(
      input.immutable_review_commits,
      "immutable review-commit gate",
    ),
    corrections_preempt_forward_work: boolean(
      input.corrections_preempt_forward_work,
      "correction preemption gate",
    ),
  };
};
const decodeStrategyProjection = (value: unknown): StrategyProjection => {
  const input = object(value, "strategy projection");
  if (input.root_binding !== "root") return contractError("strategy root binding");
  const limits = object(input.limits, "strategy limits");
  const requirements = object(input.requirements, "strategy requirements");
  const coordination = object(input.coordination, "strategy coordination");
  const maximumConcurrentSubagents = positiveInteger(
    limits.max_concurrent_subagents,
    "maximum concurrent subagents",
  );
  const maximumDepth = unsignedInteger(limits.max_depth, "maximum strategy depth");
  const workers = array(input.workers, "strategy workers", decodeStrategyWorker);
  if (
    workers.reduce((total, worker) => total + worker.minimum_count, 0) > maximumConcurrentSubagents
  )
    return contractError("strategy worker capacity");
  if (maximumDepth < 2 && workers.some((worker) => worker.can_spawn_subagents))
    return contractError("strategy worker depth");
  return {
    root_binding: "root",
    limits: {
      max_concurrent_subagents: maximumConcurrentSubagents,
      max_depth: maximumDepth,
    },
    requirements: {
      capabilities: array(requirements.capabilities, "strategy capabilities", (item) =>
        nonEmptyString(item, "strategy capability"),
      ),
    },
    workers,
    coordination: {
      batch_when_capacity_exceeded: boolean(
        coordination.batch_when_capacity_exceeded,
        "capacity batching rule",
      ),
      candidate_review_before_ticket: boolean(
        coordination.candidate_review_before_ticket,
        "candidate review rule",
      ),
      shared_ticket_storage_required: boolean(
        coordination.shared_ticket_storage_required,
        "shared ticket storage rule",
      ),
      pipeline: optional(coordination.pipeline, decodeStrategyPipeline),
    },
    binding: nullable(input.binding, decodeStrategyBindingState),
  };
};
const decodeStrategyBindingRecord = (value: unknown): StrategyBindingRecord => {
  const input = object(value, "strategy binding record");
  if (input.role !== "implementation-writer") return contractError("strategy binding role");
  const resolution = object(input.resolution, "strategy binding resolution");
  return {
    adapter: nonEmptyString(input.adapter, "strategy binding adapter"),
    role: "implementation-writer",
    model: nonEmptyString(input.model, "strategy binding model"),
    reasoning_effort: nonEmptyString(input.reasoning_effort, "strategy binding reasoning effort"),
    resolution: {
      mode: nonEmptyString(resolution.mode, "strategy binding resolution mode"),
      value: nonEmptyString(resolution.value, "strategy binding resolution value"),
    },
    state: decodeStrategyBindingState(input.state),
  };
};
const decodeScope = (value: unknown): Scope => {
  const input = object(value, "scope");
  return {
    project: string(input.project, "scope project"),
    investigation: optional(input.investigation, (item) => string(item, "scope investigation")),
  };
};
const decodeIdentity = (value: unknown): Identity => {
  const input = object(value, "identity");
  return {
    scope: decodeScope(input.scope),
    identity: string(input.identity, "scoped identity"),
  };
};
const decodeWorkItem = (value: unknown): WorkItemDraft => {
  const input = object(value, "work item");
  return {
    id: string(input.id, "work item ID"),
    title: string(input.title, "work item title"),
    project: string(input.project, "work item project"),
    investigation: string(input.investigation, "work item investigation"),
    status: string(input.status, "work item status"),
    reported_by_role: string(input.reported_by_role, "reported-by role"),
    reported_by_agent: string(input.reported_by_agent, "reported-by agent"),
    source_commit: string(input.source_commit, "source commit"),
    created_at: string(input.created_at, "created timestamp"),
    updated_at: string(input.updated_at, "updated timestamp"),
    confidence: string(input.confidence, "confidence"),
    decision_refs: strings(input.decision_refs, "decision references"),
    related_tickets: strings(input.related_tickets, "related tickets"),
    supersedes: strings(input.supersedes, "supersedes references"),
    superseded_by: strings(input.superseded_by, "superseded-by references"),
    rank: optional(input.rank, (item) => number(item, "work item rank")),
    requirement_and_evidence: string(input.requirement_and_evidence, "requirement section"),
    impact: string(input.impact, "impact section"),
    resolution_boundary: string(input.resolution_boundary, "resolution section"),
    acceptance_criteria: string(input.acceptance_criteria, "acceptance section"),
    verification: string(input.verification, "verification section"),
    relationships_and_duplicate_analysis: string(
      input.relationships_and_duplicate_analysis,
      "relationship section",
    ),
    review_and_disposition_history: string(input.review_and_disposition_history, "review section"),
  };
};
const decodeBoardPayload = (value: unknown): BoardPayload => {
  const input = object(value, "board draft");
  return {
    id: string(input.id, "board ID"),
    title: string(input.title, "board title"),
    status_source: decodeBoardStatusSource(input.status_source),
    filter_statuses: optional(input.filter_statuses, (item) =>
      nullable(item, (present) => strings(present, "board status filters")),
    ),
    filter_kinds: optional(input.filter_kinds, (item) =>
      nullable(item, (present) => strings(present, "board kind filters")),
    ),
    columns: array(input.columns, "board columns", (item) => {
      const column = object(item, "board column");
      return {
        name: string(column.name, "board column name"),
        statuses: strings(column.statuses, "board column statuses"),
      };
    }),
  };
};
const decodeBoardDraft = (value: unknown): BoardDraft => {
  const board = decodeBoardPayload(value);
  return {
    ...board,
    columns: board.columns.map((column) => ({
      ...column,
      editor_key: `${board.id}:${column.name}`,
    })),
  };
};
const decodeRecord = (value: unknown): Record => {
  const input = object(value, "record");
  return {
    path: string(input.path, "record path"),
    scope: optional(input.scope, decodeScope),
    classification: decodeClassification(input.classification),
    kind: optional(input.kind, decodeKind),
    identity: optional(input.identity, decodeIdentity),
    title: string(input.title, "record title"),
    content: optional(input.content, (item) => string(item, "record content")),
    rendered_markdown: optional(input.rendered_markdown, (item) =>
      string(item, "rendered Markdown"),
    ),
    work_item: optional(input.work_item, decodeWorkItem),
    board: optional(input.board, decodeBoardDraft),
    strategy: optional(input.strategy, decodeStrategyProjection),
    strategy_binding: optional(input.strategy_binding, decodeStrategyBindingRecord),
  };
};
const decodeCard = (value: unknown): Card => {
  const input = object(value, "board card");
  return {
    identity: decodeIdentity(input.identity),
    kind: decodeKind(input.kind),
    title: string(input.title, "card title"),
    status: string(input.status, "card status"),
    rank: nullable(input.rank, (item) => number(item, "card rank")),
  };
};
const decodeBoard = (value: unknown): Board => {
  const input = object(value, "board");
  return {
    identity: decodeIdentity(input.identity),
    title: string(input.title, "board title"),
    status_source: decodeBoardStatusSource(input.status_source),
    filter_statuses: nullable(input.filter_statuses, (item) =>
      strings(item, "board status filters"),
    ),
    filter_kinds: nullable(input.filter_kinds, (item) => strings(item, "board kind filters")),
    columns: array(input.columns, "board columns", (item) => {
      const column = object(item, "derived board column");
      return {
        name: string(column.name, "board column name"),
        statuses: strings(column.statuses, "board column statuses"),
        cards: array(column.cards, "board cards", decodeCard),
      };
    }),
  };
};
const decodeBoardStatusSource = (value: unknown): BoardStatusSource => {
  if (value === "disposition" || value === "progress") return value;
  return contractError("board status source");
};
const decodeRelationship = (value: unknown): Relationship => {
  const input = object(value, "relationship");
  const kind = input.kind;
  if (
    kind !== "decision" &&
    kind !== "related" &&
    kind !== "supersedes" &&
    kind !== "superseded_by"
  )
    return contractError("relationship kind");
  return { source: decodeIdentity(input.source), target: decodeIdentity(input.target), kind };
};
const decodeDiagnostic = (value: unknown): Diagnostic => {
  const input = object(value, "diagnostic");
  return {
    code: string(input.code, "diagnostic code"),
    path: string(input.path, "diagnostic path"),
    field: optional(input.field, (item) => string(item, "diagnostic field")),
    section: optional(input.section, (item) => string(item, "diagnostic section")),
    message: string(input.message, "diagnostic message"),
  };
};
const decodeChangeRequest = (value: unknown): ChangeRequest => {
  const input = object(value, "change request");
  if (input.operation !== "replace") return contractError("change operation");
  const draft = object(input.draft, "record draft");
  const kind = draft.kind;
  if (kind === "board")
    return {
      operation: "replace",
      path: string(input.path, "change path"),
      draft: { kind, ...decodeBoardPayload(draft) },
    };
  if (kind !== "ticket" && kind !== "epic") return contractError("draft kind");
  return {
    operation: "replace",
    path: string(input.path, "change path"),
    draft: { kind, ...decodeWorkItem(draft) },
  };
};

export const decodeCurrent = <T>(value: unknown, decode: Decoder<T>): T => {
  return decodeIndexed(value, decode).value;
};
export const decodeIndexed = <T>(value: unknown, decode: Decoder<T>): Indexed<T> => {
  const envelope = object(value, "index envelope");
  const current = object(envelope.Current, "current index");
  return {
    sourceRevision: string(current.source_revision, "source revision"),
    value: decode(current.value),
  };
};
export const decodeRecords = (value: unknown): ReadonlyArray<Record> =>
  array(value, "records", decodeRecord);
export const decodeBoards = (value: unknown): ReadonlyArray<Board> =>
  array(value, "boards", decodeBoard);
export const decodeRelationships = (value: unknown): ReadonlyArray<Relationship> =>
  array(value, "relationships", decodeRelationship);
export const decodeDiagnostics = (value: unknown): ReadonlyArray<Diagnostic> =>
  array(value, "diagnostics", decodeDiagnostic);
export const decodePreview = (value: unknown): Preview => {
  const input = object(value, "preview");
  return {
    request: decodeChangeRequest(input.request),
    expected_target_revision: nullable(input.expected_target_revision, (item) =>
      string(item, "target revision"),
    ),
    expected_store_revision: string(input.expected_store_revision, "store revision"),
    proposed_store_revision: string(input.proposed_store_revision, "proposed revision"),
    diagnostics: decodeDiagnostics(input.diagnostics),
    diff: string(input.diff, "preview diff"),
  };
};
export const decodeApplyResponse = (value: unknown): ApplyResponse => {
  const input = object(value, "apply response");
  const result = object(input.result, "apply result");
  return {
    result: {
      path: string(result.path, "applied path"),
      resulting_target_revision: nullable(result.resulting_target_revision, (item) =>
        string(item, "resulting target revision"),
      ),
      resulting_store_revision: string(result.resulting_store_revision, "resulting revision"),
      diff: string(result.diff, "applied diff"),
    },
    index_error: nullable(input.index_error, (item) => string(item, "index error")),
  };
};
export const decodeHostFailure = (value: unknown, status: number): HostFailure => {
  if (!isObject(value) || typeof value.error !== "string")
    return { message: `The host rejected this request (${status}).`, code: undefined };
  return {
    message: value.error,
    code: value.code === "stale_revision" ? "stale_revision" : undefined,
  };
};
