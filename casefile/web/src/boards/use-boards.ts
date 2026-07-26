import { useEffect, useState } from "react";
import { fetchBoards } from "../api";
import { type Board, type Diagnostic, type Scope } from "../model";

export type BoardsState =
  | Readonly<{ tag: "no_scope" }>
  | Readonly<{ tag: "loading" }>
  | Readonly<{ tag: "failure"; message: string }>
  | Readonly<{ tag: "stale" }>
  | Readonly<{ tag: "invalid"; diagnostics: ReadonlyArray<Diagnostic> }>
  | Readonly<{ tag: "ready"; boards: ReadonlyArray<Board> }>;

type BoardRequest = Readonly<{
  scope: Scope;
  recordsRevision: string;
  refreshKey: number;
}>;
type StoredBoardsState =
  | Readonly<{ tag: "loading"; request: BoardRequest }>
  | Readonly<{ tag: "failure"; request: BoardRequest; message: string }>
  | Readonly<{ tag: "stale"; request: BoardRequest }>
  | Readonly<{ tag: "ready"; request: BoardRequest; boards: ReadonlyArray<Board> }>;

const requestFor = (
  scope: Scope | undefined,
  recordsRevision: string | undefined,
  refreshKey: number,
): BoardRequest | undefined =>
  scope === undefined || scope.investigation === undefined || recordsRevision === undefined
    ? undefined
    : { scope, recordsRevision, refreshKey };

const sameRequest = (left: BoardRequest, right: BoardRequest): boolean =>
  left.scope.project === right.scope.project &&
  left.scope.investigation === right.scope.investigation &&
  left.recordsRevision === right.recordsRevision &&
  left.refreshKey === right.refreshKey;

const scopedBoardDiagnostics = (
  scope: Scope | undefined,
  diagnostics: ReadonlyArray<Diagnostic>,
): ReadonlyArray<Diagnostic> => {
  if (scope === undefined || scope.investigation === undefined) return [];
  const prefix = `projects/${scope.project}/investigations/${scope.investigation}/`;
  return diagnostics.filter(
    (diagnostic) =>
      diagnostic.path.startsWith(`${prefix}boards/`) ||
      diagnostic.path === `${prefix}progress/log.toml`,
  );
};

const visibleState = (
  request: BoardRequest | undefined,
  diagnostics: ReadonlyArray<Diagnostic>,
  state: StoredBoardsState | undefined,
): BoardsState => {
  if (request === undefined) return { tag: "no_scope" };
  if (diagnostics.length > 0) return { tag: "invalid", diagnostics };
  if (state === undefined || !sameRequest(state.request, request)) return { tag: "loading" };
  if (state.tag === "loading") return { tag: "loading" };
  if (state.tag === "failure") return { tag: "failure", message: state.message };
  if (state.tag === "stale") return { tag: "stale" };
  return { tag: "ready", boards: state.boards };
};

export const useBoards = (
  scope: Scope | undefined,
  recordsRevision: string | undefined,
  refreshKey: number,
  diagnostics: ReadonlyArray<Diagnostic>,
): BoardsState => {
  const request = requestFor(scope, recordsRevision, refreshKey);
  const invalidDiagnostics = scopedBoardDiagnostics(scope, diagnostics);
  const [state, setState] = useState<StoredBoardsState>();

  useEffect(() => {
    if (request === undefined || invalidDiagnostics.length > 0) return;
    const controller = new AbortController();
    setState({ tag: "loading", request });
    void fetchBoards(request.scope, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      if (result.tag === "failure") {
        setState({ tag: "failure", request, message: result.message });
        return;
      }
      if (result.value.sourceRevision !== request.recordsRevision) {
        setState({ tag: "stale", request });
        return;
      }
      setState({ tag: "ready", request, boards: result.value.value });
    });
    return () => controller.abort();
  }, [diagnostics, recordsRevision, refreshKey, scope?.investigation, scope?.project]);

  return visibleState(request, invalidDiagnostics, state);
};
