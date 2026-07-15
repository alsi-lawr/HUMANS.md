# Strategy Matrix Schema

Every selected strategy is copied into its investigation as TOML. Presets are choices, never defaults.

Required root keys are `schema_version = 1`, `strategy_id`, `phase`, and `platform`. `[orchestrator].binding` must be `root`. `[limits]` requires positive `max_concurrent_subagents` and non-negative `max_depth`. Each `[[workers]]` requires portable `role`, `platform_profile`, exact `model`, exact `reasoning`, positive `minimum_count` and `maximum_count`, and `can_spawn_subagents`; minimum must not exceed maximum. `[coordination]` requires `batch_when_capacity_exceeded`, `candidate_review_before_ticket`, and `shared_ticket_storage_required`.

The portable core never supplies platform-field values. An adapter validates model and reasoning identifiers, role existence, count/depth/concurrency, nesting, shared storage, the human-question mechanism, and planning-mode capability. Worker maxima describe available assignment slots; the concurrency limit is the number simultaneously open. Nested strategies require depth at least two and at least one spawning worker.
