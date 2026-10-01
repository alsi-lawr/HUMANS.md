import { useEffect, useReducer } from "react";
import { fetchWorkspace } from "../api";
import {
  type Diagnostic,
  type Record,
  type WorkspaceContext,
  type WorkspaceReadToken,
  type WorkspaceResponse,
} from "../model";

type ReadyWorkspace = Readonly<{
  tag: "ready";
  records: ReadonlyArray<Record>;
  unfilteredRecords: ReadonlyArray<Record>;
  sourceRevision: string;
  freshness: WorkspaceReadToken;
  diagnostics: ReadonlyArray<Diagnostic>;
}>;
export type Workspace =
  Readonly<{ tag: "loading" }> | Readonly<{ tag: "failure"; message: string }> | ReadyWorkspace;
export type WorkspaceController = Readonly<{
  workspace: Workspace;
  search: string;
  setSearch: (value: string) => void;
  refresh: () => void;
  refreshKey: number;
  context: () => WorkspaceContext;
  accept: (value: WorkspaceResponse, context: WorkspaceContext) => void;
}>;

const sameToken = (left: WorkspaceReadToken, right: WorkspaceReadToken): boolean =>
  left.source_revision.value === right.source_revision.value &&
  left.publication_id.value === right.publication_id.value &&
  left.provider_instance.value === right.provider_instance.value;

const reconcile = (value: WorkspaceResponse, baseline: ReadyWorkspace | undefined): Workspace => {
  if (
    value.state === "unchanged" &&
    (baseline === undefined || !sameToken(value.freshness, baseline.freshness))
  )
    return { tag: "failure", message: "The host reused a different workspace baseline." };
  const unfilteredRecords = value.state === "updated" ? value.records : baseline?.unfilteredRecords;
  const diagnostics = value.state === "updated" ? value.diagnostics : baseline?.diagnostics;
  if (unfilteredRecords === undefined || diagnostics === undefined)
    return { tag: "failure", message: "No workspace baseline is available." };
  const paths = new Set(value.matching_paths);
  const filtered = unfilteredRecords.filter((record) => paths.has(record.path));
  if (filtered.length !== paths.size)
    return { tag: "failure", message: "The host returned unknown workspace paths." };
  return {
    tag: "ready",
    records: filtered,
    unfilteredRecords,
    sourceRevision: value.freshness.source_revision.value,
    freshness: value.freshness,
    diagnostics,
  };
};

type RequestState =
  | Readonly<{ tag: "idle" }>
  | Readonly<{ tag: "pending"; sequence: number; context: WorkspaceContext }>;
type State = Readonly<{
  workspace: Workspace;
  search: string;
  refreshKey: number;
  sequence: number;
  request: RequestState;
}>;
type Action =
  | Readonly<{ tag: "search"; value: string }>
  | Readonly<{ tag: "refresh" }>
  | Readonly<{ tag: "received"; sequence: number; value: WorkspaceResponse }>
  | Readonly<{ tag: "failed"; sequence: number; message: string }>
  | Readonly<{ tag: "applied"; value: WorkspaceResponse; context: WorkspaceContext }>;

const contextFor = (workspace: Workspace, search: string): WorkspaceContext => ({
  knownToken: workspace.tag === "ready" ? workspace.freshness : undefined,
  search: search.trim() || undefined,
});
const pending = (state: State): State => {
  const sequence = state.sequence + 1;
  return {
    ...state,
    sequence,
    request: { tag: "pending", sequence, context: contextFor(state.workspace, state.search) },
  };
};
const initial: State = pending({
  workspace: { tag: "loading" },
  search: "",
  refreshKey: 0,
  sequence: 0,
  request: { tag: "idle" },
});
const reducer = (state: State, action: Action): State => {
  if (action.tag === "search")
    return action.value === state.search ? state : pending({ ...state, search: action.value });
  if (action.tag === "refresh") return pending({ ...state, refreshKey: state.refreshKey + 1 });
  if (action.tag === "failed" || action.tag === "received") {
    if (state.request.tag !== "pending" || action.sequence !== state.request.sequence) return state;
    if (action.tag === "failed")
      return {
        ...state,
        workspace: { tag: "failure", message: action.message },
        request: { tag: "idle" },
      };
    const before = state.workspace.tag === "ready" ? state.workspace : undefined;
    const workspace = reconcile(action.value, before);
    const changed =
      before !== undefined &&
      workspace.tag === "ready" &&
      !sameToken(before.freshness, workspace.freshness);
    return {
      ...state,
      workspace,
      refreshKey: state.refreshKey + Number(changed),
      request: { tag: "idle" },
    };
  }
  const before = state.workspace.tag === "ready" ? state.workspace : undefined;
  // A newer request may already have adopted another publication while apply ran.
  if (
    before !== undefined &&
    action.context.knownToken !== undefined &&
    !sameToken(before.freshness, action.context.knownToken) &&
    !sameToken(before.freshness, action.value.freshness)
  )
    return pending(state);
  let workspace = reconcile(action.value, before);
  const next = {
    ...state,
    workspace,
    sequence: state.sequence + 1,
    refreshKey: state.refreshKey + 1,
    request: { tag: "idle" } as const,
  };
  if ((action.context.search ?? "") === state.search.trim() || workspace.tag !== "ready")
    return next;
  // Retain the current visible selection while resolving the new search against the new baseline.
  if (before !== undefined) {
    const visible = new Set(before.records.map((record) => record.path));
    workspace = {
      ...workspace,
      records: workspace.unfilteredRecords.filter((record) => visible.has(record.path)),
    };
  }
  return pending({ ...next, workspace });
};

export const useWorkspace = (): WorkspaceController => {
  const [state, dispatch] = useReducer(reducer, initial);
  useEffect(() => {
    if (state.request.tag === "idle") return;
    const request = state.request;
    const controller = new AbortController();
    void fetchWorkspace(request.context, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      dispatch(
        result.tag === "failure"
          ? { tag: "failed", sequence: request.sequence, message: result.message }
          : { tag: "received", sequence: request.sequence, value: result.value },
      );
    });
    return () => controller.abort();
  }, [state.request]);

  return {
    workspace: state.workspace,
    search: state.search,
    refreshKey: state.refreshKey,
    setSearch: (value) => dispatch({ tag: "search", value }),
    refresh: () => dispatch({ tag: "refresh" }),
    context: () => contextFor(state.workspace, state.search),
    accept: (value, context) => dispatch({ tag: "applied", value, context }),
  };
};
