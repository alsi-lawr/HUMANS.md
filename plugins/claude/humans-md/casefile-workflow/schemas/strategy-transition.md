# Strategy Transition Schema

A transition record contains schema version, UTC timestamp, phase, mode (`governed` or `ad-hoc`), previous and selected strategy IDs, selected matrix path and SHA-256, preserved root binding, preserved work or task-scratch paths, active ownership inventory, available capabilities, human rationale, backup identity, and whether governed state was updated. Planning is a governed phase. Ad-hoc task-scratch transitions require neither tickets nor a durable casefile.

The transition is invalid when the complete selected matrix is malformed, root binding changes, work disappears, capabilities are unavailable, or two active owners overlap. Governed replacement and transition creation are one rollback-verified transaction. Ad-hoc records preserve the complete selected matrix beside the transition.
