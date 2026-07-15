---
name: casefile-codex-setup
description: "Use when a human explicitly asks to prepare an opt-in Codex Casefile configuration candidate from an installed humans-md plugin. Does not edit active configuration or perform cutover."
---

# Casefile Codex Setup

Require the installed plugin root and a repository-local or temporary output directory. Run `${CODEX_PLUGIN_ROOT}/scripts/prepare-setup.py` without `--apply`, review every rendered absolute profile path and feature flag, then rerun with `--apply` only to create candidate files.

Before active cutover, prepare a complete reviewed configuration and load `casefile-codex-cutover`. Its explicit plan must inventory direct skills, agents, workflow resources, active configuration, and marketplace state; preserve unrelated state and the global contract; run strict configuration and marketplace discovery; restart the host into fresh probes; and prove V1 plus exact root and inspector bindings before removing named superseded copies. Never edit `models_cache.json` and never treat package validation as live cutover evidence.
