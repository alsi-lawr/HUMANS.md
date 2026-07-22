import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { Sidebar } from "./navigation/sidebar";
import { ChangeControls } from "./record-detail/change-review";
import { WorkItemEditor } from "./record-detail/work-item-editor";
import { type WorkItemDraft } from "./model";
import { type StrategyGraph as Graph } from "./strategy/graph-model";
import { StrategyGraph } from "./strategy/strategy-graph";

const workItem: WorkItemDraft = {
  id: "HMD-011",
  title: "Minimum ticket",
  project: "demo",
  investigation: "sample",
  status: "accepted",
  reported_by_role: "inspector",
  reported_by_agent: "agent",
  source_commit: "abc",
  created_at: "2026-07-18T10:00:00Z",
  updated_at: "2026-07-18T10:00:00Z",
  confidence: "high",
  decision_refs: [],
  related_tickets: [],
  supersedes: [],
  superseded_by: [],
  rank: 1,
  requirement_and_evidence: "Required.",
  impact: "Impact.",
  resolution_boundary: "Boundary.",
  acceptance_criteria: "Criteria.",
  verification: "Tests.",
  relationships_and_duplicate_analysis: "None.",
  review_and_disposition_history: "Accepted.",
};
const noAction = (): void => {};

test("renders project navigation without a null investigation label", () => {
  const html = renderToStaticMarkup(
    <Sidebar
      tab="projects"
      projects={[{ name: "demo", investigations: 1, tickets: 2 }]}
      investigations={[]}
      project={undefined}
      investigation={undefined}
      diagnostics={[]}
      onTab={noAction}
      onProject={noAction}
      onInvestigation={noAction}
    />,
  );

  expect(html).toContain("demo");
  expect(html).not.toContain("null");
});

test("blocks mutation controls while a preserved draft needs reconciliation", () => {
  const html = renderToStaticMarkup(
    <ChangeControls
      capability="capability"
      status="conflict"
      message="Canonical content changed."
      onCapability={noAction}
      onPreview={noAction}
      onApply={noAction}
      onReconcile={noAction}
      preview={undefined}
    />,
  );

  expect(html).toContain("Resume after reconciliation");
  expect(html).toContain("disabled");
});

test("keeps deferred status and rank workflows out of the work-item form", () => {
  const html = renderToStaticMarkup(
    <WorkItemEditor kind="ticket" item={workItem} onChange={noAction} />,
  );

  expect(html).not.toContain(">Status<");
  expect(html).not.toContain(">Rank<");
});

const strategyGraph: Graph = {
  phase: { tag: "known", phase: "implementation" },
  root: { id: "root", kind: "root", binding: "root" },
  workers: [
    {
      id: "worker:implementation-writer:1",
      kind: "worker",
      worker: {
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
      effective: {
        model: "gpt-5.6-terra",
        reasoning_effort: "xhigh",
        source: "binding",
      },
    },
  ],
  edges: [{ source: "root", target: "worker:implementation-writer:1" }],
  projection: {
    root_binding: "root",
    limits: { max_concurrent_subagents: 2, max_depth: 1 },
    requirements: { capabilities: ["subagents"] },
    workers: [],
    coordination: {
      batch_when_capacity_exceeded: true,
      candidate_review_before_ticket: false,
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
  },
};

test("renders graph nodes as ordered focusable buttons with decorative root connectors", () => {
  const html = renderToStaticMarkup(<StrategyGraph graph={strategyGraph} />);

  const root = html.indexOf('aria-label="Inspect Root orchestrator"');
  const writer = html.indexOf('aria-label="Inspect implementation-writer"');
  expect(root).toBeGreaterThan(-1);
  expect(writer).toBeGreaterThan(root);
  expect(html).toContain('aria-pressed="true"');
  expect(html).toContain('aria-hidden="true"');
  expect(html).toContain("focus-visible:ring-2");
  expect(html).toContain("Limits and requirements");
  expect(html).toContain("Coordination");
  expect(html).toContain("Pipeline gates");
  expect(html).toContain("Disjoint write paths");
  expect(html).not.toContain("<canvas");
  expect(html).not.toContain("<svg");
});

test("renders zero-worker strategy as a valid root-only graph", () => {
  const html = renderToStaticMarkup(
    <StrategyGraph
      graph={{
        ...strategyGraph,
        workers: [],
        edges: [],
        projection: { ...strategyGraph.projection, workers: [] },
      }}
    />,
  );

  expect(html).toContain("No workers declared. This is a valid root-only strategy.");
  expect(html).toContain('aria-label="Inspect Root orchestrator"');
});
