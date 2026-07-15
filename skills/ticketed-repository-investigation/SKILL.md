---
name: ticketed-repository-investigation
description: "Use when a human asks to investigate a repository and preserve findings as governed planning tickets. Coordinates strategy selection, authorised ticket creation, delegated review, contention escalation, and implementation-plan handoff; route ordinary code review or direct implementation elsewhere."
---

# Ticketed Repository Investigation

Assume the root request-receiving agent is the orchestrator; never spawn a replacement orchestrator. Read repository authority and current state, define the investigation boundary, and create the investigation layout from `planning-workflow/schemas/investigation-layout.md`.

## Resolve strategies

Require an explicit investigation strategy and later an explicit review strategy. If absent, enumerate installed compatible strategy skills, filter unsupported capabilities, recommend based on scope, and ask the human to select; never choose. Resolve a platform preset, ad-hoc matrix, or human-approved override and copy the exact TOML into `strategy/`. Validate it before delegation.

Verify authorised workers can share writable planning storage while source remains read-only to them. If canonical storage is outside the workspace, use the adapter's task-local mirror. Stop before direct-ticket delegation when shared storage is unavailable.

## Investigate and ticket

Apply the selected strategy. Every detective reports a candidate before writing. Arbitrate uniqueness: merge the same defect under one owner; cross-link behaviourally distinct findings; seek narrower evidence for unclear collisions. Reserve an ID and exact provisional path only after arbitration. The authorised investigator writes only that path using the ticket schema.

Verify evidence and move each ticket to accepted or rejected. Rejected tickets retain resolved rationale. Log human constraints at the narrowest correct decision scope.

## Review and resolve

Delegate all substantive review through the selected review strategy. Reviewers write under `review/round-XXX/` and never edit source or tickets. Route obvious bounded corrections back to an investigator and return the ticket to provisional. Escalate every non-obvious contention through the adapter's human-question mechanism with interpretations, evidence, consequences, and concrete options; log the answer before disposition.

Do not close while provisional tickets exist. Verify rejected rationale, commit and synchronise reviewed planning state, then ask the human to select a compatible implementation matrix. Enter the adapter's non-mutating planning phase, use accepted tickets only, group dependency-safe phases with exclusive ownership, embed the exact implementation matrix and review flow, and return the plan for acceptance. Persist it only after returning to a mutating phase. A future root executes that accepted strategy without redesigning it.
