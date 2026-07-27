---
name: casefile-consolidate
description:
  "Use only for an explicit request to migrate one selected activated Casefile investigation to the
  progress-log shape, or to diagnose and repair that investigation's progress log. Do not use for
  ordinary Casefile start, investigation, review, implementation, closeout, strategy changes,
  generic validation, or unrelated planning cleanup."
---

# Casefile Consolidate

Resolve and show the planning root and the exact investigation before doing anything. Validate the
selected target with the canonical CLI and report root-wide diagnostics outside that scope as
inherited context; they neither authorize a bypass nor block a valid target.

Use the fixed-root provider for progress bootstrap and append operations. Put the complete immutable
preview in the current task's `.agent-workspace/<session-id>/` directory, outside the planning root.
Display it, obtain explicit human approval, and apply that exact preview unchanged in the same MCP
session. Do not parse or write `progress/log.toml`, ticket Markdown, or another progress file.

## Migrate an absent progress log

First call `casefile_preview_progress` with the typed `bootstrap` operation. Present its exact
target, eligible accepted ticket IDs that will derive as `unknown`, proposed empty log, diff, and
scoped diagnostics. It creates only an absent `progress/log.toml` containing `schema_version = 1`;
it records no invented ticket history.

Ask for an explicit apply decision after the preview. On approval, call `casefile_apply_progress`
with the saved preview unchanged and report the resulting path and revision. An existing valid log
is a no-op. A malformed or noncanonical log, legacy layout, unactivated target, rejected/provisional
ticket, ambiguous scope, or divergent preview is refused or reported without repair-by-migration.

## Repair one malformed log

Require caller-supplied exact replacement content in a file outside the planning root. Do not infer
state, reconstruct history, or convert legacy layouts. Before previewing, copy the malformed
target's exact bytes to task scratch under a SHA-256 content-hash filename and report that backup
path and retention through closeout. Then invoke the CLI human/recovery adapter's
`progress-repair-preview` command and show its complete diff, target, scoped diagnostics, and backup
plan. Whole-log replacement remains outside provider capability discovery.

Wait for an explicit apply decision. Apply only the saved preview with `progress-repair-apply`. The
canonical writer performs the one-file atomic replacement and post-write validation; report failure
without editing the original yourself. A matching retry is a no-op. If the target or planning-root
revision changed, stop and make a fresh preview; do not reuse, merge, or alter the caller's
replacement.

For command examples and developer validation/package guidance, see `CONTRIBUTING.md`; user-facing
migration and repair reference lives in the project wiki.

## Provision the delivery board

After the progress-log outcome is successfully applied or confirmed as an existing valid no-op, call
`casefile_preview_default_delivery_board` for the same selected investigation. Save its fresh
immutable preview in the current task's `.agent-workspace/<session-id>/` directory, outside the
planning root. Show the exact target, diff, and diagnostics, then wait for an explicit apply
decision. Apply only that saved preview unchanged with `casefile_apply_default_delivery_board`. An
exact existing `boards/delivery.toml` is a content no-op; a differing file, symlink, non-file
collision, stale revision, invalid activation, missing mapping, or ambiguous mapping is preserved
and visibly refused.

The canonical identity combines the configured project prefix and mapped investigation directory
name. Before preview and apply, preflight every activated mapping and refuse if that identity does
not map to exactly one investigation. The generic preview may preserve unchanged diagnostics from
its exact pre-write Store baseline, which remain visible in canonical scan, check, and query output;
it still refuses every diagnostic introduced or changed by this board request. Apply remains pinned
to the complete saved Store revision.

This step creates or confirms only the explicit delivery board. It does not read or mutate the
progress log or tickets. Keep progress and board previews/applies sequential and independent; do not
add rollback, recovery, a journal, or a combined transaction.

When preparing the selected balanced verification, load `references/verification-contract.md`. It
records the candidate/no-skill evidence gate; it is not evidence that the gate ran.
