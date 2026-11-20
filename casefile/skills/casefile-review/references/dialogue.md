# Dialogue review

Require the `casefile-review-dialogue` matrix and nested-worker support. Spawn one chair; the chair
spawns the authorised challenger. Both inspect independently and conduct at most two focused
reconciliation rounds. Record the joint verdict, agreed corrections, unresolved contentions,
evidence, and ticket IDs. Reviewers edit neither source nor tickets.

For Codex, the root runs the delegation check in `casefile` for the chair. Pass the exact strategy
target and authorized challenger role. The chair repeats the check from its own session before
asking the challenger to review and reports an unavailable launch to the root.
