# Case study: thesis-to-plugin migration, 2026-07-15

## Scope

This migration converted HUMANS.md from a source-only instruction repository into portable contracts plus deterministic Codex and Claude plugin packages. Five accepted tickets governed the public-name replacement, reusable skills, vendor adapters, verification machinery, CI, and thesis documentation. One exclusive writer owned overlapping repository output; atomic review and focused verification remain separate recorded stages.

The durable private casefile is [available to authorised maintainers](https://github.com/alsi-lawr/agent-planning/tree/main/projects/humans-md/investigations/20260715-thesis-and-plugin). Promotion and final disposition are owned by the separate closeout ticket. This public summary contains no raw prompts, private transcripts, credentials, local configuration, or full model catalog.

## Decisions preserved

- Active workflow names were replaced completely with the Casefile vocabulary; compatibility aliases were rejected.
- Shared skills, roles, schemas, and scripts remain platform-neutral. Runtime models, effort, setup, metadata, and installation policy live in adapters.
- Packages are committed outputs generated from declarative multi-plugin manifests. Reproducibility means equal path sets, modes, and bytes, not merely equivalent content.
- Skill verification starts with a human-selected TOML strategy. Candidate and baseline cases are simultaneous; old-skill baselines are immutable; absolute acceptance precedes comparative deltas.
- Deterministic validation replaced known loose schema and delta handling. The migration deliberately omitted an evaluation viewer, description optimizer, automated grader, and any mechanism that exposes rubrics or diagnoses to evaluated prompts.
- Contract bootstrap and runtime profile changes are preview-first, opt-in, backed up, atomic, idempotent, and never installation side effects.

## Safety boundary

The Codex catalog profiler accepts only an explicit fresh export and refuses `models_cache.json`. It patches allowlisted instruction or model-message fields and declared selectors, preserves other fields, writes hash-addressed backups, restores on failure, and reports stale profile fields. The drift workflow stores only a comparison report.

The Claude package follows the standard root layout: only `plugin.json` is under `.claude-plugin`, while `skills/` and `agents/` are root components using `${CLAUDE_PLUGIN_ROOT}` paths. Its adapter preserves the accepted Sonnet medium-high policy tier while mapping it to the supported `high` frontmatter value. No Claude installation or behavioral execution occurred in this batch.

## Evidence and open gates

Local evidence is limited to source checks, stdlib unit tests, deterministic package parity, structural metadata validation, and isolated discovery checks that can run without changing user state. The balanced suite has stable prompts and rubrics for every included revised or adopted skill, but sampled behavior must still be produced by fresh isolated runtime runs.

No live Codex cutover is claimed. A release requires a host restart into a new root thread, proof of V1 behavior, exact child model and effort probes (especially inspector Terra/xhigh), rollback verification, and removal of only superseded direct copies. Claude strict validation, loading, triggering, and exact tier routing also remain explicit gates.
