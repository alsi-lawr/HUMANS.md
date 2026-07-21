import { useEffect, useState } from "react";
import { fetchDiagnostics, fetchRecords } from "../api";
import { type Diagnostic, type Record } from "../model";

export type Workspace =
  | Readonly<{ tag: "loading" }>
  | Readonly<{ tag: "failure"; message: string }>
  | Readonly<{
      tag: "ready";
      records: ReadonlyArray<Record>;
      diagnostics: ReadonlyArray<Diagnostic>;
    }>;

export type WorkspaceController = Readonly<{
  workspace: Workspace;
  search: string;
  setSearch: (value: string) => void;
  refresh: () => void;
}>;

export const useWorkspace = (): WorkspaceController => {
  const [workspace, setWorkspace] = useState<Workspace>({ tag: "loading" });
  const [search, setSearch] = useState("");
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    setWorkspace((current) => (current.tag === "ready" ? current : { tag: "loading" }));
    const filter = search.trim();
    void Promise.all([
      fetchRecords(filter === "" ? undefined : filter, controller.signal),
      fetchDiagnostics(controller.signal),
    ]).then(([records, diagnostics]) => {
      if (controller.signal.aborted) return;
      if (records.tag === "failure") {
        setWorkspace({ tag: "failure", message: records.message });
        return;
      }
      if (diagnostics.tag === "failure") {
        setWorkspace({ tag: "failure", message: diagnostics.message });
        return;
      }
      setWorkspace({ tag: "ready", records: records.value, diagnostics: diagnostics.value });
    });
    return () => controller.abort();
  }, [refreshKey, search]);

  return {
    workspace,
    search,
    setSearch,
    refresh: () => setRefreshKey((current) => current + 1),
  };
};
