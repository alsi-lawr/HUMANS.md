---
name: casefile-review
description:
  "Use to review proposed Casefile tickets before implementation using independent,
  adversarial-dialogue, or primary-plus-verification review."
---

# Casefile Review

Load `casefile` and validate the ticket set. When choosing a review strategy, present compatible
options with a recommendation and wait for explicit selection:

- [Atomic](references/atomic.md) for independent disjoint groups.
- [Dialogue](references/dialogue.md) for chair-and-challenger reconciliation.
- [Two stage](references/two-stage.md) for primary review plus focused verification.

For a governed Codex review, persist the exact selected matrix through the strategy transition.
Follow the role assignment flow in `casefile` before delegation, including on re-entry. Apply its
delegation check each time a root or nested caller assigns work to a Codex reviewer.

Reviewers write evidence only. The root reconciles duplicates and decisions, routes corrections to
the original owner, and escalates unresolved contention to the human.
