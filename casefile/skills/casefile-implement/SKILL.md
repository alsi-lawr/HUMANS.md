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
every declared review stage. Before routing a finding, classify it as a correction, contention, or
follow-up. Reviewers propose a class with evidence; the root has final classification authority:

- A correction is an explicit violation of the accepted contract and returns to the same writer in
  dependency order.
- A contention would introduce new architecture, durable state, a dependency, a failure guarantee, a
  compatibility promise, public behavior, or a material path expansion. The same semantic concern
  surviving one correction is also a contention.
- A follow-up is optional hardening that does not block acceptance.

Do not route a contention to a writer as though it were accepted scope. Stop ticket-batch work or
drain a pipeline to serial state, present the evidence and concrete choice to the human, and resume
mutation only after the human rejects the expansion or amends the governing decision or ticket.
Complete a ticket only after the recorded flow accepts it. See the brief
[correction-escalation case study](references/correction-escalation-case-study.md).

For Codex, immediately before every implementation-writer spawn, first successfully transition the
applicable ticket to canonical `in_progress`, then run the installed
`scripts/resolve-writer-binding.py resolve` with the planning root, active investigation, exact
selected implementation strategy ID, and exact ticket ID. The resolver independently requires that
current progress before returning a spawn. This applies equally to the first ticket, later batches,
pipeline overlap, resume, and every correction round. Delegate with exactly the returned `spawn`
object: V1 returns a named agent type; V2 returns a model-free role plus explicit model, reasoning,
and bounded history override. Never reuse an earlier resolution without revalidation.

If resolution says the persisted or matrix-derived pair is invalid or unavailable, stop before
delegation and before any planning/source mutation. Run `offer`, present its complete current list,
state when Sol/high is unavailable, and request a new explicit selection. Replace the binding with
`select`; canonical Store progress derives whether implementation or correction work is inactive and
fails closed rather than accepting a caller assertion. Never substitute Sol/high or another pair
silently. A missing binding in a historical Casefile is not an error: the resolver returns the
selected matrix writer default after checking its current availability.

Keep per-ticket review and verification focused on that ticket's acceptance criteria, changed
surfaces, and concrete findings. Reserve full workspace, package, and authenticated gates for the
release candidate unless the ticket explicitly owns one of those surfaces.
