---
name: casefile-switch-strategy
description: "Use when a human changes Casefile execution strategy during planning, investigation, review, implementation, closeout, or ad-hoc task-scratch work. Preserves root authority and work while refusing incompatible capabilities or overlapping writers."
---

# Casefile Switch Strategy

Inventory the current phase, exact matrix, root binding, work products, open workers, and active write ownership. Require the human to select an explicit compatible preset or complete ad-hoc matrix; never infer a replacement.

Run the bundled switch validator with the current state, selected matrix, and available capabilities. Refuse a switch that changes the root, loses work references, needs an unavailable capability, or leaves overlapping active writers. Close or transfer workers only through the root.

For governed work, preview the current-to-selected matrix diff, then transactionally back up and replace the phase matrix together with its transition record. Restore and verify the prior matrix if either write fails. For ad-hoc task-scratch work, require no ticket or casefile, but record the complete matrix, scratch work paths, and rationale beside the transition. Resume from preserved work; do not restart accepted work or rewrite historical records.

Load `casefile-workflow/scripts/switch-strategy.py` when validating or applying a transition.
