import { afterAll, beforeAll, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { act } from "react";
import { cp, mkdir, mkdtemp, readFile, rm, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { App } from "./app";

type RunningHost = Readonly<{
  url: string;
  capability: string;
  root: string;
  stop: () => Promise<void>;
}>;

const casefileRoot = resolve(import.meta.dir, "../../..");
const sharedFixture = join(casefileRoot, "casefile-store/tests/fixtures/minimum");
const ticketPath = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";
const networkFetch = globalThis.fetch.bind(globalThis);

beforeAll(() => {
  GlobalRegistrator.register({ url: "http://casefile.test" });
  Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);
});

afterAll(async () => {
  Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT");
  await GlobalRegistrator.unregister();
});

test("navigates and reconciles governed work against the shared host fixture", async () => {
  const { createRoot } = await import("react-dom/client");
  const host = await startHost();
  const browserFetch = globalThis.fetch;
  const relationshipQueries: Array<unknown> = [];
  globalThis.fetch = Object.assign(
    async (input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
      if (typeof init?.body === "string") {
        const body: unknown = JSON.parse(init.body);
        if (isRelationshipQuery(body)) relationshipQueries.push(body);
      }
      const target =
        typeof input === "string" && input.startsWith("/") ? `${host.url}${input}` : input;
      return await networkFetch(target, init);
    },
    { preconnect: browserFetch.preconnect },
  );

  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<App />));
    await waitFor(() => container.textContent?.includes("Casefile projects") === true);

    expect(container.textContent).toContain("demo");
    expect(container.textContent).not.toContain("demo / null");

    await click(container, "Strategies");
    expect(container.textContent).toContain(
      "Select an investigation before inspecting its strategies",
    );

    await unlink(join(host.root, "projects/demo/investigations/sample/review/broken.md"));
    await click(container, "Refresh");
    await click(container, "demo");
    await waitFor(() => container.textContent?.includes("Investigations") === true);
    await click(container, "sample");
    await waitFor(() => container.textContent?.includes("Governed work") === true);
    expect(container.textContent).toContain("Minimum ticket");
    expect(container.textContent).toContain("Minimum epic");
    await click(container, "Boards");
    await waitFor(() => container.textContent?.includes("Delivery boards") === true);
    expect(container.textContent).toContain("Unknown");
    expect(container.textContent).toContain("Minimum ticket");
    await change(labelledInput(container, "Search records"), "no-ticket-list-match");
    await waitFor(() => container.textContent?.includes("Minimum ticket") === true);
    await click(container, "Minimum ticket");
    expect(container.textContent).toContain("HMD-011");
    await change(labelledInput(container, "Search records"), "");
    await click(container, "Tickets");
    await click(container, "HMD-011.md");

    await click(container, "Strategies");
    await waitFor(() => container.textContent?.includes("Phase roles and constraints") === true);
    expect(container.textContent).toContain(
      "Select a phase strategy or writer binding to inspect it",
    );
    expect(container.textContent).not.toContain("Legacy strategy without a typed projection");
    expect(container.textContent).toContain("implementation phase");
    expect(container.textContent).toContain("investigation phase");
    expect(container.textContent).toContain("review phase");
    expect(container.textContent).toContain("Implementation writer binding");

    await click(container, "implementation.toml");
    await waitFor(() => container.textContent?.includes("Declared role graph") === true);
    expect(container.textContent).toContain("Root orchestrator");
    expect(container.textContent).toContain("implementation-writer");
    expect(container.textContent).toContain("verification-reviewer");
    const writerNode = buttonWithLabel(container, "Inspect implementation-writer");
    await focusAndActivate(writerNode);
    expect(document.activeElement).toBe(writerNode);
    expect(writerNode.getAttribute("aria-pressed")).toBe("true");
    expect(container.textContent).toContain("gpt-5.6-terra / xhigh");
    expect(container.textContent).toContain("Effective sourcebinding");
    for (const fact of [
      "Count",
      "May spawn",
      "Profile",
      "Declared runtime",
      "Limits and requirements",
      "shared-ticket-storage",
      "Coordination",
      "Pipeline gates",
      "Disjoint write paths",
      "Immutable review commits",
    ])
      expect(container.textContent).toContain(fact);
    await click(container, "Source");
    expect(container.textContent).toContain('strategy_id = "casefile-implement-pipeline"');

    await click(container, "investigation.toml");
    expect(container.textContent).toContain(
      "No workers declared. This is a valid root-only strategy.",
    );

    await click(container, "review.toml");
    expect(container.textContent).toContain("Invalid strategy record");
    await click(container, "Diagnostics");
    expect(container.textContent).toContain("invalid_toml");
    await click(container, "Source");
    expect(container.textContent).toContain("workers = [");
    await writeFile(
      join(host.root, "projects/demo/investigations/sample/strategy/review.toml"),
      legacyReviewStrategy,
    );
    await click(container, "Refresh");
    await waitFor(
      () => container.textContent?.includes("Legacy strategy without a typed projection") === true,
    );

    await click(container, "bindings.toml");
    expect(container.textContent).toContain("Non-graph state");
    expect(container.textContent).toContain("Stateresolved");
    expect(container.textContent).toContain("Effective sourcebinding");
    await click(container, "Source");
    expect(container.textContent).toContain('model = "gpt-5.6-terra"');

    await change(labelledInput(container, "Search records"), "no-strategy-can-match-this");
    await waitFor(
      () =>
        container.textContent?.includes("No strategy records match the shared search filter") ===
        true,
    );
    await change(labelledInput(container, "Search records"), "");
    await waitFor(() => container.textContent?.includes("implementation.toml") === true);

    await click(container, "Files");
    await waitFor(() => container.textContent?.includes("Files by directory") === true);
    expect(container.textContent).toContain("HMD-D-002-project.md");
    expect(container.textContent).toContain("Project-level files remain visible");
    await click(container, "render.md");
    await click(container, "Rendered");
    expect(container.textContent).toContain("Rendered evidence");
    const unsafeLink = [...container.querySelectorAll("a")].find((link) =>
      link.textContent?.includes("bad link"),
    );
    expect(unsafeLink).toBeUndefined();
    expect(container.querySelector("script")).toBeNull();
    await click(container, "Source");
    expect(container.textContent).toContain("<script>bad()</script>");

    await click(container, "Tickets");
    await click(container, "HMD-011.md");
    await waitFor(() => relationshipQueries.length > 0);
    expect(relationshipQueries).toContainEqual({
      query: "relationships",
      identity: {
        scope: { project: "demo", investigation: "sample" },
        identity: "HMD-011",
      },
    });

    await click(container, "Rendered");
    expect(container.textContent).toContain("Rendered Markdown");
    expect(container.textContent).toContain("HMD-011");
    await click(container, "Source");
    expect(container.textContent).toContain("Exact source");
    expect(container.textContent).toContain("## Acceptance criteria");
    await click(container, "Overview");

    const shortIdentity = await networkFetch(`${host.url}/api/query`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ query: "relationships", identity: "HMD-011" }),
    });
    expect(shortIdentity.status).toBe(400);

    await change(labelledInput(container, "Title"), "Preserved browser draft");
    await click(container, "Preview changes");
    await waitFor(() => container.textContent?.includes("Exact diff") === true);
    expect(container.textContent).toContain("Exact diff");
    expect(container.textContent).toContain("Preserved browser draft");

    const canonical = join(host.root, ticketPath);
    const original = await readFile(canonical, "utf8");
    await writeFile(canonical, original.replace("Minimum ticket", "Concurrent title"));
    await change(labelledInput(container, "Write capability"), host.capability);
    await click(container, "Apply preview");
    await waitFor(() => container.textContent?.includes("Canonical content changed") === true);
    await waitFor(() => container.textContent?.includes("Concurrent title") === true);
    expect(container.textContent).toContain("Canonical content changed");
    expect(labelledInput(container, "Title").value).toBe("Preserved browser draft");
    expect(container.textContent).toContain("Concurrent title");

    await change(labelledInput(container, "Title"), "Reconciled browser title");
    await click(container, "Resume after reconciliation");
    await click(container, "Preview changes");
    await click(container, "Apply preview");
    expect(await readFile(canonical, "utf8")).toContain("Reconciled browser title");

    await click(container, "Files");
    await click(container, "main.toml");
    await change(labelledInput(container, "Title"), "Revised board");
    await click(container, "Preview changes");
    await waitFor(() => container.textContent?.includes('title = "Revised board"') === true);
    expect(container.textContent).toContain('title = "Revised board"');
  } finally {
    await act(async () => root.unmount());
    container.remove();
    globalThis.fetch = browserFetch;
    await host.stop();
  }
}, 120_000);

test("rejects stale board projections across scope changes and refresh outcomes", async () => {
  const { createRoot } = await import("react-dom/client");
  const host = await startHost({ nestedInvestigations: true, boardsForNested: true });
  const browserFetch = globalThis.fetch;
  let betaBoardGate = deferred();
  let delayBetaBoard = true;
  let failBoardRequests = false;
  globalThis.fetch = Object.assign(
    async (input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
      const body = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
      if (isBoardsQuery(body, "beta/shared")) {
        if (failBoardRequests) {
          return new Response(JSON.stringify({ error: "simulated board failure" }), {
            status: 503,
          });
        }
        if (delayBetaBoard) await betaBoardGate.promise;
      }
      const target =
        typeof input === "string" && input.startsWith("/") ? `${host.url}${input}` : input;
      return await networkFetch(target, init);
    },
    { preconnect: browserFetch.preconnect },
  );
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<App />));
    await waitFor(() => container.textContent?.includes("Casefile projects") === true);
    await click(container, "demo");
    await waitFor(() => container.textContent?.includes("Investigations") === true);
    await click(container, "alpha/shared");
    await waitFor(() => container.textContent?.includes("Alpha ticket") === true);
    await click(container, "Boards");
    await waitFor(() => container.textContent?.includes("Alpha board") === true);

    await clickNavigationItem(container, "beta/shared");
    expect(container.textContent).toContain("Loading canonical board projection");
    expect(container.textContent).not.toContain("Alpha board");
    betaBoardGate.resolve();
    await waitFor(() => container.textContent?.includes("Beta board") === true);

    delayBetaBoard = true;
    betaBoardGate = deferred();
    await click(container, "Refresh");
    expect(container.textContent).toContain("Loading canonical board projection");
    expect(container.textContent).not.toContain("Beta board");
    betaBoardGate.resolve();
    await waitFor(() => container.textContent?.includes("Beta board") === true);

    failBoardRequests = true;
    await click(container, "Refresh");
    await waitFor(() => container.textContent?.includes("Boards query failed") === true);
    expect(container.textContent).toContain("simulated board failure");
  } finally {
    await act(async () => root.unmount());
    container.remove();
    globalThis.fetch = browserFetch;
    await host.stop();
  }
}, 120_000);

test("loads a progress log and places cards in their recorded board columns", async () => {
  const { createRoot } = await import("react-dom/client");
  const host = await startHost({ progress: true });
  const browserFetch = globalThis.fetch;
  globalThis.fetch = Object.assign(
    async (input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
      const target =
        typeof input === "string" && input.startsWith("/") ? `${host.url}${input}` : input;
      return await networkFetch(target, init);
    },
    { preconnect: browserFetch.preconnect },
  );
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<App />));
    await waitFor(() => container.textContent?.includes("Casefile projects") === true);
    await click(container, "demo");
    await waitFor(() => container.textContent?.includes("Investigations") === true);
    await click(container, "sample");
    await waitFor(() => container.textContent?.includes("Governed work") === true);
    await click(container, "Boards");
    await waitFor(() => container.textContent?.includes("Progress") === true);

    const inProgress = container.querySelector('[aria-label="Progress: In progress"]');
    const blocked = container.querySelector('[aria-label="Progress: Blocked"]');
    expect(inProgress?.textContent).toContain("Minimum ticket");
    expect(inProgress?.textContent).toContain("HMD-011 · in_progress");
    expect(blocked?.textContent).toContain("Blocked ticket");
    expect(blocked?.textContent).toContain("HMD-012 · blocked");
  } finally {
    await act(async () => root.unmount());
    container.remove();
    globalThis.fetch = browserFetch;
    await host.stop();
  }
}, 120_000);

test("keeps nested investigations with the same leaf independently selectable", async () => {
  const { createRoot } = await import("react-dom/client");
  const host = await startHost({ nestedInvestigations: true });
  const browserFetch = globalThis.fetch;
  globalThis.fetch = Object.assign(
    async (input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
      const target =
        typeof input === "string" && input.startsWith("/") ? `${host.url}${input}` : input;
      return await networkFetch(target, init);
    },
    { preconnect: browserFetch.preconnect },
  );
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<App />));
    await waitFor(() => container.textContent?.includes("Casefile projects") === true);

    await click(container, "demo");
    await waitFor(() => container.textContent?.includes("Investigations") === true);
    expect(container.textContent).toContain("alpha/shared");
    expect(container.textContent).toContain("beta/shared");

    await click(container, "alpha/shared");
    await waitFor(() => container.textContent?.includes("Alpha ticket") === true);
    expect(container.textContent).not.toContain("Beta ticket");
    await click(container, "Strategies");
    expect(container.textContent).toContain(
      "This investigation has no recognized strategy or binding records",
    );

    await click(container, "Investigations");
    await click(container, "beta/shared");
    await waitFor(() => container.textContent?.includes("Beta ticket") === true);
    expect(container.textContent).not.toContain("Alpha ticket");
  } finally {
    await act(async () => root.unmount());
    container.remove();
    globalThis.fetch = browserFetch;
    await host.stop();
  }
}, 120_000);

const isBoardsQuery = (value: unknown, investigation: string): boolean => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  if (Reflect.get(value, "query") !== "boards") return false;
  const scope = Reflect.get(value, "scope");
  return (
    typeof scope === "object" &&
    scope !== null &&
    !Array.isArray(scope) &&
    Reflect.get(scope, "investigation") === investigation
  );
};

const isRelationshipQuery = (
  value: unknown,
): value is Readonly<{ query: "relationships"; identity: unknown }> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  return Reflect.get(value, "query") === "relationships";
};

type Deferred = Readonly<{ promise: Promise<void>; resolve: () => void }>;

const deferred = (): Deferred => {
  let resolve: (() => void) | undefined;
  const promise = new Promise<void>((resolvePromise) => {
    resolve = resolvePromise;
  });
  if (resolve === undefined) throw new Error("Deferred promise did not expose a resolver");
  return { promise, resolve };
};

const settle = async (milliseconds = 100): Promise<void> => {
  await new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
};

const waitFor = async (condition: () => boolean): Promise<void> => {
  const deadline = Date.now() + 5_000;
  while (!condition()) {
    if (Date.now() >= deadline) throw new Error("Timed out waiting for the workbench");
    await act(async () => settle(50));
  }
};

const click = async (container: HTMLElement, text: string): Promise<void> => {
  const button = [...container.querySelectorAll("button")].find((candidate) =>
    candidate.textContent?.includes(text),
  );
  if (button === undefined) throw new Error(`Button not found: ${text}`);
  await act(async () => {
    button.click();
    await settle();
  });
};

const clickNavigationItem = async (container: HTMLElement, value: string): Promise<void> => {
  const button = [...container.querySelectorAll("button")].find((candidate) =>
    candidate.textContent?.startsWith(value),
  );
  if (button === undefined) throw new Error(`Navigation item not found: ${value}`);
  await act(async () => {
    button.click();
    await settle(0);
  });
};

const buttonWithLabel = (container: HTMLElement, label: string): HTMLButtonElement => {
  const button = [...container.querySelectorAll("button")].find(
    (candidate) => candidate.getAttribute("aria-label") === label,
  );
  if (button === undefined) throw new Error(`Button not found: ${label}`);
  return button;
};

const focusAndActivate = async (button: HTMLButtonElement): Promise<void> => {
  await act(async () => {
    button.focus();
    button.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" }));
    button.click();
    button.dispatchEvent(new KeyboardEvent("keyup", { bubbles: true, key: "Enter" }));
    await settle();
  });
};

const labelledInput = (container: HTMLElement, label: string): HTMLInputElement => {
  const input = [...container.querySelectorAll("input")].find((candidate) => {
    if (candidate.getAttribute("aria-label") === label) return true;
    if ([...(candidate.labels ?? [])].some((item) => item.textContent?.trim() === label))
      return true;
    return candidate.closest("label")?.textContent?.trim().startsWith(label) === true;
  });
  if (input === undefined) throw new Error(`Input not found: ${label}`);
  return input;
};

const change = async (input: HTMLInputElement, value: string): Promise<void> => {
  const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  if (setValue === undefined) throw new Error("HTML input value setter is unavailable");
  await act(async () => {
    setValue.call(input, value);
    input.dispatchEvent(new InputEvent("input", { bubbles: true, data: value }));
    await settle();
  });
};

const startHost = async (
  options: Readonly<{
    nestedInvestigations?: boolean;
    boardsForNested?: boolean;
    progress?: boolean;
  }> = {},
): Promise<RunningHost> => {
  const temporary = await mkdtemp(join(tmpdir(), "casefile-browser-flow-"));
  const root = join(temporary, "root");
  await cp(sharedFixture, root, { recursive: true });
  await writeFile(join(root, "legacy.txt"), "legacy\n");
  await mkdir(join(root, "projects/demo/decision-log"), { recursive: true });
  await writeFile(
    join(root, "projects/demo/decision-log/HMD-D-002-project.md"),
    "# HMD-D-002 - Project decision\n\n## Status\n\naccepted\n\n## Decision\n\nProject scope.\n",
  );
  await writeFile(
    join(root, "projects/demo/investigations/sample/review/broken.md"),
    "not governed markdown\n",
  );
  await writeFile(
    join(root, "projects/demo/investigations/sample/evidence/render.md"),
    "# Rendered evidence\n\n- **emphasis**\n- `code`\n\n| Safe | Value |\n| --- | --- |\n| yes | 1 |\n\n[bad link](JaVaScRiPt:bad()) [safe link](https://example.com)\n\n<script>bad()</script>\n",
  );
  const strategyRoot = join(root, "projects/demo/investigations/sample/strategy");
  await writeFile(join(strategyRoot, "investigation.toml"), rootOnlyStrategy);
  await writeFile(join(strategyRoot, "implementation.toml"), implementationStrategy);
  await writeFile(join(strategyRoot, "review.toml"), "schema_version = 1\nworkers = [\n");
  await writeFile(join(strategyRoot, "bindings.toml"), writerBinding);
  await writeFile(
    join(root, "projects/demo/investigations/sample/boards/progress.toml"),
    options.progress === true
      ? 'schema_version = 1\nid = "HMD-progress"\ntitle = "Progress"\nstatus_source = "progress"\nfilter_kinds = ["ticket"]\n\n[[columns]]\nname = "In progress"\nstatuses = ["in_progress"]\n\n[[columns]]\nname = "Blocked"\nstatuses = ["blocked"]\n'
      : 'schema_version = 1\nid = "HMD-progress"\ntitle = "Delivery"\nstatus_source = "progress"\nfilter_kinds = ["ticket"]\n\n[[columns]]\nname = "Unknown"\nstatuses = ["unknown"]\n',
  );
  if (options.progress === true) {
    const ticket = await readFile(join(root, ticketPath), "utf8");
    await writeFile(
      join(root, "projects/demo/investigations/sample/tickets/accepted/HMD-012.md"),
      ticket
        .replaceAll("HMD-011", "HMD-012")
        .replace("Minimum ticket", "Blocked ticket")
        .replace("rank: 1", "rank: 2"),
    );
    await mkdir(join(root, "projects/demo/investigations/sample/progress"), { recursive: true });
    await writeFile(
      join(root, "projects/demo/investigations/sample/progress/log.toml"),
      'schema_version = 1\n\n[[entries]]\nid = "start"\nrecorded_at = "2026-07-26T10:00:00Z"\nrecorded_by = "test"\nticket_id = "HMD-011"\nkind = "transition"\nfrom = "unknown"\nto = "in_progress"\n\n[[entries]]\nid = "blocked"\nrecorded_at = "2026-07-26T10:01:00Z"\nrecorded_by = "test"\nticket_id = "HMD-012"\nkind = "transition"\nfrom = "unknown"\nto = "blocked"\n',
    );
  }
  if (options.nestedInvestigations === true) {
    await writeFile(
      join(root, "casefile.toml"),
      'schema_version = 1\n\n[projects.demo]\nprefix = "HMD"\ninvestigations = ["projects/demo/investigations/alpha/shared", "projects/demo/investigations/beta/shared"]\n',
    );
    const ticket = await readFile(join(root, ticketPath), "utf8");
    for (const [investigation, id, title] of [
      ["alpha/shared", "HMD-101", "Alpha ticket"],
      ["beta/shared", "HMD-102", "Beta ticket"],
    ]) {
      const path = join(
        root,
        "projects/demo/investigations",
        investigation,
        "tickets/accepted",
        `${id}.md`,
      );
      await mkdir(resolve(path, ".."), { recursive: true });
      await writeFile(
        path,
        ticket
          .replaceAll("HMD-011", id)
          .replace("Minimum ticket", title)
          .replace('investigation: "sample"', `investigation: \"${investigation}\"`),
      );
      if (options.boardsForNested === true) {
        const boardRoot = join(root, "projects/demo/investigations", investigation, "boards");
        await mkdir(boardRoot, { recursive: true });
        await writeFile(
          join(boardRoot, "main.toml"),
          `schema_version = 1\nid = "${id}-board"\ntitle = "${title.replace(" ticket", "")} board"\nfilter_statuses = ["accepted"]\nfilter_kinds = ["ticket"]\n\n[[columns]]\nname = "Accepted"\nstatuses = ["accepted"]\n`,
        );
      }
    }
  }
  for (const command of [
    ["git", "init", "-q"],
    ["git", "config", "user.email", "casefile@example.test"],
    ["git", "config", "user.name", "Casefile Test"],
    ["git", "add", "."],
    ["git", "commit", "-qm", "fixture"],
  ])
    await run(command, root);

  const process = Bun.spawn({
    cmd: [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "casefile-cli",
      "--",
      "--root",
      root,
      "serve",
      "--index",
      join(temporary, "index.sqlite"),
      "--write",
    ],
    cwd: casefileRoot,
    stdout: "pipe",
    stderr: "inherit",
  });
  const lines = await readLines(process.stdout, 4);
  const address = lines[0]?.replace("Casefile server: ", "");
  const capability = lines[3]?.replace("Casefile write capability: ", "");
  if (address === undefined || capability === undefined) {
    process.kill();
    await process.exited;
    await rm(temporary, { force: true, recursive: true });
    throw new Error(`Unexpected Casefile launch output: ${lines.join(" | ")}`);
  }
  return {
    url: address,
    capability,
    root,
    stop: async () => {
      process.kill();
      await process.exited;
      await rm(temporary, { force: true, recursive: true });
    },
  };
};

const rootOnlyStrategy = `schema_version = 1
strategy_id = "casefile-investigate-solo"
phase = "investigation"
adapter = "codex"

[orchestrator]
binding = "root"

[limits]
max_concurrent_subagents = 1
max_depth = 0

[requirements]
capabilities = []

[coordination]
batch_when_capacity_exceeded = false
candidate_review_before_ticket = true
shared_ticket_storage_required = true
`;

const implementationStrategy = `schema_version = 1
strategy_id = "casefile-implement-pipeline"
phase = "implementation"
adapter = "codex"

[orchestrator]
binding = "root"

[limits]
max_concurrent_subagents = 4
max_depth = 2

[requirements]
capabilities = ["subagents", "shared-ticket-storage"]

[[workers]]
role = "implementation-writer"
platform_profile = "casefile-writer"
model = "gpt-5.6-sol"
reasoning = "high"
minimum_count = 1
maximum_count = 2
can_spawn_subagents = false

[[workers]]
role = "verification-reviewer"
platform_profile = "casefile-reviewer"
minimum_count = 1
maximum_count = 1
can_spawn_subagents = true

[coordination]
batch_when_capacity_exceeded = true
candidate_review_before_ticket = true
shared_ticket_storage_required = true

[coordination.pipeline]
maximum_active_tickets = 2
look_ahead_read_only = true
require_dependency_independence = true
require_disjoint_write_paths = true
immutable_review_commits = true
corrections_preempt_forward_work = true
`;

const writerBinding = `schema_version = 1
adapter = "codex"
role = "implementation-writer"
model = "gpt-5.6-terra"
reasoning_effort = "xhigh"

[resolution]
mode = "catalog_id"
value = "gpt-5.6-terra/xhigh"
`;

const legacyReviewStrategy = `schema_version = 1
strategy_id = "casefile-review-atomic"
phase = "review"
adapter = "codex"
`;

const run = async (command: ReadonlyArray<string>, cwd: string): Promise<void> => {
  const process = Bun.spawn({ cmd: [...command], cwd, stdout: "pipe", stderr: "pipe" });
  const [status, stdout, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);
  if (status !== 0) throw new Error(`${command.join(" ")} failed:\n${stdout}${stderr}`);
};

const readLines = async (
  stream: ReadableStream<Uint8Array>,
  count: number,
): Promise<ReadonlyArray<string>> => {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (buffer.split("\n").filter((line) => line.length > 0).length < count) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
  }
  reader.releaseLock();
  return buffer
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
};
