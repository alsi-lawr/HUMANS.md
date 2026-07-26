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
python scripts/package-plugin.py build --all
python scripts/package-plugin.py check --all
```

`nix flake check` evaluates the flake. CI can also be replayed from inside the shell with a cached
runner image:

```sh
act pull_request -j validate --pull=false \
  -P ubuntu-latest=catthehacker/ubuntu:act-latest
```

## Casefile development

The Casefile Rust workspace owns canonical parsing, validation, querying, preview, and apply
semantics. The SQLite adapter is a disposable derived index. The loopback server fixes one planning
root at launch and embeds the tracked browser build; the browser does not parse or write planning
files directly.

### Ticket progress and consolidation

`progress/log.toml` is an investigation-scoped canonical record. Ticket disposition remains the
review decision; delivery progress is derived separately. The Rust Store and
`casefile-workflow/scripts/transition-ticket-progress.py` are the only supported progress write
path. Do not edit a progress log, ticket frontmatter, or a second progress file to migrate, repair,
or update ticket delivery state.

For a selected active investigation, first validate only that scope, then save the script's preview
outside the planning root. Apply only the unchanged saved preview:

```sh
casefile --root "$CASEFILE_ROOT" check --require-activation --investigation "$INVESTIGATION"
python casefile/casefile-workflow/scripts/transition-ticket-progress.py \
  --root "$CASEFILE_ROOT" --casefile casefile --preview-file "$TASK_SCRATCH/progress-preview.json" \
  bootstrap-unknown --investigation "$INVESTIGATION"
# Obtain the explicit apply decision, then rerun with --apply and the same preview file.
```

The same script owns ordinary transitions and typed notes. Supply the ticket's currently derived
state in `--from`, use a stable operation ID, and preserve the generated preview for the apply or
exact retry. Notes use category `deviation` or `quirk` and never change state. Do not backfill
stages that were not captured when they occurred; record a note instead. The `replace` action
accepts an exact caller-supplied complete log only for malformed-log repair. No action may use a
preview inside the planning root.

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
progress-log outcome, provision its canonical delivery board through the separate wrapper:

```sh
python casefile/casefile-workflow/scripts/provision-delivery-board.py \
  --root "$CASEFILE_ROOT" --casefile casefile \
  --preview-file "$TASK_SCRATCH/delivery-board-preview.json" \
  --investigation "$INVESTIGATION"
# At a consolidation gate, obtain the explicit apply decision first. New-Casefile setup already
# authorizes this exact record. Apply only the saved preview:
python casefile/casefile-workflow/scripts/provision-delivery-board.py \
  --root "$CASEFILE_ROOT" --casefile casefile \
  --preview-file "$TASK_SCRATCH/delivery-board-preview.json" \
  --investigation "$INVESTIGATION" --apply
```

The wrapper selects the exact activated project's prefix and mapped investigation directory name
only to construct `<PREFIX>-<INVESTIGATION-DIRECTORY>-delivery`. This keeps board identities unique
when one project has multiple investigations with distinct final directory names. Before preview and
apply, the wrapper preflights every activated mapping and refuses if the derived identity maps to
anything other than exactly one investigation. The Rust `preview` and `apply` operations remain
authoritative for board rendering, path checks, validation, Store revisions, and the one-file atomic
write. Generic preview compares the proposed diagnostics with its exact pre-write baseline:
unchanged baseline diagnostics remain visible to scan, check, and query but do not block the write;
an introduced or changed diagnostic does. The whole-Store revision still pins that baseline through
apply. The wrapper creates an absent `boards/delivery.toml`, reports exact canonical content as a
no-op, and refuses a different target without replacement. It never reads or mutates progress or
tickets, and consolidation keeps the progress and board writes sequential rather than transactional.

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

`casefile/adapters/codex/scripts/resolve-writer-binding.py` reads the active effective catalog from
the configured Codex home. It offers only visible model/effort pairs that match the selected
multi-agent runtime and have a verified packaged resolution for both implementation strategies. V1
requires an exact generated named profile for the pair. V2 requires each strategy's runtime wrapper,
a positive fork context, and explicit model and effort overrides at spawn.

Sol/high is a recommendation, not a default. The offer reports whether it is available and always
requires an explicit exact selection. If it is unavailable, present the remaining offered pairs
without recommending a substitute. Before ticket-batch, pipeline, resumed, or correction work,
resolve the canonical projection and revalidate the pair against a fresh offer. Stop before
delegation for pending, unresolved, invalid, or newly unavailable state; obtain explicit reselection
while implementation is inactive.

Binding replacement uses the Casefile CLI's `replace-strategy-binding` operation. It validates the
candidate and atomically replaces only `strategy/bindings.toml` with a temporary-file rename. It
must be refused while implementation or correction work is active. Git history is the only history
boundary: do not add an archive, journal, second state file, or client-side write path.

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
to V1; V2 requires Codex 0.145.0 or newer. The Claude adapter supplies workflow skills, matrices,
and role agents without owning the standing contract. Neither adapter removes the shared marketplace
or sibling plugins.

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

The Casefile browser build under `casefile/casefile-server/web/` is tracked and embedded by Rust.
After changing `casefile/web/`, rebuild it and verify that the committed assets are intentional.

To validate the full generated marketplace tree:

```sh
python scripts/package-plugin.py build --all
python scripts/build-marketplace-catalog.py
cp -R packaging/marketplace/. build/marketplace/
cp LICENSE build/marketplace/LICENSE
python scripts/package-plugin.py check --all
python scripts/validate-package-roots.py
```

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
5. Dispatch `publish-marketplace.yml` with the manifest version.
6. Verify the annotated marketplace tag, generated versions, and `Source-Commit` provenance.

Release, history rewrite, branch deletion, and repository-setting changes require explicit human
authority.
