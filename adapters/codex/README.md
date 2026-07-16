# Codex adapter

The generated Codex plugin binds the portable Casefile workflow to Sol/xhigh
at the root and matrix-specific Terra profiles for delegated roles.

After marketplace installation, invoke `codex-setup`. The skill only runs
`scripts/setup-codex.py`: first as a preview, then once with `--apply` after
human approval. The script owns catalog export, allowlisted model profiling,
configuration, the global contract, legacy removal, backup, rollback, and
mechanical verification. It does not ask an agent to construct or edit a
cutover plan.

The generated `models-humans-md-v1.json` is essential. Codex 0.144.1 bundles
Sol and Terra with `multi_agent_version = "v2"`; feature flags alone do not
replace those selectors. Setup points `model_catalog_json` at a preserved copy
of the fresh bundled catalog with the declared Sol and Terra selectors set to
JSON null. It never reads or writes `models_cache.json`.

Successful setup records an immutable receipt beneath
`~/.codex/backups/humans-md/` and a hash-bound active pointer beneath
`~/.codex/state/humans-md/`. Invoke `codex-uninstall` to restore that receipt
and remove the plugin and marketplace transactionally. Setup-owned config is
kept in two hash-bound marked blocks. Uninstall removes only those blocks,
preserves unrelated later config edits, and refuses changes inside owned
configuration.

Fully restart the Codex host and start a new root thread after setup. Exact
fresh-process child model, effort, and API behaviour remain runtime checks;
deterministic package and configuration checks do not substitute for them.
