# Strategy Binding Schema

`strategy/bindings.toml` is the governed, Casefile-wide implementation-writer overlay. It does not
modify a selected phase matrix. Schema version 1 requires `adapter`, the literal
`role = "implementation-writer"`, `model`, `reasoning_effort`, and a `[resolution]` table with
non-empty `mode` and `value`. The resolution table records how the adapter instantiated the pair
(for example a named profile or runtime override) without making that adapter detail portable.

The Rust Casefile parser owns validation and projection. A binding applies only to the selected
`strategy/implementation.toml` when its adapter matches and that matrix has exactly one
`implementation-writer`. A missing binding uses the matrix writer pair for compatible historical
Casefiles. A present invalid or unmatched binding is not a fallback: clients receive its invalid or
unresolved state and no effective writer pair. Clients must not create a binding before an exact
implementation matrix has been selected and persisted. A binding encountered before implementation
is selected projects as pending, but that projection does not authorize preselection.

Replacing a binding is a typed preview/apply operation, never a client edit. The request contains no
caller activity assertion. The Store derives activity from one valid canonical progress log:
`in_progress`, `in_review`, `verifying`, and `blocked` refuse replacement, while `unknown` and
`complete` permit it; missing, malformed, conflicting, or unsupported progress fails closed. Every
writer spawn separately requires an explicit current `in_progress` transition for its exact ticket.
An accepted binding preview atomically replaces only `bindings.toml`. Repository Git history is the
history authority; no binding archive or journal is maintained.
