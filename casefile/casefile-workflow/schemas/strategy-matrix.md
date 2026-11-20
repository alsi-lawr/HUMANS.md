# Strategy Matrix Schema

Every selected strategy is copied unchanged into its casefile. Presets are choices, never defaults.

Required root keys are `schema_version = 1`, `strategy_id`, `phase`, and `adapter`. Supported phases
are planning, investigation, review, implementation, and closeout. `[orchestrator].binding` is
`root`; this preserves the request-receiving agent's authority and never selects its model or
effort. `[limits]` declares positive concurrency and non-negative nesting depth.
`[requirements].capabilities` lists runtime capabilities that must already be available. Each worker
declares role, adapter profile, minimum and maximum count, and whether it may spawn children.
Shipped Codex matrices use named behavioral profiles with model and effort supplied by a confirmed
strategy assignment. The parser still reads model and effort fields in historical matrices.

`[coordination]` records batching, candidate review, and shared-store requirements. An
implementation pipeline adds `[coordination.pipeline]` with a positive active-ticket limit and
boolean read-only look-ahead, dependency-independence, disjoint-write-path, immutable-review-commit,
and correction-preemption gates. Absence of that table means no implementation overlap is
authorised, including when the matrix declares a read-only look-ahead worker. Worker minima cannot
exceed capacity. A spawning worker needs nesting depth of at least two. Validate the complete
selected matrix before delegation and after every strategy switch.
