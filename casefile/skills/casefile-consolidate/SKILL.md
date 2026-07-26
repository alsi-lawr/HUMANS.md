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

Use only `casefile-workflow/scripts/transition-ticket-progress.py` for a progress mutation. Put its
immutable preview file in the current task's `.agent-workspace/<session-id>/` directory, outside the
planning root. Do not parse or write `progress/log.toml`, ticket Markdown, or another progress file.

## Migrate an absent progress log

First run `bootstrap-unknown` without `--apply`. Present its exact target, eligible accepted ticket
IDs that will derive as `unknown`, proposed empty log, diff, and scoped diagnostics. It creates only
an absent `progress/log.toml` containing `schema_version = 1`; it records no invented ticket
history.

Ask for an explicit apply decision after the preview. On approval, apply the saved preview unchanged
and report the resulting path and revision. An existing valid log is a no-op. A malformed or
noncanonical log, legacy layout, unactivated target, rejected/provisional ticket, ambiguous scope,
or divergent preview is refused or reported without repair-by-migration.

## Repair one malformed log

Require caller-supplied exact replacement content in a file outside the planning root. Do not infer
state, reconstruct history, or convert legacy layouts. Before previewing, copy the malformed
target's exact bytes to task scratch under a SHA-256 content-hash filename and report that backup
path and retention through closeout. Then invoke the same script's `replace` action without
`--apply` and show its complete diff, target, scoped diagnostics, and backup plan.

Wait for an explicit apply decision. Apply only the saved preview. The canonical writer performs the
one-file atomic replacement and post-write validation; report failure without editing the original
yourself. A matching retry is a no-op. If the target or planning-root revision changed, stop and
make a fresh preview; do not reuse, merge, or alter the caller's replacement.

For command examples and developer validation/package guidance, see `CONTRIBUTING.md`; user-facing
migration and repair reference lives in the project wiki.

When preparing the selected balanced verification, load `references/verification-contract.md`. It
records the candidate/no-skill evidence gate; it is not evidence that the gate ran.
