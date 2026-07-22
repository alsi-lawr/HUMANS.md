import { describe, expect, test } from "bun:test";
import {
  type BoardDraft,
  type Identity,
  type Record,
  editableDraft,
  sameIdentity,
  toChangeRequest,
} from "./model";
import { strategiesForInvestigation } from "./navigation/use-scope-navigation";

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
  strategy: undefined,
  strategy_binding: undefined,
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

test("strategy navigation preserves full investigation scope and recognized invalid records", () => {
  const alpha: Record = {
    ...record,
    path: "projects/humans/investigations/alpha/shared/strategy/review.toml",
    scope: { project: "humans", investigation: "alpha/shared" },
    classification: "invalid",
    kind: "strategy",
    board: undefined,
  };
  const beta: Record = {
    ...record,
    path: "projects/humans/investigations/beta/shared/strategy/review.toml",
    scope: { project: "humans", investigation: "beta/shared" },
    kind: "strategy",
    board: undefined,
  };
  const unrecognized: Record = {
    ...alpha,
    path: "projects/humans/investigations/alpha/shared/strategy/raw.toml",
    classification: "ungoverned",
  };

  expect(
    strategiesForInvestigation([alpha, beta, unrecognized], "humans", "alpha/shared").map(
      (item) => item.path,
    ),
  ).toEqual([alpha.path]);
  expect(strategiesForInvestigation([alpha], "humans", undefined)).toEqual([]);
  expect(editableDraft({ ...beta, classification: "governed" })).toBeUndefined();
  expect(
    editableDraft({
      ...beta,
      path: "projects/humans/investigations/beta/shared/strategy/bindings.toml",
      kind: "strategy_binding",
    }),
  ).toBeUndefined();
});
