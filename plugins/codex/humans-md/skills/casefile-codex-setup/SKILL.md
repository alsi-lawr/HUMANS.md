---
name: casefile-codex-setup
description: "Use when a human explicitly asks to prepare an opt-in Codex Casefile configuration candidate from an installed humans-md plugin. Does not edit active configuration or perform cutover."
---

# Casefile Codex Setup

Require the installed plugin root and a repository-local or temporary output directory. Run `${CODEX_PLUGIN_ROOT}/scripts/prepare-setup.py` without `--apply`, review every rendered absolute profile path and feature flag, then rerun with `--apply` only to create candidate files.

Before active cutover, separately back up direct skills, agents, workflow resources, and relevant configuration. Merge only the generated declarations, preserve unrelated state and the global contract, run strict configuration and marketplace discovery, restart the host, and open a new root thread. Prove V1 plus exact root and child profile bindings before removing only superseded direct copies. Restore on any failure. Never edit `models_cache.json` and never treat package validation as live cutover evidence.
