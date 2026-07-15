---
name: ticket-batch-subagent-pipeline
description: "Use when implementing an accepted ticket batch under an already selected platform strategy matrix with task-scoped writers, matrix-declared look-ahead workers, and recorded review flow. Do not choose models, reasoning, or strategy defaults."
---


    # Ticket-Batch Subagent Pipeline

    Require the accepted implementation plan and its exact selected matrix. Validate every worker binding and the recorded review flow before delegation; if absent or incomplete, ask the human rather than inventing values.

    The root remains orchestrator across batches and owns scope, dependency order, exclusive write ownership, acceptance, correction routing, and synthesis. Instantiate only matrix-declared workers. Give overlapping mutations to one task-scoped writer per batch; all other workers are read-only unless the matrix explicitly grants a disjoint write path.

    The writer returns immutable commits and focused evidence. Apply the plan's review stages to every ticket and correction. Verification review checks acceptance and concrete primary findings rather than repeating the full review. Corrections return to the same writer in dependency-safe order. A ticket completes only after its recorded review flow accepts it; close batch workers before the next batch.
