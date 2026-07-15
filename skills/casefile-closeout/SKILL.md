---
name: casefile-closeout
description: "Use when resolved Casefile, planning, orchestration, or review artifacts in task scratch should be promoted through a configured durable-store adapter. Do not use for cleanup, active work, implementation, or forge-issue conversion."
---

# Casefile Closeout

1. Read repository authority and inventory task scratch.
2. Classify every artifact as disposable, active, or durable. Promote only resolved current-session material.
3. Resolve the configured store, mandatory `projects.toml`, project namespace, and persistence adapter. Require the project-to-absolute-source mapping before promotion; never infer or silently replace it. Run the bundled project-map validator after map and namespace changes.
4. Preserve selected artifacts and provenance without normalising history.
5. Compare source and destination file lists, modes, and hashes; run bundled validators.
6. Synchronise through the selected adapter and independently verify durability.
7. Delete source copies only after verification. Retain active, unresolved, secret-bearing, failed, or unselected material.
8. Report promoted and retained paths, destination identity, checks, and inherited defects.

Load the persistence adapter's contribution skill before synchronising. Never create forge issues or claim archived plans describe current behaviour.
