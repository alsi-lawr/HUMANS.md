---
name: casefile-codex-setup
description: "Use after installing casefile for Codex to preview and install Casefile model, V1, profile, and role integration."
---

# Casefile Codex setup

Run `${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install --plugin-root ${CODEX_PLUGIN_ROOT}` without `--apply`, review the plan, then ask once before applying. This lifecycle owns only Casefile model-catalog, V1 feature, profile, and role configuration. It never installs or replaces `AGENTS.md`.
