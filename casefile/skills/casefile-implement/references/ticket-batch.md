# Ticket-batch implementation

Require the selected `casefile-implement-ticket-batch` matrix. Give the writer a dependency-safe
accepted batch under exclusive ownership. Review and verify its immutable ticket commits before
assigning the next batch. The reviewer proposes correction, contention, or follow-up; the root makes
the final classification. Return an explicit contract correction to the same writer. Record an
optional hardening follow-up without blocking acceptance. Stop the batch for human resolution when a
finding expands architecture, durable state, dependencies, failure guarantees, compatibility, public
behavior, or material paths, or when the same concern survives one correction. Complete each ticket
only after its recorded review flow accepts it.

Keep ticket checks focused. Run full workspace, package, and authenticated verification at the
release candidate unless this ticket explicitly owns that surface.

For Codex, follow the delegation check in `casefile` for the writer, atomic ticket reviewer, and
verification reviewer under `casefile-implement-ticket-batch`. Repeat it for each resumed batch and
correction assignment.
