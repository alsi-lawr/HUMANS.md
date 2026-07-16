# Codex adapter

The agent that receives the request remains the orchestrator with its existing
model and reasoning effort. The generated Codex plugin does not select or
replace either value. It binds only delegated Casefile roles to matrix-specific
profiles: Terra for implementation and review, and Luna only for the optional
read-only pipeline look-ahead.

After marketplace installation, invoke `codex-setup`. The skill only runs
`scripts/setup-codex.py`: first as a preview, then once with `--apply` after
human approval. The script owns catalog export, allowlisted model profiling,
configuration, the global contract, legacy removal, backup, rollback, and
mechanical verification.

The generated `models-humans-md-v1.json` is essential. Codex 0.144.1 bundles
Sol, Terra, and Luna with `multi_agent_version = "v2"`; feature flags alone do
not replace those selectors. Setup points `model_catalog_json` at a preserved
copy of the active catalog with the declared Sol, Terra, and Luna
selectors set to JSON null. It never reads or writes `models_cache.json`.

Successful setup records a durable receipt beneath `~/.codex/backups/humans-md/`
and an active pointer beneath `~/.codex/state/humans-md/`. Invoke
`codex-uninstall` to restore that receipt and remove the plugin and marketplace
transactionally. Setup-owned config is kept in two marked blocks. Uninstall
removes only those blocks, preserves unrelated later config edits, and shows a
per-file Git diff before replacing modified managed files.

Fully restart the Codex host and start a new root thread after setup. Exact
fresh-process child model, effort, and API behaviour remain runtime checks;
deterministic package and configuration checks do not substitute for them.
