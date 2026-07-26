---
name: casefile
description:
  "Use to start or resume substantial repository work that needs traceable investigation, human
  decisions, reviewed tickets, implementation, and evidence."
---

# Casefile

The request-receiving root remains the orchestrator with the model and effort through which the
human invoked it. Root is an authority binding, not a model profile. Read repository authority and
current state, bound the work, resolve the configured planning store, and validate its
project-to-source mapping. Write every durable Casefile artifact directly in that resolved store;
never clone or mirror the planning store in task scratch. Use the session's `.agent-workspace` only
for disposable, non-authoritative previews, content-hash backups, isolated output, and command logs.
Never infer a source path or replace the root.

Route the current phase to `casefile-investigate`, `casefile-review`, `casefile-implement`, or
`casefile-close`. Every governed phase requires an explicit compatible strategy. Present compatible
choices and a recommendation, then wait for human selection.

When starting a new Codex Casefile, activate the new investigation root, then run the installed
`scripts/resolve-writer-binding.py offer` against the active Codex home before further delegation.
Present every returned model/effort pair, identify Sol/high as the recommendation, and require an
explicit exact choice. If the offer marks Sol/high unavailable, say so and present every remaining
valid pair without recommending an unselectable pair. Do not infer a recommendation as consent.
Persist only the confirmed pair with the resolver's `select` command and
`--implementation-active false`; it delegates to the canonical Casefile binding transaction. If
offering or persistence fails, stop without further planning or source mutation and surface the
diagnostic. This selection gate is Codex-only; do not change Claude startup. Existing Casefiles
being resumed without a binding retain their selected implementation matrix default and do not
receive a start-time backfill.

Keep investigation source access read-only. The root alone arbitrates duplicate findings, reserves
ticket IDs and paths, records decisions, and disposes tickets. Reviewers write evidence only.
Implementation begins only from accepted tickets and an approved dependency-safe plan. Preserve
rejected rationale, contention, verification, and unresolved risk through closeout.
