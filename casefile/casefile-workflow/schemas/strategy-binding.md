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
unresolved state and no effective writer pair. Before implementation is selected, a valid binding is
pending.

Replacing a binding is an adapter/runtime operation, never a client edit. It must be refused while
an implementation writer or correction is active and atomically replace only `bindings.toml`.
Repository Git history is the history authority; no binding archive or journal is maintained.
