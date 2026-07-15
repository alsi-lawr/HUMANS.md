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
- [`adapters/`](adapters/) and [`packaging/plugins/`](packaging/plugins/) bind portable source to a runtime and deterministically generate committed packages.

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

`skill-generator` begins with a human-selected TOML verification strategy. Stable suites separate positive triggers, hard near-neighbour non-triggers, and task behaviour from hidden rubrics. Candidate and no-skill or immutable-old-skill baseline arms run in the same evaluation window, absolute acceptance precedes comparative deltas, and evidence remains classified rather than inflated.

## Reproducible packages

Both committed packages are version 0.1.0, published as `alsi-lawr`, sourced from `alsi-lawr/HUMANS.md`, and licensed MIT:

- [`plugins/codex/humans-md/`](plugins/codex/humans-md/)
- [`plugins/claude/humans-md/`](plugins/claude/humans-md/)

Build and compare every manifest with Python 3.14 and the standard library only:

```sh
python scripts/strip-non-ascii.py --check .
python scripts/package-plugin.py build --all
python scripts/package-plugin.py check --all
python scripts/validate-skill.py --all --root .
python scripts/validate-casefile.py --source .
python -m unittest discover -s tests -v
```

The packager rejects traversal, symlinks, missing or empty sources, duplicate destinations, non-ASCII text, mode drift, byte drift, and stale output. Generated packages contain all runtime resources and do not depend on this checkout after installation.

## Adapter status

### Codex

The Codex package contains `.codex-plugin/plugin.json`, repository marketplace metadata, named worker profiles, exact matrices, setup skills, and guarded scripts. The accepted V1 contract binds root to Sol/xhigh and matrix workers to Terra at their declared efforts, with `multi_agent = true` and `multi_agent_v2 = false`.

Add the package directory as a personal marketplace only when ready to inspect it:

```sh
codex plugin marketplace add /absolute/path/to/plugins/codex/humans-md --json
codex plugin list --available --json
```

Installation and cutover are separate, opt-in operations. The setup skill renders candidate configuration; it does not edit active configuration. A live cutover must back up direct resources and relevant configuration, install through the marketplace, validate strict configuration and discovery, restart into a new root thread, prove V1 plus exact child model and effort, and restore automatically on failure. This repository does not distribute or edit `models_cache.json`.

### Claude

The Claude package keeps only `plugin.json` beneath `.claude-plugin`; `skills/` and `agents/` are package-root components and resource references use `${CLAUDE_PLUGIN_ROOT}`. Its policy tiers are Opus/high for inspectors and review chairs, Sonnet/medium-high for investigators, writers, challengers, and atomic reviewers, and Haiku/medium for focused verification. The adapter records the medium-high policy tier and maps it to the supported `high` frontmatter value rather than emitting an invalid level.

The strict structural command is:

```sh
claude plugin validate plugins/claude/humans-md --strict
```

Claude was not installed, validated, or behaviourally tested as part of this implementation batch. Strict validation, installation, loading, triggering, and exact routing remain release gates.

## Contract bootstrap

Package installation never changes a repository contract. `contract-bootstrap` acts only on an explicit request and explicit source and destination. It previews the complete diff, refuses an ambiguous merge, requires replacement authority for a differing target, creates a hash-addressed backup, writes atomically, and leaves an identical target untouched.

## Evidence and reproducibility

Mechanical evidence comes from the stdlib tests, source validators, package parity check, ASCII check, matrix validation, and vendor structural checks. Behavioral evidence uses the separate [`verification/`](verification/) strategies, suite, isolated prompts, and rubrics. The 2026-07-15 migration is summarized in the sanitized [Casefile migration case study](docs/case-studies/2026-07-15-thesis-and-plugin.md); governed private evidence remains access-controlled.

CI repeats ASCII, tests, source and package validation, strict Claude validation, and isolated Codex marketplace discovery. A weekly/manual workflow exports the latest bundled Codex catalog and compares only declared profile-relevant fields. It maintains one labelled issue and never edits instructions or commits catalog data.

## Limitations and release gates

- No live Codex cutover success is claimed. Fresh-process V1 and exact inspector model/effort proof are unresolved gates.
- Claude installation and behavioural verification are unresolved gates.
- The balanced behavioral suite is specified but must be executed in isolated runtime contexts for every included revised or adopted skill before behavioral claims.
- Durable private record promotion is a separate closeout ticket; public documentation contains no raw transcript.
- Publication, pushing, release tags, automated grading, evaluation viewers, description optimization, and code-skill packaging are outside this batch.

Released under the [MIT licence](LICENSE).
