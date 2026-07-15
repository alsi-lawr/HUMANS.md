---
name: casefile-switch-strategy
description: "Use when a human changes Casefile execution strategy during investigation, review, planning, implementation, or closeout. Preserves root authority and work while refusing incompatible capabilities or overlapping writers."
---

# Casefile Switch Strategy

Inventory the current phase, exact matrix, root binding, work products, open workers, and active write ownership. Require the human to select an explicit compatible preset or complete ad-hoc matrix; never infer a replacement.

Run the bundled switch validator with the current state, selected matrix, and available capabilities. Refuse a switch that changes the root, loses work references, needs an unavailable capability, or leaves overlapping active writers. Close or transfer workers only through the root.

For governed work, persist the selected matrix and transition record before continuing. For an ad-hoc switch, record the complete matrix and rationale in the current casefile. Resume from preserved work; do not restart accepted work or rewrite historical records.

Load `casefile-workflow/scripts/switch-strategy.py` when validating or applying a transition.
