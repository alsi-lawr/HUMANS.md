import { expect, test } from "bun:test";
import { decodeBoards, decodeCurrent, decodeHostFailure, decodeRecords } from "./api-contract";

const projectDecision = {
  path: "projects/demo/decision-log/HMD-D-002-project.md",
  scope: { project: "demo" },
  classification: "governed",
  kind: "decision",
  identity: { scope: { project: "demo" }, identity: "HMD-D-002" },
  title: "HMD-D-002 - Project",
  content: "# HMD-D-002 - Project",
  rendered_markdown: "<h1>HMD-D-002 - Project</h1>",
  search_text: "HMD-D-002 - Project",
};

test("decodes the owned project-scope wire shape without a null investigation", () => {
  const records = decodeCurrent(
    { Current: { source_revision: "sha256:current", value: [projectDecision] } },
    decodeRecords,
  );

  expect(records[0]?.scope).toEqual({ project: "demo", investigation: undefined });
  expect(records[0]?.rendered_markdown).toContain("<h1>");
  expect(() =>
    decodeCurrent(
      {
        Current: {
          source_revision: "sha256:current",
          value: [
            {
              ...projectDecision,
              scope: { project: "demo", investigation: null },
            },
          ],
        },
      },
      decodeRecords,
    ),
  ).toThrow("invalid scope investigation");
});

test("accepts the canonical progress record kind and rejects future kinds", () => {
  const progress = {
    ...projectDecision,
    path: "projects/demo/investigations/sample/progress/log.toml",
    scope: { project: "demo", investigation: "sample" },
    kind: "progress",
    identity: {
      scope: { project: "demo", investigation: "sample" },
      identity: "progress/log",
    },
    title: "Progress log",
  };

  expect(decodeRecords([progress])[0]?.kind).toBe("progress");
  expect(() => decodeRecords([{ ...progress, kind: "future_kind" }])).toThrow(
    "invalid record kind",
  );
});

test("accepts the canonical strategy transition record kind", () => {
  const transition = {
    ...projectDecision,
    path: "projects/demo/investigations/sample/strategy/transitions/change.toml",
    scope: { project: "demo", investigation: "sample" },
    kind: "strategy_transition",
    identity: {
      scope: { project: "demo", investigation: "sample" },
      identity: "strategy-transition:change",
    },
    title: "Strategy transition",
  };

  expect(decodeRecords([transition])[0]?.kind).toBe("strategy_transition");
});

test("preserves the host stale-revision failure code", () => {
  expect(
    decodeHostFailure({ error: "stale target revision", code: "stale_revision" }, 409),
  ).toEqual({ message: "stale target revision", code: "stale_revision" });
});

test("decodes both canonical board status sources and rejects malformed selectors", () => {
  const boards = decodeBoards([
    {
      identity: { scope: { project: "demo", investigation: "sample" }, identity: "HMD-board" },
      title: "Disposition",
      status_source: "disposition",
      filter_statuses: null,
      filter_kinds: ["ticket"],
      columns: [{ name: "Accepted", statuses: ["accepted"], cards: [] }],
    },
    {
      identity: { scope: { project: "demo", investigation: "sample" }, identity: "HMD-progress" },
      title: "Progress",
      status_source: "progress",
      filter_statuses: null,
      filter_kinds: ["ticket"],
      columns: [{ name: "Unknown", statuses: ["unknown"], cards: [] }],
    },
  ]);

  expect(boards.map((board) => board.status_source)).toEqual(["disposition", "progress"]);
  expect(() =>
    decodeBoards([
      {
        ...boards[0],
        status_source: "future-state",
      },
    ]),
  ).toThrow("board status source");
});

const strategyProjection = {
  root_binding: "root",
  limits: { max_concurrent_subagents: 3, max_depth: 2 },
  requirements: { capabilities: ["subagents"] },
  workers: [
    {
      role: "implementation-writer",
      platform_profile: "casefile-writer",
      model: "gpt-5.6-sol",
      reasoning_effort: "high",
      minimum_count: 1,
      maximum_count: 1,
      can_spawn_subagents: false,
    },
  ],
  coordination: {
    batch_when_capacity_exceeded: true,
    candidate_review_before_ticket: true,
    shared_ticket_storage_required: true,
    pipeline: {
      maximum_active_tickets: 2,
      look_ahead_read_only: true,
      require_dependency_independence: true,
      require_disjoint_write_paths: true,
      immutable_review_commits: true,
      corrections_preempt_forward_work: true,
    },
  },
  binding: {
    state: "resolved",
    effective: {
      model: "gpt-5.6-terra",
      reasoning_effort: "xhigh",
      source: "binding",
    },
  },
};

test("strictly decodes typed strategy and binding projections", () => {
  const records = decodeRecords([
    {
      ...projectDecision,
      path: "projects/demo/investigations/sample/strategy/implementation.toml",
      scope: { project: "demo", investigation: "sample" },
      kind: "strategy",
      strategy: strategyProjection,
    },
    {
      ...projectDecision,
      path: "projects/demo/investigations/sample/strategy/bindings.toml",
      scope: { project: "demo", investigation: "sample" },
      kind: "strategy_binding",
      strategy_binding: {
        adapter: "codex",
        role: "implementation-writer",
        model: "gpt-5.6-terra",
        reasoning_effort: "xhigh",
        resolution: { mode: "catalog_id", value: "gpt-5.6-terra/xhigh" },
        state: strategyProjection.binding,
      },
    },
  ]);

  expect(records[0]?.strategy?.workers[0]?.runtime).toEqual({
    tag: "declared",
    model: "gpt-5.6-sol",
    reasoning_effort: "high",
  });
  expect(records[0]?.strategy?.binding?.state).toBe("resolved");
  expect(
    records[0]?.strategy?.binding?.state === "resolved"
      ? records[0].strategy.binding.effective.source
      : undefined,
  ).toBe("binding");
  expect(records[1]?.kind).toBe("strategy_binding");
  expect(records[1]?.strategy_binding?.state.state).toBe("resolved");
});

test("rejects malformed strategy runtime pairs and impossible effective sources", () => {
  const strategyRecord = {
    ...projectDecision,
    path: "projects/demo/investigations/sample/strategy/implementation.toml",
    scope: { project: "demo", investigation: "sample" },
    kind: "strategy",
    strategy: strategyProjection,
  };
  expect(() =>
    decodeRecords([
      {
        ...strategyRecord,
        strategy: {
          ...strategyProjection,
          workers: [{ ...strategyProjection.workers[0], reasoning_effort: undefined }],
        },
      },
    ]),
  ).toThrow("invalid worker runtime pair");
  expect(() =>
    decodeRecords([
      {
        ...strategyRecord,
        strategy: {
          ...strategyProjection,
          binding: {
            state: "resolved",
            effective: {
              model: "gpt-5.6-terra",
              reasoning_effort: "xhigh",
              source: "matrix",
            },
          },
        },
      },
    ]),
  ).toThrow("invalid resolved binding source");
  expect(() => decodeRecords([{ ...strategyRecord, strategy: null }])).toThrow(
    "invalid strategy projection",
  );
  expect(() =>
    decodeRecords([
      {
        ...strategyRecord,
        strategy: {
          ...strategyProjection,
          workers: [{ ...strategyProjection.workers[0], minimum_count: 0 }],
        },
      },
    ]),
  ).toThrow("invalid worker minimum count");
});
