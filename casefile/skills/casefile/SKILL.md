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

Establish read context hierarchically through Provider protocol v2: call `casefile_snapshot`, use
its catalogue to resolve the exact project and complete investigation scope, then request that
scope's `record_index`. Request `record_detail` only for the exact identities necessary for the
current step. Never request unscoped or bulk records, infer an investigation path by concatenation,
or combine snapshot, index, detail, board, or transition results carrying different revisions.

Route the current phase to `casefile-investigate`, `casefile-review`, `casefile-implement`, or
`casefile-close`. Every governed phase requires an explicit compatible strategy. Present compatible
choices and a recommendation, then wait for human selection.

Review the Provider's compact preview envelope. Request confirmation only when
`approval_required = true`, then apply its `preview_id` in the same MCP session.

When starting a new Casefile, activate the new investigation root, then call
`casefile_preview_default_delivery_board`, review its envelope, and apply its `preview_id`. The
request to start the Casefile authorizes this canonical board setup but not a ticket-progress
transition. Stop before delegation if provisioning fails; never replace a differing board.

Do not offer, select, preview, or persist an implementation-writer binding during startup,
investigation, or review. A binding is implementation-phase state: its selection requires an
accepted dependency-safe plan, the human's explicit implementation-strategy choice, and the exact
selected implementation matrix to be persisted and valid. Route binding selection through
`casefile-implement`; never pre-create a matrix to satisfy the binding Provider.

Keep investigation source access read-only. The root alone arbitrates duplicate findings, reserves
ticket IDs and paths, records decisions, and disposes tickets. Reviewers write evidence only.
Implementation begins only from accepted tickets and an approved dependency-safe plan. Preserve
rejected rationale, contention, verification, and unresolved risk through closeout.
