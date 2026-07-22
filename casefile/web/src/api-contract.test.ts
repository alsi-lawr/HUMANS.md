import { expect, test } from "bun:test";
import { decodeCurrent, decodeHostFailure, decodeRecords } from "./api-contract";

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

test("preserves the host stale-revision failure code", () => {
  expect(decodeHostFailure({ error: "stale store revision", code: "stale_revision" }, 409)).toEqual(
    { message: "stale store revision", code: "stale_revision" },
  );
});
