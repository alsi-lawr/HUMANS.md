# Strategy Transition Schema

A governed transition is a strict schema-version-1 TOML record at
`strategy/transitions/<UTC-token>-<operation-id>.toml`. The filename is deterministic from its
RFC-3339 `recorded_at` value and lowercase hyphenated `operation_id`. The record carries the phase,
previous and selected strategy IDs, selected-matrix origin and SHA-256, expected Store and matrix
revisions, proposed matrix revision, preserved root binding, governed-update fact, human rationale,
available capabilities, preserved safe work paths, and active ownership claims.

Only the governed strategy-transition preview/apply operation creates this record. It validates the
complete selected matrix through the canonical Rust parser, requires the selected phase to match the
governed target, preserves `root`, checks required capabilities, and rejects overlapping active
owners. The selected matrix and transition record are one failure-atomic, rollback-verified
transaction. Exact operation replay is a content no-op; reuse of its deterministic identity with
different content is refused.

The first transition for a phase may create its missing governed matrix. Its transition records
`unselected` as the previous strategy and `absent` as the expected matrix revision. A missing matrix
after a recorded transition for the same phase is rejected as damaged governed state rather than
silently recreated.

No transition backup, journal, or preview-history artifact is created. Git is the durable history
authority. Historical pre-schema transition and backup files remain raw and untouched.
