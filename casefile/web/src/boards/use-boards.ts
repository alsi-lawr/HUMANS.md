import { useEffect, useState } from "react";
import { fetchBoards } from "../api";
import { type Board, type Scope } from "../model";

export type BoardsState =
  | Readonly<{ tag: "no_scope" }>
  | Readonly<{ tag: "loading" }>
  | Readonly<{ tag: "failure"; message: string }>
  | Readonly<{ tag: "stale" }>
  | Readonly<{ tag: "ready"; boards: ReadonlyArray<Board> }>;

export const useBoards = (
  scope: Scope | undefined,
  recordsRevision: string | undefined,
  refreshKey: number,
): BoardsState => {
  const [state, setState] = useState<BoardsState>({ tag: "no_scope" });
  const project = scope?.project;
  const investigation = scope?.investigation;

  useEffect(() => {
    if (scope === undefined || scope.investigation === undefined) {
      setState({ tag: "no_scope" });
      return;
    }
    const controller = new AbortController();
    setState({ tag: "loading" });
    void fetchBoards(scope, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      if (result.tag === "failure") {
        setState({ tag: "failure", message: result.message });
        return;
      }
      if (result.value.sourceRevision !== recordsRevision) {
        setState({ tag: "stale" });
        return;
      }
      setState({ tag: "ready", boards: result.value.value });
    });
    return () => controller.abort();
  }, [investigation, project, recordsRevision, refreshKey]);

  return state;
};
