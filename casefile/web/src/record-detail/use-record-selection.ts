import { useEffect, useState } from "react";
import { fetchRelationships } from "../api";
import {
  type Draft,
  type Identity,
  type Record,
  type Relationship,
  editableDraft,
  sameIdentity,
} from "../model";

export type RecordSelection = Readonly<{
  selected: Identity | undefined;
  record: Record | undefined;
  relationships: ReadonlyArray<Relationship>;
  draft: Draft | undefined;
  error: string | undefined;
  selectRecord: (identity: Identity) => void;
  clearRecord: () => void;
  updateDraft: (draft: Draft) => void;
}>;
type RelationshipQuery =
  | Readonly<{ tag: "ready"; relationships: ReadonlyArray<Relationship> }>
  | Readonly<{ tag: "failure"; message: string }>;

export const useRecordSelection = (records: ReadonlyArray<Record>): RecordSelection => {
  const [selected, setSelected] = useState<Identity | undefined>(undefined);
  const [query, setQuery] = useState<RelationshipQuery>({ tag: "ready", relationships: [] });
  const [draft, setDraft] = useState<Draft | undefined>(undefined);

  useEffect(() => {
    if (selected === undefined) {
      setQuery({ tag: "ready", relationships: [] });
      return;
    }
    const controller = new AbortController();
    void fetchRelationships(selected, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      if (result.tag === "success") {
        setQuery({ tag: "ready", relationships: result.value });
        return;
      }
      setQuery({ tag: "failure", message: result.message });
    });
    return () => controller.abort();
  }, [selected]);

  const record = findRecord(records, selected);
  return {
    selected,
    record,
    relationships: query.tag === "ready" ? query.relationships : [],
    draft,
    error: query.tag === "failure" ? query.message : undefined,
    selectRecord: (identity) => {
      const record = findRecord(records, identity);
      setSelected(identity);
      setDraft(record === undefined ? undefined : editableDraft(record));
      setQuery({ tag: "ready", relationships: [] });
    },
    clearRecord: () => {
      setSelected(undefined);
      setDraft(undefined);
      setQuery({ tag: "ready", relationships: [] });
    },
    updateDraft: setDraft,
  };
};

const findRecord = (
  records: ReadonlyArray<Record>,
  identity: Identity | undefined,
): Record | undefined =>
  identity === undefined
    ? undefined
    : records.find(
        (record) => record.identity !== undefined && sameIdentity(record.identity, identity),
      );
