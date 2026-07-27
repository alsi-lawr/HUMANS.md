# humans-md marketplace

This generated marketplace contains synchronized packages: `humans-md` (standing contract
lifecycle), `casefile` (governed workflow and optional Codex integration), and `coding` (reusable
coding guidance).

Install `humans-md` when a managed standing contract is wanted. `casefile` and `coding` are optional
siblings. Each plugin can be installed or removed independently; no plugin lifecycle removes this
marketplace.

This repository is generated. Release tags contain installable trees; source and contribution
history belong in the source repository.

The Casefile package declares a fixed-root local stdio MCP server. Supply one absolute activated
planning Store with `CASEFILE_PLANNING_ROOT`; the default source-coherent launcher requires Cargo,
Rust, and an available dependency cache or network because it runs the packaged lockfile with
`cargo run --locked`. A compatible external executable is accepted only as an explicit override.
