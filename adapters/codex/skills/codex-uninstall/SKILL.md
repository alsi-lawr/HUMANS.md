---
name: codex-uninstall
description: "Use to remove an installed humans-md Codex setup and restore its durable pre-install receipt with rollback protection."
---

# Codex Uninstall

Run `${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py uninstall` without `--apply`. Show the concise JSON preview and any per-file Git diff for modified managed files, then ask once for approval. The script resolves the active receipt from `$CODEX_HOME/state/humans-md/current.json` (default `~/.codex`); never select a backup by recency.

After approval, rerun with `--apply`. The script removes only its marked configuration blocks, preserves unrelated later configuration changes, and restores the durable backup with rollback on failure. If it fails, say only: "Uninstall failed. Do you want me to debug it?" Do not investigate unless the human agrees.
