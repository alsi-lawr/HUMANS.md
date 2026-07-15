# Case study: thesis-to-plugin migration, 2026-07-15

## Scope

This migration converted HUMANS.md from a source-only instruction repository into portable contracts plus deterministic Codex and Claude plugin packages. Five accepted tickets governed the public-name replacement, reusable skills, vendor adapters, verification machinery, CI, and thesis documentation. One exclusive writer owned overlapping repository output. Primary atomic review rejected the initial implementation and routed concrete corrections; focused verification remains pending.

The [intended durable private destination](https://github.com/alsi-lawr/agent-planning/tree/main/projects/humans-md/investigations/20260715-thesis-and-plugin) remains pending promotion and hash verification under the separate closeout ticket. It is not yet claimed as available. This public summary contains no raw prompts, private transcripts, credentials, local configuration, or full model catalog.

## Decisions preserved

- Active workflow names were replaced completely with the Casefile vocabulary; compatibility aliases were rejected.
- Shared skills, roles, schemas, and scripts remain platform-neutral. Runtime models, effort, setup, metadata, and installation policy live in adapters.
- Packages are committed outputs generated from one portable product manifest with shared resources and vendor adapter sections. The packager discovers multiple product manifests and compares path sets, modes, and bytes, not merely equivalent content.
- Skill verification starts by enumerating compatible installed TOML presets, recommending without selecting, and recording the human-selected path and hash. Candidate and baseline cases are simultaneous; old-skill baselines are immutable; absolute acceptance precedes comparative deltas.
- Deterministic validation replaced known loose schema and delta handling. The migration deliberately omitted an evaluation viewer, description optimizer, automated grader, and any mechanism that exposes rubrics or diagnoses to evaluated prompts.
- Contract bootstrap and runtime profile changes are preview-first, opt-in, backed up, atomic, idempotent, and never installation side effects.

## Safety boundary

The Codex catalog profiler accepts a caller-supplied fresh export and refuses a path named `models_cache.json`; it cannot attest freshness after a file is renamed. It patches eight hash-bound instruction and model-message resource pairs plus declared `multi_agent_version` selectors, preserves other fields, writes hash-addressed backups, restores bytes and metadata on failure, and reports stale profile fields. The drift workflow stores only a comparison report. A separate preview-first cutover tool inventories and hashes complete declared state and has rollback-focused unit evidence, but has never been invoked against live user state.

The Claude package follows the standard root layout: only `plugin.json` is under `.claude-plugin`, while `skills/` and `agents/` are root components using `${CLAUDE_PLUGIN_ROOT}` paths. Its adapter preserves the accepted Sonnet medium-high policy tier while mapping it to the supported `high` frontmatter value. Primary review passed strict structural validation; no Claude installation, loading, routing, or behavioral execution occurred in this batch.

## Evidence and open gates

Local evidence is limited to source checks, stdlib unit tests, deterministic package parity, structural metadata validation, and isolated install/discovery checks that can run without changing live user state. The balanced suite has stable realistic prompts and hidden rubrics for every included revised or adopted skill, but sampled behavior remains `unverified` until fresh isolated candidate and baseline runs are hash-bound to honest runtime records. No run evidence is fabricated here.

No live Codex cutover is claimed. A release requires a host restart into a new root thread, proof of V1 behavior, exact child model and effort probes (especially inspector Terra/xhigh), rollback verification, and removal of only superseded direct copies. Claude strict validation, loading, triggering, and exact tier routing also remain explicit gates.
