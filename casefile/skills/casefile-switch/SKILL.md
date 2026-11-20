---
name: casefile-switch
description:
  "Use to change Casefile strategy mid-task while preserving the root, completed work, active
  ownership, and governed or ad-hoc records."
---

# Casefile Switch

Inventory the current phase, matrix, root binding, work products, workers, and active write
ownership. Present compatible presets and require explicit selection. Refuse unavailable
capabilities, root replacement, lost work, or overlapping active writers.

For governed work, build the typed transition request, call `casefile_preview_strategy_transition`,
review its envelope, then apply its `preview_id`. The explicit strategy selection authorizes this
persistence; a materially different transition requires a new selection. For a selected governed
Codex strategy, load `casefile` and follow its role assignment flow for the new exact matrix before
delegation. Reuse still-eligible saved choices for that strategy when available. Run the Codex
delegation check for the new matrix even when continuing an existing worker.

For ad-hoc work, require no ticket and use only the local CLI `scratch-strategy` operation. Require
an explicit absolute scratch target and a matrix outside the configured planning Store. The CLI must
refuse any input or target overlapping that Store and writes only the selected scratch file. It
creates no governed transition and is not planning state; no provider or MCP operation exists for
it.

The scratch operation provisions a strategy. Codex delegation requires valid explicit role
assignments for that strategy. Leave delegation pending when the scratch flow supplies only a
matrix.
