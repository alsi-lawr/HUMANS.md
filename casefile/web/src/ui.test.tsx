import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { Sidebar } from "./navigation/sidebar";
import { ChangeControls } from "./record-detail/change-review";
import { WorkItemEditor } from "./record-detail/work-item-editor";
import { type WorkItemDraft } from "./model";

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

test("renders project scopes without a null investigation label", () => {
  const html = renderToStaticMarkup(
    <Sidebar
      scopes={[{ project: "demo", investigation: undefined }]}
      selected={undefined}
      diagnostics={[]}
      onSelect={noAction}
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
