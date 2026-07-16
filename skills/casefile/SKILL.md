---
name: casefile
description: "Use to start or resume substantial repository work that needs traceable investigation, human decisions, reviewed tickets, implementation, and evidence."
---

# Casefile

The request-receiving root remains the orchestrator. Read repository authority and current state, bound the work, resolve the configured planning store, and validate its project-to-source mapping. Never infer a source path or replace the root.

Route the current phase to `casefile-investigate`, `casefile-review`, `casefile-implement`, or `casefile-close`. Every governed phase requires an explicit compatible strategy. Present compatible choices and a recommendation, then wait for human selection.

Keep investigation source access read-only. The root alone arbitrates duplicate findings, reserves ticket IDs and paths, records decisions, and disposes tickets. Reviewers write evidence only. Implementation begins only from accepted tickets and an approved dependency-safe plan. Preserve rejected rationale, contention, verification, and unresolved risk through closeout.
