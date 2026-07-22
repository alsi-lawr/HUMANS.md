# Pipelined implementation

Require the selected `casefile-implement-pipeline` matrix. Keep at most two tickets active: one
under exact-commit review and one in look-ahead or implementation. While the writer implements
ticket N, the read-only look-ahead investigator may preflight one planned ticket N+1 and report
likely write paths, dependencies, interfaces, verification targets, risks, and unresolved questions.

After ticket N has an immutable commit, the root may assign N+1 to the same writer while N is
reviewed only after independently confirming that the tickets have no dependency and no overlapping
write paths. Reviewers inspect the named commit rather than later workspace state. Do not start N+2
until N is accepted. An explicit contract correction to N preempts forward work and returns to the
same writer as a new commit.

The reviewer proposes correction, contention, or follow-up; the root makes the final classification.
Record optional hardening as a non-blocking follow-up. Drain to serial execution when a dependency,
path overlap, correction scope, unavailable exact-commit check, or ownership uncertainty makes
overlap unsafe. Also drain when a finding proposes new architecture, durable state, a dependency, a
failure guarantee, a compatibility promise, public behavior, or material path expansion, or when the
same concern survives one correction. Keep mutation stopped until the human rejects that contention
or amends the governing decision or ticket.

Keep ticket checks focused. Run full workspace, package, and authenticated verification at the
release candidate unless this ticket explicitly owns that surface.

For Codex, resolve the Casefile writer binding with strategy ID `casefile-implement-pipeline`
immediately before the initial writer, each permitted next-ticket spawn, every resume, and every
correction. Use only the returned spawn arguments. Binding unavailability preempts forward work and
stops before mutation pending explicit inactive reselection; it never changes reviewer or look-ahead
bindings.
