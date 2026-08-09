export type Classification = "governed" | "ungoverned" | "invalid" | "raw";
export type Kind =
  | "activation"
  | "project_map"
  | "request"
  | "decision"
  | "evidence"
  | "review"
  | "plan"
  | "closeout"
  | "strategy"
  | "strategy_binding"
  | "strategy_transition"
  | "progress"
  | "ticket"
  | "epic"
  | "board";

export type Scope = Readonly<{ project: string; investigation: string | undefined }>;
export type Identity = Readonly<{ scope: Scope; identity: string }>;
export type Diagnostic = Readonly<{
  code: string;
  path: string;
  field: string | undefined;
  section: string | undefined;
  message: string;
}>;

export type EffectiveWriterBinding = Readonly<{
  model: string;
  reasoning_effort: string;
  source: "matrix" | "binding";
}>;
export type StrategyBindingState =
  | Readonly<{ state: "absent"; effective: EffectiveWriterBinding }>
  | Readonly<{ state: "pending" }>
  | Readonly<{ state: "resolved"; effective: EffectiveWriterBinding }>
  | Readonly<{ state: "unresolved" }>
  | Readonly<{ state: "invalid" }>;
export type StrategyWorker = Readonly<{
  role: string;
  platform_profile: string;
  runtime:
    | Readonly<{ tag: "unspecified" }>
    | Readonly<{ tag: "declared"; model: string; reasoning_effort: string }>;
  minimum_count: number;
  maximum_count: number;
  can_spawn_subagents: boolean;
}>;
export type StrategyPipeline = Readonly<{
  maximum_active_tickets: number;
  look_ahead_read_only: boolean;
  require_dependency_independence: boolean;
  require_disjoint_write_paths: boolean;
  immutable_review_commits: boolean;
  corrections_preempt_forward_work: boolean;
}>;
export type StrategyProjection = Readonly<{
  root_binding: "root";
  limits: Readonly<{ max_concurrent_subagents: number; max_depth: number }>;
  requirements: Readonly<{ capabilities: ReadonlyArray<string> }>;
  workers: ReadonlyArray<StrategyWorker>;
  coordination: Readonly<{
    batch_when_capacity_exceeded: boolean;
    candidate_review_before_ticket: boolean;
    shared_ticket_storage_required: boolean;
    pipeline: StrategyPipeline | undefined;
  }>;
  binding: StrategyBindingState | null;
}>;
export type StrategyBindingRecord = Readonly<{
  adapter: string;
  role: "implementation-writer";
  model: string;
  reasoning_effort: string;
  resolution: Readonly<{ mode: string; value: string }>;
  state: StrategyBindingState;
}>;

export type WorkItemDraft = Readonly<{
  id: string;
  title: string;
  project: string;
  investigation: string;
  status: string;
  reported_by_role: string;
  reported_by_agent: string;
  source_commit: string;
  created_at: string;
  updated_at: string;
  confidence: string;
  decision_refs: ReadonlyArray<string>;
  related_tickets: ReadonlyArray<string>;
  supersedes: ReadonlyArray<string>;
  superseded_by: ReadonlyArray<string>;
  rank: number | undefined;
  requirement_and_evidence: string;
  impact: string;
  resolution_boundary: string;
  acceptance_criteria: string;
  verification: string;
  relationships_and_duplicate_analysis: string;
  review_and_disposition_history: string;
}>;
export type BoardColumnPayload = Readonly<{ name: string; statuses: ReadonlyArray<string> }>;
export type BoardStatusSource = "disposition" | "progress";
export type BoardColumn = BoardColumnPayload & Readonly<{ editor_key: string }>;
export type BoardPayload = Readonly<{
  id: string;
  title: string;
  status_source: BoardStatusSource;
  filter_statuses: ReadonlyArray<string> | null | undefined;
  filter_kinds: ReadonlyArray<string> | null | undefined;
  columns: ReadonlyArray<BoardColumnPayload>;
}>;
export type BoardDraft = Readonly<{
  id: string;
  title: string;
  status_source: BoardStatusSource;
  filter_statuses: ReadonlyArray<string> | null | undefined;
  filter_kinds: ReadonlyArray<string> | null | undefined;
  columns: ReadonlyArray<BoardColumn>;
}>;
export type Draft =
  | Readonly<{ kind: "ticket" | "epic"; value: WorkItemDraft }>
  | Readonly<{ kind: "board"; value: BoardDraft }>;
type RecordDraft =
  | (WorkItemDraft & Readonly<{ kind: "ticket" | "epic" }>)
  | (BoardPayload & Readonly<{ kind: "board" }>);

export type Record = Readonly<{
  path: string;
  scope: Scope | undefined;
  classification: Classification;
  kind: Kind | undefined;
  identity: Identity | undefined;
  title: string;
  content: string | undefined;
  rendered_markdown: string | undefined;
  work_item: WorkItemDraft | undefined;
  board: BoardDraft | undefined;
  strategy: StrategyProjection | undefined;
  strategy_binding: StrategyBindingRecord | undefined;
}>;
export type Card = Readonly<{
  identity: Identity;
  kind: Kind;
  title: string;
  status: string;
  rank: number | null;
}>;
export type Board = Readonly<{
  identity: Identity;
  title: string;
  status_source: BoardStatusSource;
  filter_statuses: ReadonlyArray<string> | null;
  filter_kinds: ReadonlyArray<string> | null;
  columns: ReadonlyArray<
    Readonly<{ name: string; statuses: ReadonlyArray<string>; cards: ReadonlyArray<Card> }>
  >;
}>;
export type Relationship = Readonly<{
  source: Identity;
  target: Identity;
  kind: "decision" | "related" | "supersedes" | "superseded_by";
}>;
export type ChangeRequest = Readonly<{ operation: "replace"; path: string; draft: RecordDraft }>;
export type Preview = Readonly<{
  preview_id: string;
  approval_required: boolean;
  rendered_bytes: ReadonlyArray<number> | null;
  no_op: boolean;
  request: ChangeRequest;
  expected_target_revision: string | null;
  diagnostics: ReadonlyArray<Diagnostic>;
  diff: string;
}>;
export type ApplyResponse = Readonly<{
  result: Readonly<{
    path: string;
    resulting_target_revision: string | null;
    resulting_store_revision: string;
    diff: string;
    no_op: boolean;
  }>;
  cache: Readonly<{
    state: "not_configured" | "current" | "degraded";
    source_revision?: string;
    message?: string;
  }>;
}>;

export const toChangeRequest = (path: string, draft: Draft): ChangeRequest =>
  draft.kind === "board"
    ? {
        operation: "replace",
        path,
        draft: {
          kind: "board",
          ...draft.value,
          columns: draft.value.columns.map(({ name, statuses }) => ({ name, statuses })),
        },
      }
    : { operation: "replace", path, draft: { kind: draft.kind, ...draft.value } };
export const editableDraft = (record: Record): Draft | undefined => {
  if (record.classification !== "governed") return undefined;
  if ((record.kind === "ticket" || record.kind === "epic") && record.work_item !== undefined)
    return { kind: record.kind, value: record.work_item };
  return record.kind === "board" && record.board !== undefined
    ? { kind: "board", value: record.board }
    : undefined;
};
export const sameScope = (left: Scope, right: Scope): boolean =>
  left.project === right.project && left.investigation === right.investigation;
export const sameIdentity = (left: Identity, right: Identity): boolean =>
  left.identity === right.identity && sameScope(left.scope, right.scope);
export const scopeLabel = (scope: Scope): string =>
  scope.investigation === undefined ? scope.project : `${scope.project} / ${scope.investigation}`;
