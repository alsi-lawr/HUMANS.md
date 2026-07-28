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

Review the Provider's compact preview envelope. Request confirmation only when
`approval_required = true`, then apply its `preview_id` in the same MCP session.

When starting a new Casefile, activate the new investigation root, then call
`casefile_preview_default_delivery_board`, review its envelope, and apply its `preview_id`. The
request to start the Casefile authorizes this canonical board setup but not a ticket-progress
transition. Stop before delegation if provisioning fails; never replace a differing board.

For a new Codex Casefile, run the installed `scripts/resolve-writer-binding.py offer` against the
active Codex home only after the delivery board is provisioned. Present every returned model/effort
pair, identify Sol/high as the recommendation, and require an explicit exact choice. If the offer
marks Sol/high unavailable, say so and present every remaining valid pair without recommending an
unselectable pair. Do not infer a recommendation as consent. Run the resolver's `select` command
only to materialize the confirmed typed binding request. Pass that request to
`casefile_preview_writer_binding`, review its envelope, and apply its `preview_id`. The explicit
model/effort choice authorizes persistence. If offering, preview, or persistence fails, stop without
further planning or source mutation and surface the diagnostic. This selection gate is Codex-only;
do not change Claude startup. Existing Casefiles being resumed without a binding retain their
selected implementation matrix default and do not receive a start-time backfill.

Keep investigation source access read-only. The root alone arbitrates duplicate findings, reserves
ticket IDs and paths, records decisions, and disposes tickets. Reviewers write evidence only.
Implementation begins only from accepted tickets and an approved dependency-safe plan. Preserve
rejected rationale, contention, verification, and unresolved risk through closeout.
