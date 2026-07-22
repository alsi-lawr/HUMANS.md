import { describe, expect, test } from "bun:test";
import { type Record, type StrategyProjection } from "../model";
import { projectStrategyGraph } from "./graph-model";

const resolvedProjection: StrategyProjection = {
  root_binding: "root",
  limits: { max_concurrent_subagents: 4, max_depth: 2 },
  requirements: { capabilities: ["subagents", "shared-storage"] },
  workers: [
    {
      role: "implementation-writer",
      platform_profile: "casefile-writer",
      runtime: {
        tag: "declared",
        model: "gpt-5.6-sol",
        reasoning_effort: "high",
      },
      minimum_count: 1,
      maximum_count: 2,
      can_spawn_subagents: false,
    },
    {
      role: "verification-reviewer",
      platform_profile: "casefile-reviewer",
      runtime: { tag: "unspecified" },
      minimum_count: 1,
      maximum_count: 1,
      can_spawn_subagents: true,
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

const strategyRecord = (phase: string, projection: StrategyProjection): Record => ({
  path: `projects/demo/investigations/sample/strategy/${phase}.toml`,
  scope: { project: "demo", investigation: "sample" },
  classification: "governed",
  kind: "strategy",
  identity: undefined,
  title: `${phase} strategy`,
  content: "exact source",
  rendered_markdown: undefined,
  work_item: undefined,
  board: undefined,
  strategy: projection,
  strategy_binding: undefined,
});

const graphFor = (record: Record) => {
  const state = projectStrategyGraph(record);
  if (state.tag !== "graph") throw new Error(`Expected graph, received ${state.tag}`);
  return state.graph;
};

describe("canonical strategy graph projection", () => {
  test("connects root directly to every declared worker in deterministic declaration order", () => {
    const graph = graphFor(strategyRecord("implementation", resolvedProjection));

    expect(graph.workers.map((worker) => worker.worker.role)).toEqual([
      "implementation-writer",
      "verification-reviewer",
    ]);
    expect(graph.edges).toEqual([
      { source: "root", target: "worker:implementation-writer:1" },
      { source: "root", target: "worker:verification-reviewer:1" },
    ]);
    expect(graph.edges.some((edge) => edge.source !== "root")).toBeFalse();
    expect(graph.workers[0]?.effective).toEqual({
      model: "gpt-5.6-terra",
      reasoning_effort: "xhigh",
      source: "binding",
    });
    expect(graph.workers[1]?.effective).toBeUndefined();
  });

  test("never overlays the implementation binding on another phase", () => {
    const graph = graphFor(strategyRecord("review", resolvedProjection));

    expect(graph.workers.every((worker) => worker.effective === undefined)).toBeTrue();
  });

  test("shows no successful pair for unresolved or ambiguously declared writers", () => {
    const unresolved = graphFor(
      strategyRecord("implementation", {
        ...resolvedProjection,
        binding: { state: "unresolved" },
      }),
    );
    expect(unresolved.workers[0]?.effective).toBeUndefined();

    const duplicate = graphFor(
      strategyRecord("implementation", {
        ...resolvedProjection,
        workers: [resolvedProjection.workers[0], resolvedProjection.workers[0]].filter(
          (worker) => worker !== undefined,
        ),
      }),
    );
    expect(duplicate.workers.every((worker) => worker.effective === undefined)).toBeTrue();
  });

  test("preserves the historical matrix default only when the host marks the binding absent", () => {
    const graph = graphFor(
      strategyRecord("implementation", {
        ...resolvedProjection,
        binding: {
          state: "absent",
          effective: {
            model: "gpt-5.6-sol",
            reasoning_effort: "high",
            source: "matrix",
          },
        },
      }),
    );

    expect(graph.workers[0]?.effective).toEqual({
      model: "gpt-5.6-sol",
      reasoning_effort: "high",
      source: "matrix",
    });
  });

  test("keeps a valid zero-worker projection as a root-only graph", () => {
    const graph = graphFor(
      strategyRecord("investigation", { ...resolvedProjection, workers: [], binding: null }),
    );

    expect(graph.root).toEqual({ id: "root", kind: "root", binding: "root" });
    expect(graph.workers).toEqual([]);
    expect(graph.edges).toEqual([]);
  });

  test("distinguishes invalid records from legacy projections", () => {
    const record = strategyRecord("review", resolvedProjection);

    expect(
      projectStrategyGraph({ ...record, classification: "invalid", strategy: undefined }),
    ).toEqual({ tag: "invalid" });
    expect(projectStrategyGraph({ ...record, strategy: undefined })).toEqual({ tag: "legacy" });
  });
});
