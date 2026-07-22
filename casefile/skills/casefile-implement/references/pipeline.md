# Pipelined implementation

Require the selected `casefile-implement-pipeline` matrix. Keep at most two tickets active: one
under exact-commit review and one in look-ahead or implementation. While the writer implements
ticket N, the read-only look-ahead investigator may preflight one planned ticket N+1 and report
likely write paths, dependencies, interfaces, verification targets, risks, and unresolved questions.

After ticket N has an immutable commit, the root may assign N+1 to the same writer while N is
reviewed only after independently confirming that the tickets have no dependency and no overlapping
write paths. Reviewers inspect the named commit rather than later workspace state. Do not start N+2
until N is accepted. A correction to N preempts forward work and returns to the same writer as a new
commit.

Drain to serial execution when a dependency, path overlap, correction scope, unavailable
exact-commit check, or ownership uncertainty makes overlap unsafe.

For Codex, resolve the Casefile writer binding with strategy ID `casefile-implement-pipeline`
immediately before the initial writer, each permitted next-ticket spawn, every resume, and every
correction. Use only the returned spawn arguments. Binding unavailability preempts forward work and
stops before mutation pending explicit inactive reselection; it never changes reviewer or look-ahead
bindings.
