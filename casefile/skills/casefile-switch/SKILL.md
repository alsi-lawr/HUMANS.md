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
display and save the complete immutable preview, and request explicit human approval. Only after
approval pass that exact preview unchanged to `casefile_apply_strategy_transition`; provider write
authority is not approval.

For ad-hoc work, require no ticket and use only the local CLI `scratch-strategy` operation. Require
an explicit absolute scratch target and a matrix outside the configured planning Store. The CLI must
refuse any input or target overlapping that Store and writes only the selected scratch file. It
creates no governed transition, is not planning state, and is deliberately absent from provider and
MCP capability discovery.
