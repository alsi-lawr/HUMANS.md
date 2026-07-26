import { useEffect, useState } from "react";
import { fetchDiagnostics, fetchRecords } from "../api";
import { type Diagnostic, type Record } from "../model";

export type Workspace =
  | Readonly<{ tag: "loading" }>
  | Readonly<{ tag: "failure"; message: string }>
  | Readonly<{
      tag: "ready";
      records: ReadonlyArray<Record>;
      unfilteredRecords: ReadonlyArray<Record>;
      sourceRevision: string;
      diagnostics: ReadonlyArray<Diagnostic>;
    }>;

export type WorkspaceController = Readonly<{
  workspace: Workspace;
  search: string;
  setSearch: (value: string) => void;
  refresh: () => void;
  refreshKey: number;
}>;

export const useWorkspace = (): WorkspaceController => {
  const [workspace, setWorkspace] = useState<Workspace>({ tag: "loading" });
  const [search, setSearch] = useState("");
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    setWorkspace((current) => (current.tag === "ready" ? current : { tag: "loading" }));
    const filter = search.trim();
    const filtered = fetchRecords(filter === "" ? undefined : filter, controller.signal);
    const all = filter === "" ? filtered : fetchRecords(undefined, controller.signal);
    void Promise.all([filtered, all, fetchDiagnostics(controller.signal)]).then((results) => {
      const [records, unfilteredRecords, diagnostics] = results;
      if (controller.signal.aborted) return;
      if (records.tag === "failure") {
        setWorkspace({ tag: "failure", message: records.message });
        return;
      }
      if (unfilteredRecords.tag === "failure") {
        setWorkspace({ tag: "failure", message: unfilteredRecords.message });
        return;
      }
      if (diagnostics.tag === "failure") {
        setWorkspace({ tag: "failure", message: diagnostics.message });
        return;
      }
      setWorkspace({
        tag: "ready",
        records: records.value.value,
        unfilteredRecords: unfilteredRecords.value.value,
        sourceRevision: unfilteredRecords.value.sourceRevision,
        diagnostics: diagnostics.value.value,
      });
    });
    return () => controller.abort();
  }, [refreshKey, search]);

  return {
    workspace,
    search,
    setSearch,
    refresh: () => setRefreshKey((current) => current + 1),
    refreshKey,
  };
};
