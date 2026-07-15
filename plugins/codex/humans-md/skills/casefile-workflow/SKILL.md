---
name: casefile-workflow
description: "Use when a human asks to investigate a repository and preserve findings as governed Casefile tickets. Coordinates explicit strategy selection, authorised ticket creation, review, contention, and implementation-plan handoff; route ordinary review or direct implementation elsewhere."
---

# Casefile Workflow

The request-receiving root is the orchestrator; never spawn a replacement. Read repository authority and current state, bound the investigation, then load the project-map, investigation-layout, ticket, decision, and strategy schemas from the resolved Casefile package.

Resolve the planning store's mandatory `projects.toml`. Before adding a project namespace, map its name to its absolute source directory and validate the map. Never infer a source path or overwrite a conflict. Create the investigation only after the mapping is valid.

## Select and run

Require an explicit compatible strategy for every governed phase. Enumerate available presets and recorded ad-hoc matrices, filter unavailable capabilities, recommend from the stated scope, and ask the human to select; never choose. Copy the exact selected matrix into the casefile and validate it before delegation.

Keep delegated source access read-only during investigation. Require shared writable planning storage before multi-worker strategies. Every investigator reports a candidate before writing. The root arbitrates uniqueness, reserves one ID and path, verifies evidence, and alone disposes tickets. Merge the same defect; cross-link distinct behaviour. Rejected tickets retain rationale and decision references.

## Review and hand off

Apply the selected review flow to every ticket. Reviewers write evidence, not source or tickets. Route bounded corrections to the original owner. Escalate non-obvious contention with interpretations, evidence, consequences, and concrete options; record the human decision before disposition.

Do not close with provisional tickets. Verify accepted and rejected records, then require an explicit implementation matrix. Build a dependency-safe plan from accepted tickets, assign one writer to overlapping paths, preserve the exact review flow, and return it for human acceptance. A later root executes that plan without redesign.
