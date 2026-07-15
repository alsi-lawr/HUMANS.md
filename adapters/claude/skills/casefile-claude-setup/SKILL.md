---
name: casefile-claude-setup
description: "Use when a human asks how to validate or inspect the packaged humans-md Casefile plugin for Claude. Does not install the plugin or alter user configuration."
---

# Casefile Claude Setup

Resolve all package resources beneath `${CLAUDE_PLUGIN_ROOT}`. Validate with `claude plugin validate ${CLAUDE_PLUGIN_ROOT} --strict`. Inspect the root `skills/`, `agents/`, `casefile-workflow/`, and `verification/` components; only `plugin.json` belongs under `.claude-plugin`.

Do not install, enable, or behaviourally exercise the package without separate human authority. Structural validation does not prove loading, triggering, model routing, effort routing, or Casefile behaviour.
