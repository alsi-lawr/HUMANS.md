<div align="center">

# HUMANS.md

**A thesis and reproducible plugin system for controlled acceleration: lean standing conduct, task-shaped skills, and governed Casefile work.**

</div>

Repository instruction files often become encyclopedias. HUMANS.md argues for a narrower split: always-loaded text governs conduct, skills carry task knowledge only when needed, and deterministic tools enforce boundaries prose cannot guarantee. The repository now ships that thesis as portable source plus reproducible `humans-md` packages for Codex and Claude.

## The thesis

Always-loaded instructions change agent defaults before a task is understood. They should therefore carry durable behavioural invariants rather than setup lore, broad repository context, or task procedures. This project separates five concerns:

- [`AGENTS.md`](AGENTS.md) is the standing behaviour contract.
- [`HUMANS.md`](HUMANS.md) preserves the human rationale and evidence.
- [`skills/`](skills/) contains portable task models loaded on demand.
- [`casefile-workflow/`](casefile-workflow/) contains portable roles, schemas, and guarded workflow scripts.
- [`adapters/`](adapters/) and the portable [`packaging/plugins/humans-md.toml`](packaging/plugins/humans-md.toml) product manifest bind shared source to runtimes and deterministically generate committed packages.

The operating aim is controlled acceleration: faster work because boundaries, choices, ownership, and evidence are easier to inspect.

## Casefile

Casefile turns repository investigation and accepted implementation into governed, reviewable records. Its active public skills are:

- `casefile-workflow`
- `casefile-investigate-solo`, `casefile-investigate-atomic`, and `casefile-investigate-inspector-tree`
- `casefile-review-atomic`, `casefile-review-dialogue`, and `casefile-review-two-stage`
- `casefile-implement-ticket-batch`, `casefile-switch-strategy`, and `casefile-closeout`

Every governed phase requires an explicit compatible matrix. The request-receiving root retains authority; investigators report candidates before tickets, overlapping writes have one owner, reviews are recorded, and strategy switches preserve work while refusing capability or ownership conflicts.

## Portable skills

The packages include every Casefile skill plus `contract-bootstrap`, `git-contribution`, `skill-generator`, `skill-packaging`, and `readme-generator`. The `build-code` and `test-benchmark-code` skills are intentionally excluded for a later code-skills package.

`skill-generator` begins every create, revise, or audit task by enumerating compatible installed TOML verification presets, recommending one without selecting it, and recording the human's selected path and hash in the task model. Stable suites separate positive triggers, hard near-neighbour non-triggers, and task behaviour from hidden rubrics. Candidate and no-skill or immutable-old-skill baseline arms run in the same evaluation window, absolute acceptance precedes comparative deltas, and evidence remains classified rather than inflated.

## Reproducible packages

Both committed packages are version 0.1.0, published as `alsi-lawr`, sourced from `alsi-lawr/HUMANS.md`, and licensed MIT:

- [`plugins/codex/humans-md/`](plugins/codex/humans-md/)
- [`plugins/claude/humans-md/`](plugins/claude/humans-md/)

Build and compare every discovered product and vendor adapter with Python 3.14 and the standard library only:

```sh
python scripts/strip-non-ascii.py --check .
python scripts/package-plugin.py build --all
python scripts/package-plugin.py check --all
python scripts/validate-skill.py --all --root .
python scripts/validate-casefile.py --source .
python -m unittest discover -s tests -v
```

The packager has no hard-coded product name, identity, or vendor list. It rejects traversal, symlinks, missing or empty sources, duplicate destinations, overlapping outputs, non-ASCII text, mode drift, byte drift, and stale output. Generated packages contain all runtime resources and do not depend on this checkout after installation. Both include the canonical `AGENTS.md` template; installation never applies it, and `contract-bootstrap` still requires an explicit source, destination, preview, and replacement decision.

## Adapter status

### Codex

The Codex package contains `.codex-plugin/plugin.json`, repository marketplace metadata, 11 matrix-qualified worker profiles, exact matrices, setup and cutover skills, and guarded scripts. The accepted V1 contract binds root to Sol/xhigh and matrix workers to Terra at their declared efforts, with `multi_agent = true`, `multi_agent_v2 = false`, and declared `multi_agent_version` selectors set to JSON null. Eight locally authored model instruction/message pairs are stored as separate adapter resources; no full catalog is committed.

Add the package directory as a personal marketplace only when ready to inspect it:

```sh
codex plugin marketplace add /absolute/path/to/plugins/codex/humans-md --json
codex plugin add humans-md@humans-md --json
codex plugin list --available --json
```

Installation and cutover are separate, opt-in operations. The setup skill renders candidate configuration; it does not edit active configuration. The cutover tool is preview-only unless explicitly applied with a complete plan. It inventories and hashes active configuration, direct resources, and marketplace state; installs the marketplace package and reviewed complete configuration; requires strict, discovery, fresh V1, root, and inspector probes; removes only named superseded copies after success; and restores plus hash-verifies the complete inventory on failure.

The 2026-07-15 personal cutover completed successfully. Fresh processes recorded V1, Sol/xhigh at the root, and Terra/xhigh for the matrix-qualified inspector; source, installed, and packaged bytes matched. Fifteen superseded direct paths were removed after the gates passed, while unrelated direct skills and global instructions were preserved. The hash-addressed rollback inventory remains local; rollback was not exercised because no gate failed.

The catalog profiler consumes a caller-supplied fresh export, but cannot mechanically attest freshness after a file is renamed. It rejects a path named `models_cache.json`, input/target aliasing, and symlink inputs; patches only hash-bound authored resources and declared selectors; and restores prior bytes, mode, and mtime after a failed write or verification.

### Claude

The Claude package keeps only `plugin.json` beneath `.claude-plugin`; `skills/` and `agents/` are package-root components and resource references use `${CLAUDE_PLUGIN_ROOT}`. Its policy tiers are Opus/high for inspectors and review chairs, Sonnet/medium-high for investigators, writers, challengers, and atomic reviewers, and Haiku/medium for focused verification. The adapter records the medium-high policy tier and maps it to the supported `high` frontmatter value rather than emitting an invalid level.

The strict structural command is:

```sh
claude plugin validate plugins/claude/humans-md --strict
```

Local verification passed Claude's strict structural validator. The package was not installed, loaded, triggered, routed, or behaviourally tested; those live checks remain release gates.

## Contract bootstrap

Package installation never changes a repository contract. Each package carries `templates/AGENTS.md`, but `contract-bootstrap` acts only on an explicit request and explicit source and destination. It previews the complete diff, refuses an ambiguous merge, requires replacement authority for a differing target, creates a hash-addressed backup, writes atomically, and leaves an identical target untouched.

## Evidence and reproducibility

Mechanical evidence comes from the stdlib tests, standalone source/package validators, package parity check, ASCII check, matrix/profile cross-checks, and vendor structural checks. Behavioral evidence uses the separate [`verification/`](verification/) strategies, suite, realistic isolated prompts, hidden rubrics, and hash-bound run schema. No behavioral run records are present, so sampled behavior remains `unverified`. The 2026-07-15 migration is summarized in the sanitized [Casefile plugin workflow case study](docs/2026-07-15-casefile-plugin-workflow.md); its governed private record was promoted and hash-verified under HMD-006.

CI repeats ASCII, tests, source and standalone package validation, strict Claude validation, and isolated Codex marketplace add, plugin add, and discovery. A weekly/manual workflow exports the latest bundled Codex catalog and compares only declared profile-relevant fields, including non-null `multi_agent_version`. It maintains one labelled issue and never edits instructions or commits catalog data.

## Limitations and release gates

- Claude installation, loading, routing, and behavioural verification are unresolved gates.
- The balanced behavioral suite is specified but must be executed in isolated runtime contexts for every included revised or adopted skill before behavioral claims.
- The durable private record contains no reconstructed prior-agent artifacts; the public case study contains no raw transcript.
- Publication, pushing, release tags, automated grading, evaluation viewers, description optimization, and code-skill packaging are outside this batch.

Released under the [MIT licence](LICENSE).
