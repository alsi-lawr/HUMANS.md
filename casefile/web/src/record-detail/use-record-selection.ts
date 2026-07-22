import { useEffect, useState } from "react";
import { fetchRelationships } from "../api";
import { type Draft, type Record, type Relationship, editableDraft } from "../model";

export type RecordSelection = Readonly<{
  selectedPath: string | undefined;
  record: Record | undefined;
  relationships: ReadonlyArray<Relationship>;
  draft: Draft | undefined;
  error: string | undefined;
  selectRecord: (record: Record) => void;
  clearRecord: () => void;
  updateDraft: (draft: Draft) => void;
}>;
type RelationshipQuery =
  | Readonly<{ tag: "ready"; relationships: ReadonlyArray<Relationship> }>
  | Readonly<{ tag: "failure"; message: string }>;

const emptyQuery: RelationshipQuery = { tag: "ready", relationships: [] };

export const useRecordSelection = (records: ReadonlyArray<Record>): RecordSelection => {
  const [selectedPath, setSelectedPath] = useState<string | undefined>(undefined);
  const [query, setQuery] = useState<RelationshipQuery>(emptyQuery);
  const [draft, setDraft] = useState<Draft | undefined>(undefined);

  useEffect(() => {
    const record = findRecord(records, selectedPath);
    if (record?.identity === undefined) {
      setQuery(emptyQuery);
      return;
    }
    const controller = new AbortController();
    void fetchRelationships(record.identity, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      if (result.tag === "success") {
        setQuery({ tag: "ready", relationships: result.value });
        return;
      }
      setQuery({ tag: "failure", message: result.message });
    });
    return () => controller.abort();
  }, [records, selectedPath]);

  const record = findRecord(records, selectedPath);
  return {
    selectedPath,
    record,
    relationships: query.tag === "ready" ? query.relationships : [],
    draft,
    error: query.tag === "failure" ? query.message : undefined,
    selectRecord: (record) => {
      setSelectedPath(record.path);
      setDraft(editableDraft(record));
      setQuery(emptyQuery);
    },
    clearRecord: () => {
      setSelectedPath(undefined);
      setDraft(undefined);
      setQuery(emptyQuery);
    },
    updateDraft: setDraft,
  };
};

const findRecord = (
  records: ReadonlyArray<Record>,
  path: string | undefined,
): Record | undefined =>
  path === undefined ? undefined : records.find((record) => record.path === path);
