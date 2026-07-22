---
name: casefile-implement
description:
  "Use to implement approved Casefile tickets through a human-selected serial or bounded pipeline
  strategy with exclusive writes and recorded review."
---

# Casefile Implement

Require the accepted dependency-safe plan. Present the compatible implementation strategies,
recommend one from ticket independence and runtime capabilities, and wait for explicit selection:

- [Ticket batch](references/ticket-batch.md) for serial implementation and review.
- [Pipeline](references/pipeline.md) for bounded look-ahead and overlap of one independent next
  ticket with review.

Persist and validate the exact selected matrix before delegation. The root owns scope, dependency
order, exclusive write ownership, acceptance, correction routing, and synthesis. Assign overlapping
mutations to one writer. Writers return an immutable commit per ticket and focused evidence. Apply
every declared review stage; return corrections to the same writer in dependency order. Complete a
ticket only after the recorded flow accepts it.

For Codex, immediately before every implementation-writer spawn, run the installed
`scripts/resolve-writer-binding.py resolve` with the planning root, active investigation, and exact
selected implementation strategy ID. This applies equally to the first ticket, later batches,
pipeline overlap, resume, and every correction round. Delegate with exactly the returned `spawn`
object: V1 returns a named agent type; V2 returns a model-free role plus explicit model, reasoning,
and bounded history override. Never reuse an earlier resolution without revalidation.

If resolution says the persisted or matrix-derived pair is invalid or unavailable, stop before
delegation and before any planning/source mutation. Run `offer`, present its complete current list,
state when Sol/high is unavailable, and request a new explicit selection. Replace the binding with
`select --implementation-active false` only after confirming no implementation writer or correction
is active. Never substitute Sol/high or another pair silently. A missing binding in a historical
Casefile is not an error: the resolver returns the selected matrix writer default after checking its
current availability.
