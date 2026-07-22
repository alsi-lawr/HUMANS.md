import { useEffect, useState } from "react";
import { fetchBoards } from "../api";
import { type Board, type Record, type Scope, sameScope } from "../model";

export type ScopeNavigation = Readonly<{
  scope: Scope | undefined;
  scopes: ReadonlyArray<Scope>;
  records: ReadonlyArray<Record>;
  boards: ReadonlyArray<Board>;
  error: string | undefined;
  selectScope: (scope: Scope | undefined) => void;
}>;
type BoardQuery =
  | Readonly<{ tag: "ready"; boards: ReadonlyArray<Board> }>
  | Readonly<{ tag: "failure"; message: string }>;

export const useScopeNavigation = (allRecords: ReadonlyArray<Record>): ScopeNavigation => {
  const [scope, setScope] = useState<Scope | undefined>(undefined);
  const [query, setQuery] = useState<BoardQuery>({ tag: "ready", boards: [] });

  useEffect(() => {
    if (scope === undefined) {
      setQuery({ tag: "ready", boards: [] });
      return;
    }
    const controller = new AbortController();
    setQuery({ tag: "ready", boards: [] });
    void fetchBoards(scope, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      if (result.tag === "success") {
        setQuery({ tag: "ready", boards: result.value });
        return;
      }
      setQuery({ tag: "failure", message: result.message });
    });
    return () => controller.abort();
  }, [scope]);

  const records = allRecords.filter(
    (record) =>
      scope === undefined || (record.scope !== undefined && sameScope(record.scope, scope)),
  );

  return {
    scope,
    scopes: uniqueScopes(allRecords),
    records,
    boards: query.tag === "ready" ? query.boards : [],
    error: query.tag === "failure" ? query.message : undefined,
    selectScope: setScope,
  };
};

const uniqueScopes = (records: ReadonlyArray<Record>): ReadonlyArray<Scope> =>
  records
    .flatMap((record) => (record.scope === undefined ? [] : [record.scope]))
    .filter(
      (scope, index, all) => all.findIndex((candidate) => sameScope(candidate, scope)) === index,
    )
    .sort((left, right) =>
      `${left.project}/${left.investigation ?? ""}`.localeCompare(
        `${right.project}/${right.investigation ?? ""}`,
      ),
    );
