---
name: casefile-investigate-inspector-tree
description: "Use when the human selects hierarchical Casefile investigation with inspectors delegating bounded work. Requires nested workers and shared writable planning storage; do not offer it without both."
---

# Casefile Investigate: Inspector Tree

Load `casefile-workflow`. Validate depth, concurrency, worker counts, profiles, and shared writable planning storage. Give each inspector a disjoint domain and authorise bounded decomposition. Investigators report candidates to inspectors; inspectors verify evidence and recommend disposition. The root retains ticket-path reservation, cross-domain duplicate authority, and final disposition.
