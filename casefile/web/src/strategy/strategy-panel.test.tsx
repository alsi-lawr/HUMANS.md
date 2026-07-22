import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { type Record } from "../model";
import { StrategyPanel } from "./strategy-panel";

const noAction = (): void => {};
const baseRecord = (kind: "strategy" | "strategy_binding"): Record => ({
  path: `projects/demo/investigations/sample/strategy/${kind === "strategy" ? "review" : "bindings"}.toml`,
  scope: { project: "demo", investigation: "sample" },
  classification: "governed",
  kind,
  identity: undefined,
  title: kind,
  content: "exact source",
  rendered_markdown: undefined,
  work_item: undefined,
  board: undefined,
  strategy: undefined,
  strategy_binding: undefined,
});

const renderPanel = (
  records: ReadonlyArray<Record>,
  selectedRecord: Record | undefined,
  search = "",
): string =>
  renderToStaticMarkup(
    <StrategyPanel
      investigation="sample"
      records={records}
      selectedRecord={selectedRecord}
      selectedPath={selectedRecord?.path}
      diagnostics={[]}
      search={search}
      onSelect={noAction}
    />,
  );

test("presents legacy strategy and unresolved binding as explicit non-graph states", () => {
  const legacy = baseRecord("strategy");
  const unresolved: Record = {
    ...baseRecord("strategy_binding"),
    strategy_binding: {
      adapter: "codex",
      role: "implementation-writer",
      model: "missing-model",
      reasoning_effort: "high",
      resolution: { mode: "catalog_id", value: "missing-model/high" },
      state: { state: "unresolved" },
    },
  };

  const legacyHtml = renderPanel([legacy], legacy);
  const unresolvedHtml = renderPanel([unresolved], unresolved);

  expect(legacyHtml).toContain("Legacy strategy without a typed projection");
  expect(legacyHtml).not.toContain("Declared role graph");
  expect(unresolvedHtml).toContain("The selected runtime cannot be resolved");
  expect(unresolvedHtml).not.toContain("Effective runtime");
  expect(unresolvedHtml).not.toContain("<canvas");
});

test("distinguishes filtered strategy results from a missing investigation strategy", () => {
  const missing = renderPanel([], undefined);
  const filtered = renderPanel([], undefined, "writer");

  expect(missing).toContain("no recognized strategy or binding records");
  expect(filtered).toContain("No strategy records match the shared search filter");
});

test("requires an investigation before exposing strategy records", () => {
  const html = renderToStaticMarkup(
    <StrategyPanel
      investigation={undefined}
      records={[]}
      selectedRecord={undefined}
      selectedPath={undefined}
      diagnostics={[]}
      search=""
      onSelect={noAction}
    />,
  );

  expect(html).toContain("Select an investigation before inspecting its strategies");
});
