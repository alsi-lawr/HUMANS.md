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
persistence; a materially different transition requires a new selection.

For ad-hoc work, require no ticket and use only the local CLI `scratch-strategy` operation. Require
an explicit absolute scratch target and a matrix outside the configured planning Store. The CLI must
refuse any input or target overlapping that Store and writes only the selected scratch file. It
creates no governed transition and is not planning state; no provider or MCP operation exists for
it.
