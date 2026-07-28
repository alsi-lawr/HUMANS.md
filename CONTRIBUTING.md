# Contributing

HUMANS.md is a multi-plugin source repository. Contributions must preserve the authority boundaries
in the closest `AGENTS.md`, keep generated packages reproducible, and avoid coupling independently
installable plugins.

## Repository layout

- `humans-md/` owns the standing `AGENTS.md` / `CLAUDE.md` contract lifecycle.
- `casefile/` owns the governed workflow, Rust CLI/TUI/server/store, browser workbench, skills,
  strategies, roles, and optional Codex integration.
- `coding/` owns reusable coding, Git, README, and skill-generation guidance.
- `scripts/` owns shared source, package, and marketplace validation.
- `packaging/marketplace/` contains inputs copied into the generated marketplace repository.

Skills, role prompts, schemas, fixtures, and model instruction files are product inputs rather than
developer documentation. User and reference documentation belongs in the
[project wiki](https://github.com/alsi-lawr/HUMANS.md/wiki).

## Development environment

Enter the pinned environment and install the lock-resolved browser dependencies once:

```sh
nix develop
(cd casefile/web && bun install)
```

The shell supplies Rust, Python, Node.js, Bun, workflow, GitHub, Docker-client, and PTY tools.
Docker itself remains an external service.

Format Markdown and TypeScript with Prettier and Rust with rustfmt:

```sh
scripts/format-source.sh --write
```

Run the same source checks used by CI:

```sh
scripts/format-source.sh --check
python scripts/strip-non-ascii.py --check .
python scripts/test-all.py

(cd casefile && cargo fmt --check)
(cd casefile && cargo clippy --workspace --all-targets -- -D warnings)
(cd casefile && cargo test --workspace)

(cd casefile/web && bun run typecheck)
(cd casefile/web && bun run test)
(cd casefile/web && bun run build)

python scripts/validate-package-roots.py
python casefile/scripts/validate-casefile.py --source casefile
python coding/scripts/validate-skill.py --all --root coding
python scripts/package-plugin.py build --manifest humans-md/packaging/plugin.toml
python scripts/package-plugin.py check --manifest humans-md/packaging/plugin.toml
python scripts/package-plugin.py build --manifest coding/packaging/plugin.toml
python scripts/package-plugin.py check --manifest coding/packaging/plugin.toml
```

Those ordinary source checks deliberately exclude the Casefile release package. A Casefile package
requires the reviewed six-host artifact described under
[Packages and generated assets](#packages-and-generated-assets); it never compiles a substitute
binary locally.

`nix flake check` evaluates the flake. CI can also be replayed from inside the shell with a cached
runner image:

```sh
act pull_request -j validate --pull=false \
  -P ubuntu-latest=catthehacker/ubuntu:act-latest
```

## Casefile development

The Casefile Rust provider owns canonical capability, snapshot, typed query, preview, and apply
semantics over one Store baseline. The SQLite adapter is a disposable derived index retained by the
loopback host only for all-record and relationship projections not covered by provider operations.
The host fixes one planning root at launch and embeds the tracked browser build; it transports
provider previews and results, and the browser never parses or writes planning files directly.

### Native MCP package boundary

The generated Codex and Claude Casefile packages contain byte-identical copies of the complete
supported executable matrix and its source-bound SHA-256 manifest. They contain no `.mcp.json`,
package-local Cargo workspace, or source launcher. Host-specific receipt-backed setup requires one
explicit absolute planning root, verifies the complete matrix and selected artifact, atomically
installs the matching executable in a stable versioned user path, probes the exact identity,
protocol, capabilities, and 12-tool stdio surface, then registers
`casefile mcp-package --planning-root <absolute-root>` directly. Runtime startup needs no Cargo,
Rust, Python, Node, network, or `PATH` lookup.

At the 0.4.0 candidate boundary, Codex 0.145.0 and Claude Code 2.1.217 expose no direct marketplace
OS/architecture selector. Both packages therefore carry all six binaries. Claude npm indirection was
deliberately rejected for this release to keep one symmetric artifact, provenance, and validation
path. These are dated compatibility facts, not permanent host limitations.

MCP is a thin transport over the canonical typed Provider: capability and snapshot share one Store
baseline, queries are Store-derived, and every governed mutation is exact-preview preview/apply with
target-level conflict detection. The complete immutable Provider preview must be displayed and
explicitly approved by a human before the exact unchanged preview is applied in the same live
Provider session. Technical capability is not consent.

Activated current-v1 Stores operate in place without conversion. Unactivated, invalid, unsupported,
and early legacy activation layouts remain read-only or unsupported with actionable diagnostics; do
not automatically activate, repair, upgrade, or infer history. The CLI remains the human and
recovery adapter, including malformed-progress repair that is intentionally absent from MCP. The
project-map validator, package and CI validators, model-drift checks, and Codex writer catalog
resolver retain their distinct responsibilities.

The Provider owns governed strategy transitions and writer-binding replacement. The only ad-hoc
strategy path is `casefile scratch-strategy`, a bounded CLI-only operation requiring an explicit
target outside the configured Store; it creates no Provider-visible governed record. The superseded
`provision-delivery-board.py`, `transition-ticket-progress.py`, and `switch-strategy.py` workflow
scripts are retired. Do not restore them or present scratch output as configured-Store authority.

### Ticket progress and consolidation

`progress/log.toml` is an investigation-scoped canonical record. Ticket disposition remains the
review decision; delivery progress is derived separately. Provider progress preview/apply is the
governed progress path. Do not edit a progress log, ticket frontmatter, or a second progress file to
migrate, repair, or update ticket delivery state.

For a selected active investigation, first validate only that scope. Call the fixed-root MCP
provider's `casefile_preview_progress`, save and display the complete preview outside the planning
root, obtain explicit human approval, then pass that exact preview unchanged to
`casefile_apply_progress` in the same provider session:

```sh
casefile --root "$CASEFILE_ROOT" check --require-activation --investigation "$INVESTIGATION"
# MCP: casefile_preview_progress {"operation":{"operation":"bootstrap","investigation":"..."}}
# Show/save the complete result; after explicit approval, MCP: casefile_apply_progress {"preview":...}
```

For local recovery when MCP is unavailable, `progress-session --request <operation.json>` keeps the
provider alive, prints the complete preview, and applies only after the operator types its exact
preview ID. The analogous `default-delivery-board-session`, `strategy-transition-session`, and
`writer-binding-session` commands preserve the same one-session opaque-preview gate. Their prompt,
not provider write capability, is the approval boundary.

The same provider operation owns ordinary transitions and typed notes. Supply the ticket's currently
derived state in `--from`, use a stable operation ID, and preserve the generated preview for the
apply or exact retry. Notes use category `deviation` or `quirk` and never change state. Do not
backfill stages that were not captured when they occurred; record a note instead. The CLI
recovery-only `progress-repair-preview` and `progress-repair-apply` commands accept an exact
caller-supplied complete log only for malformed-log repair. No preview may live inside the planning
root.

Bootstrap creates an absent empty `progress/log.toml`; accepted tickets without entries derive as
`unknown`. It never writes invented initialization history and an existing valid log is a no-op. For
a malformed log, require exact caller-supplied replacement content. Before its non-mutating
`replace` preview, copy the original bytes under a SHA-256 content-hash filename in `$TASK_SCRATCH`,
retain that backup through closeout, and report it. The canonical writer owns atomic replacement and
post-write validation; on stale revision or failure, make no ad-hoc repair.

`casefile-consolidate` is the explicit-only skill for this narrow migration/repair work. It is not a
legacy-layout converter, historical-progress inference tool, generic validator, or lifecycle skill.
Its packaged source is shared by the Codex and Claude Casefile packages through
`casefile/packaging/plugin.toml`; update its validation inventory and verification suite whenever
the skill boundary changes. User-facing migration and repair guidance is in the
[Casefile ticket progress wiki page](https://github.com/alsi-lawr/HUMANS.md/wiki/Casefile-Ticket-Progress).

After a Casefile is newly activated, or after an explicit consolidation reaches a successful
progress-log outcome, provision its canonical delivery board through the separate provider
operation:

```sh
# MCP: casefile_preview_default_delivery_board {"investigation":"..."}
# Display/save the complete preview. After explicit approval:
# MCP: casefile_apply_default_delivery_board {"preview":...}
```

The provider selects the exact activated project's prefix and mapped investigation directory name
only to construct `<PREFIX>-<INVESTIGATION-DIRECTORY>-delivery`. This keeps board identities unique
when one project has multiple investigations with distinct final directory names. Before preview and
apply, the provider preflights every activated mapping and refuses if the derived identity maps to
anything other than exactly one investigation. The Rust `preview` and `apply` operations remain
authoritative for board rendering, path checks, validation, target revisions, and the one-file
atomic write. Provider preview compares the proposed diagnostics with its exact pre-write baseline:
unchanged baseline diagnostics remain visible to scan, check, and query but do not block the write;
an introduced or changed diagnostic does. Apply refuses a changed board target but does not reject
unrelated Store changes. The operation creates an absent `boards/delivery.toml`, reports exact
canonical content as a no-op, and refuses a different target without replacement. It never reads or
mutates progress or tickets, and consolidation keeps the progress and board writes sequential rather
than transactional.

### Strategies and writer bindings

Each investigation keeps the selected phase matrices in `strategy/<phase>.toml`. A complete matrix
uses schema version 1 and declares its identity, phase, adapter, root binding, limits, required
capabilities, worker rows, and coordination rules. Pipeline gates are an optional coordination
table. The request-receiving orchestrator is always `root`; a matrix describes delegated roles but
does not select the root model or effort. Validate the complete preset before copying it into the
Casefile and leave that selected source unchanged.

`strategy/bindings.toml` is a separate schema-version-1 overlay for the Casefile-wide Codex
implementation writer. It declares `adapter`, the literal `role = "implementation-writer"`, `model`,
`reasoning_effort`, and a `[resolution]` table recording the adapter resolution mode and value. It
never changes reviewer, verifier, look-ahead, or root bindings. The Rust parser is the single schema
authority and projects these client-visible states:

- `absent`: no overlay exists and the single matrix writer pair is effective;
- `pending`: a valid overlay exists before an implementation strategy is selected;
- `resolved`: the overlay adapter matches a selected implementation matrix with exactly one writer;
- `unresolved`: the overlay or matrix cannot identify one applicable effective writer; and
- `invalid`: the binding source failed canonical validation.

Historical Casefiles without `bindings.toml` remain valid and use the matrix writer pair. A present
invalid or unresolved overlay never falls back silently. The TUI, server, and browser consume the
Rust-owned typed projection and diagnostics; do not add another TOML parser to a client.

### Codex offer and spawn resolution

`casefile/adapters/codex/scripts/resolve-writer-binding.py` reads visible models and reasoning
efforts through Codex app-server's stable `model/list` method, then validates runtime selectors from
the configured receipt-owned Casefile catalog. It offers only pairs that match the selected
multi-agent runtime and have a verified packaged resolution for both implementation strategies. V1
requires an exact generated named profile for the pair. V2 requires each strategy's runtime wrapper,
a positive fork context, and explicit model and effort overrides at spawn.

Sol/high is a recommendation, not a default. The offer reports whether it is available and always
requires an explicit exact selection. If it is unavailable, present the remaining offered pairs
without recommending a substitute. Before ticket-batch, pipeline, resumed, or correction work,
resolve the canonical projection and revalidate the pair against a fresh offer. Stop before
delegation for pending, unresolved, invalid, or newly unavailable state; obtain explicit reselection
while implementation is inactive.

Binding replacement remains a root-authorized decision. Use the resolver's `select` command only to
materialize the confirmed typed request, pass that request to the Provider's writer-binding preview,
display the complete immutable preview, obtain explicit human approval, and apply that exact
unchanged preview through the Provider. The Provider derives inactivity from one valid canonical
progress log: accepted tickets may be `unknown` or `complete`, active non-complete stages refuse the
replacement, and missing, malformed, or conflicting progress fails closed. Provider capability is
not approval. Git history is the only history boundary: do not add an archive, journal, second state
file, or client-side write path.

### Workbench development

Build and inspect the browser workbench against a planning root with:

```sh
cd casefile/web
bun run typecheck
bun run test
bun run build
cd ..
cargo run -p casefile-cli -- --root ~/dev/agent-planning serve --write
```

Read-only browsing works immediately. Supply the printed, non-persisted write capability only when
testing governed ticket, epic, or board replacement.

The TUI opens the investigation-scoped Boards view with key `6`; the browser exposes the same view
through its Boards navigation item. Both clients consume the Store-derived board projection and
leave ticket progress read-only. New and explicitly consolidated Casefiles receive this canonical
delivery-progress board through the workflow wrapper above:

```toml
schema_version = 1
id = "HMD-sample-delivery"
title = "Delivery"
status_source = "progress"
filter_kinds = ["ticket"]

[[columns]]
name = "Unknown"
statuses = ["unknown"]

[[columns]]
name = "In progress"
statuses = ["in_progress"]

[[columns]]
name = "In review"
statuses = ["in_review"]

[[columns]]
name = "Verifying"
statuses = ["verifying"]

[[columns]]
name = "Blocked"
statuses = ["blocked"]

[[columns]]
name = "Complete"
statuses = ["complete"]
```

Omitting `status_source` preserves the existing disposition board. Progress boards include accepted
tickets only; their columns and `filter_statuses` are interpreted against delivery progress. Keep
missing, ambiguous, invalid, stale, loading, failure, and empty states visible, and resolve card
details against the unfiltered investigation records even when browser search is active.

The TUI's `Strategies` view is investigation-scoped and available through key `5` or the `t` view
cycle. It shows matrices and `bindings.toml` with Overview, exact Source, and Diagnostics panes.
Strategy records remain read-only even when other governed record editing is available.

The browser's `Strategies` panel receives the same typed projection from the server. Its graph has
one canonical root node and one node per declared worker, in declaration order. Draw only
root-to-worker connectors: the matrix has no worker dependency or live-execution relation to render.
Keep root-only, legacy, invalid, pending, unresolved, and empty states explicit rather than
inventing a graph. Node controls must remain native keyboard-operable buttons with visible focus,
pressed state, labelled regions, and a polite detail announcement. Selection reveals declared and
effective runtime facts plus limits, requirements, coordination, and pipeline constraints; it does
not edit a strategy or binding.

The Codex adapter owns the selected Casefile model catalog and multi-agent runtime. Setup defaults
to V1; V2 requires Codex 0.145.0 or newer. Codex setup confirms Sol, Terra, Luna, and Spark through
app-server `model/list` in a private configuration-free home. Fresh setup and upgrade both use the
new Codex-owned cache from that request to construct the Casefile catalog; upgrades use the active
receipt-owned catalog only to validate selectors before replacement. File-auth homes are refreshed
by Codex itself before a temporary credential copy is made, while environment API-key auth can be
used directly. Keyring-only homes fail closed because Codex keyring identity is home-derived and is
not assumed to cross into the private home. The temporary home is removed and selected configuration
must remain unchanged. Casefile never invokes a debug model command or directly writes, configures,
packages, or distributes Codex's selected-home cache; Codex may refresh its own cache while
refreshing authentication. The Claude adapter supplies workflow skills, matrices, role agents, and a
separate Casefile MCP binding transaction without owning the standing contract. Neither adapter
removes the shared marketplace or sibling plugins.

The source CLI is optional infrastructure, not part of installed plugin setup:

```sh
cargo build --manifest-path casefile/Cargo.toml --release -p casefile-cli
casefile/target/release/casefile --root "$CASEFILE_ROOT" check --require-activation
```

### Focused and release-candidate verification

During feature work, run the narrowest checks for the owned surface: the relevant Rust package
tests, focused Python setup/binding tests, or browser format/type/test/build commands. Rebuild and
review embedded browser assets when the web source changes. Do not repeatedly spend the full
workspace, package, or authenticated smoke gate on a local correction.

For a release candidate, pin one exact source commit and run the complete commands in
[Development environment](#development-environment), followed by the generated-marketplace checks
below. Inspect all six generated vendor metadata manifests, both regenerated catalogs, generated
Codex and Claude Casefile validation, and embedded-asset cleanliness. Run authenticated,
configuration-isolated runtime smoke only when the release ticket requires it, and retain sanitized
evidence without credentials, caches, or raw session logs.

## Packages and generated assets

The three `*/packaging/plugin.toml` manifests are the package-version authority. Package metadata is
rendered from those manifests; do not hard-code a release version in generated templates or tests.
Casefile package generation additionally requires `--casefile-artifact-root` and the exact reviewed
source commit. `.github/workflows/build-casefile-binaries.yml` is the build-only six-host matrix;
publication downloads one explicitly identified prior run, verifies version, source, manifest
digest, size, format, and hashes, and never recompiles release binaries.

The Casefile browser build under `casefile/casefile-server/web/` is tracked and embedded by Rust.
After changing `casefile/web/`, rebuild it and verify that the committed assets are intentional.

To validate the full generated marketplace tree:

```sh
export SOURCE_COMMIT="$(git rev-parse HEAD)"
export CASEFILE_ARTIFACT_ROOT="/absolute/path/to/reviewed-handoff/casefile-runtime"
export VERSION="$(python -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("casefile/packaging/plugin.toml").read_text(encoding="ascii"))["version"])')"

python scripts/casefile_artifacts.py verify \
  --artifact-root "$CASEFILE_ARTIFACT_ROOT" \
  --version "$VERSION" \
  --source-commit "$SOURCE_COMMIT"
python scripts/package-plugin.py build --all \
  --casefile-artifact-root "$CASEFILE_ARTIFACT_ROOT" \
  --casefile-source-commit "$SOURCE_COMMIT"
python scripts/build-marketplace-catalog.py
cp -R packaging/marketplace/. build/marketplace/
cp LICENSE build/marketplace/LICENSE
python scripts/package-plugin.py check --all \
  --casefile-artifact-root "$CASEFILE_ARTIFACT_ROOT" \
  --casefile-source-commit "$SOURCE_COMMIT"
python scripts/validate-package-roots.py
```

The reviewed handoff is the downloaded output of one successful `Build Casefile executable matrix`
run at `SOURCE_COMMIT`. Before that workflow exists on the default branch, its only bootstrap is a
separately approved push of the exact reviewed commit to a branch matching `casefile/build-*`. Once
the workflow exists on the default branch, use `workflow_dispatch` with the explicit source-commit
input. Record the run ID, event, head branch, retained build provenance, and `artifacts.json`
SHA-256 with the candidate. Neither build event authorizes publication. Do not use locally invented
fixture artifacts for a release candidate.

Generated marketplace history is published from source; do not edit the marketplace repository by
hand.

## Pull requests and releases

Keep commits atomic and conventional. Open source changes through a branch and require the hosted
reproducibility checks before merge. Update relevant wiki pages in the wiki repository rather than
adding new developer or reference documents beside the source.

For a release:

1. Update the synchronized versions in all three package manifests, `CITATION.cff`, and the README
   install ref.
2. Run the full source and package checks.
3. Merge a green release pull request.
4. Create an annotated source tag on the release merge and publish a GitHub Release for that tag.
5. After separate authorization, obtain the build-only Casefile matrix at the exact reviewed source
   commit through the permitted scoped bootstrap or normal manual-dispatch path. Record the
   successful workflow run ID and event, download and verify its build provenance, complete
   native-smoke and package-inventory handoff, and record the reviewed `artifacts.json` SHA-256.
6. Dispatch `publish-marketplace.yml` with all four reviewed inputs: `version`, `source_commit`,
   `binary_run_id`, and `matrix_manifest_sha256`. The workflow rejects another workflow, a failed or
   incomplete run, a different source SHA, or an incomplete handoff and never rebuilds binaries.
7. Verify the annotated marketplace tag, generated versions, `Source-Commit` provenance, packaged
   executable hashes, and hosted install lifecycle.

Release, history rewrite, branch deletion, and repository-setting changes require explicit human
authority.
