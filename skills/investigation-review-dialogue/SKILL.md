---
name: investigation-review-dialogue
description: "Use when the human selects adversarial parent-child dialogue review for investigation tickets. Requires nested subagents; route disjoint parallel review to investigation-review-atomic."
---


    # Investigation Review: Dialogue

    Validate the selected review matrix and nesting support. Spawn one chair; the chair spawns one challenger. Both inspect independently, then conduct at most two focused reconciliation rounds. The chair records review evidence and returns joint verdict, agreed corrections, remaining contentions, evidence, and affected ticket IDs. The root routes corrections and human escalation; neither reviewer edits tickets or source.
