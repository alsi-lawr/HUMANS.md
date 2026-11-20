---
name: casefile-investigate
description:
  "Use to investigate an uncertain repository problem and turn evidence into human-approved Casefile
  tickets using solo, parallel, or inspector-led work."
---

# Casefile Investigate

Load `casefile` and the strategy matrix. When choosing an investigation strategy, present the
compatible options, recommend one from scope and capabilities, and wait for explicit selection:

- [Solo](references/solo.md) for narrow inseparable work.
- [Atomic](references/atomic.md) for disjoint parallel questions.
- [Inspector tree](references/inspector-tree.md) for bounded hierarchical decomposition.

For a governed Codex investigation, persist the exact selected matrix through the strategy
transition. Follow the role assignment flow in `casefile` before delegation, including on re-entry.
Apply its delegation check each time a root or nested caller assigns work to a Codex worker.

Investigators report candidates before ticket creation. The root verifies evidence, arbitrates
uniqueness, reserves IDs and paths, and records accepted or rejected disposition.
