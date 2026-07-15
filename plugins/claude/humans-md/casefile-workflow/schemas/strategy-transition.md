# Strategy Transition Schema

A transition record contains schema version, UTC timestamp, phase, mode (`governed` or `ad-hoc`), previous and selected strategy IDs, selected matrix path and SHA-256, preserved root binding, preserved work paths, active ownership inventory, available capabilities, human rationale, and whether governed state was updated.

The transition is invalid when root binding changes, work disappears, capabilities are unavailable, or two active owners overlap. Ad-hoc records preserve the complete selected matrix beside the transition.
