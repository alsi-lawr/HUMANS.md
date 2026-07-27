# Ticket-batch implementation with look-ahead

Require the selected `casefile-implement-ticket-batch-look-ahead` matrix. Give the writer a
dependency-safe accepted batch under exclusive ownership. While that batch is implemented, the
read-only look-ahead investigator may preflight one root-assigned upcoming accepted ticket and
report likely write paths, dependencies, interfaces, verification targets, risks, and unresolved
questions. The report is advisory: it does not reserve ownership, expand scope, or authorise
implementation.

Review and verify the current batch's immutable ticket commits before assigning the next batch. Do
not begin the preflighted ticket during current-batch review; that overlap belongs only to the
pipeline strategy. Discard or repeat stale preflight when a correction, contention, dependency, or
plan change invalidates its evidence.

The reviewer proposes correction, contention, or follow-up; the root makes the final classification.
Return an explicit contract correction to the same writer. Record optional hardening without
blocking acceptance. Stop the batch for human resolution when a finding expands architecture,
durable state, dependencies, failure guarantees, compatibility, public behavior, or material paths,
or when the same concern survives one correction. Complete each ticket only after its recorded
review flow accepts it.

Keep ticket checks focused. Run full workspace, package, and authenticated verification at the
release candidate unless this ticket explicitly owns that surface.

For Codex, resolve the Casefile writer binding with strategy ID
`casefile-implement-ticket-batch-look-ahead` immediately before the initial writer, each resumed
batch, and each correction spawn. Use only the returned spawn arguments; an unavailable pair stops
the batch before mutation pending explicit inactive reselection. Writer resolution does not grant
the look-ahead investigator write authority.
