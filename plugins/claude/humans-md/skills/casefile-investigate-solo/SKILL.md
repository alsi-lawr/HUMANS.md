---
name: casefile-investigate-solo
description: "Use when the human selects root-only Casefile investigation for a narrow, inseparable scope. Do not use as an implicit fallback or for delegated investigation."
---

# Casefile Investigate: Solo

Load `casefile-workflow`. Require and persist an explicit compatible matrix even though it has no workers. The root investigates read-only, arbitrates duplicates, reserves IDs and paths, authors provisional tickets, verifies evidence, and disposes them. Spawn no worker.
