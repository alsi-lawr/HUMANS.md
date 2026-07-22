import { describe, expect, test } from "bun:test";
import {
  type BoardDraft,
  type Identity,
  type Record,
  editableDraft,
  sameIdentity,
  toChangeRequest,
} from "./model";

const board: BoardDraft = {
  id: "HMD-board",
  title: "Delivery",
  filter_statuses: ["accepted"],
  filter_kinds: ["ticket"],
  columns: [{ editor_key: "HMD-board:Accepted", name: "Accepted", statuses: ["accepted"] }],
};

const record: Record = {
  path: "projects/humans/investigations/tooling/boards/delivery.toml",
  scope: { project: "humans", investigation: "tooling" },
  classification: "governed",
  kind: "board",
  identity: {
    scope: { project: "humans", investigation: "tooling" },
    identity: board.id,
  },
  title: board.title,
  content: undefined,
  rendered_markdown: undefined,
  work_item: undefined,
  board,
};

describe("host mutation boundary", () => {
  test("flattens a typed board into the Rust tagged-enum shape", () => {
    expect(toChangeRequest(record.path, { kind: "board", value: board })).toEqual({
      operation: "replace",
      path: record.path,
      draft: {
        kind: "board",
        ...board,
        columns: [{ name: "Accepted", statuses: ["accepted"] }],
      },
    });
  });

  test("only exposes complete governed drafts for editing", () => {
    expect(editableDraft(record)).toEqual({ kind: "board", value: board });
    expect(editableDraft({ ...record, classification: "raw" })).toBeUndefined();
  });
});

test("scoped identity comparison includes project and investigation", () => {
  const identity: Identity = record.identity ?? {
    scope: { project: "humans", investigation: "tooling" },
    identity: board.id,
  };
  expect(sameIdentity(identity, { ...identity })).toBeTrue();
  expect(
    sameIdentity(identity, { ...identity, scope: { ...identity.scope, investigation: "other" } }),
  ).toBeFalse();
});
