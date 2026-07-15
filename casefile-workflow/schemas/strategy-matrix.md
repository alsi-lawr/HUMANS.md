# Strategy Matrix Schema

Every selected strategy is copied unchanged into its casefile. Presets are choices, never defaults.

Required root keys are `schema_version = 1`, `strategy_id`, `phase`, and `adapter`. Supported phases are planning, investigation, review, implementation, and closeout. `[orchestrator].binding` is `root`. `[limits]` declares positive concurrency and non-negative nesting depth. `[requirements].capabilities` lists runtime capabilities that must already be available. Each worker declares role, adapter profile, minimum and maximum count, and whether it may spawn children. Adapter matrices may add exact runtime bindings; the portable workflow never supplies them.

`[coordination]` records batching, candidate review, and shared-store requirements. Worker minima cannot exceed capacity. A spawning worker needs nesting depth of at least two. Validate the complete selected matrix before delegation and after every strategy switch.
