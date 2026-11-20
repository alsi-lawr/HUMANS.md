---
name: casefile-implement
description:
  "Use to implement approved Casefile tickets through a human-selected serial, look-ahead, or
  bounded pipeline strategy with exclusive writes and recorded review."
---

# Casefile Implement

Load `casefile` and require the accepted dependency-safe plan. When choosing an implementation
strategy, present the compatible options, recommend one from ticket independence and runtime
capabilities, and wait for explicit selection:

- [Ticket batch](references/ticket-batch.md) for serial implementation and review without
  speculative next-ticket work.
- [Ticket batch with look-ahead](references/ticket-batch-look-ahead.md) for serial implementation
  with one read-only preflight of the next accepted ticket.
- [Pipeline](references/pipeline.md) for bounded look-ahead and overlap of one independent next
  ticket with review.

Persist and validate the exact selected matrix before delegation. For Codex, follow the role
assignment flow in `casefile` before ticket progress, source mutation, or delegation, including on
re-entry. The root owns scope, dependency order, exclusive write ownership, acceptance, correction
routing, and synthesis. Assign overlapping mutations to one writer. Writers return an immutable
commit per ticket and focused evidence. Apply every declared review stage. Before routing a finding,
classify it as a correction, contention, or follow-up. Reviewers propose a class with evidence; the
root has final classification authority:

- A correction is an explicit violation of the accepted contract and returns to the same writer in
  dependency order.
- A contention would introduce new architecture, durable state, a dependency, a failure guarantee, a
  compatibility promise, public behavior, or a material path expansion. The same semantic concern
  surviving one correction is also a contention.
- A follow-up is optional hardening that does not block acceptance.

Do not route a contention to a writer as though it were accepted scope. Stop batch work or drain a
pipeline to serial state, present the evidence and concrete choice to the human, and resume mutation
only after the human rejects the expansion or amends the governing decision or ticket. Complete a
ticket only after the recorded flow accepts it. See the brief
[correction-escalation case study](references/correction-escalation-case-study.md).

For each Codex writer, reviewer, verifier, or look-ahead assignment, follow the delegation check in
`casefile` immediately before calling the worker. An implementation writer also needs canonical
`in_progress` state for the exact accepted ticket. Apply the same checks on resume and correction.
When a saved pair becomes unavailable, obtain the human's replacement through the role assignment
flow and rerun the delegation check before relaunch.

Keep per-ticket review and verification focused on that ticket's acceptance criteria, changed
surfaces, and concrete findings. Reserve full workspace, package, and authenticated gates for the
release candidate unless the ticket explicitly owns one of those surfaces.
