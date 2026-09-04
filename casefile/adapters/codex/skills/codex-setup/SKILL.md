---
name: casefile-codex-setup
description:
  "Use after installing casefile for Codex to preview and install Casefile model, multi-agent
  runtime, profile, and role integration."
---

# Casefile Codex setup

Require one explicit absolute activated planning Store root. Run
`${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install --plugin-root ${CODEX_PLUGIN_ROOT} --planning-root <absolute-root>`
without `--apply`, review the plan, then ask once before applying. Omit `--multi-agent-version` for
the V1 default, or pass `--multi-agent-version v2`. This lifecycle owns only Casefile model-catalog,
selected multi-agent feature, profile, and role configuration; it never installs or replaces
`AGENTS.md`. Codex discovery only confirms required-model availability. It never supplies or alters
the Casefile-owned replacement catalog, and projected IDs outside that catalog do not block setup.

For an existing integration that only needs the current catalog and profile registrations, use
`casefile-codex-model-migrate`. It preserves the selected runtime and MCP binding rather than
reinstalling them.
