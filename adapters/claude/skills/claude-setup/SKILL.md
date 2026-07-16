---
name: claude-setup
description: "Use after installing humans-md to validate and inspect the Claude package without changing user configuration or claiming runtime behaviour."
---

# Claude Setup

Resolve resources beneath `${CLAUDE_PLUGIN_ROOT}` and run `claude plugin validate ${CLAUDE_PLUGIN_ROOT} --strict`. Inspect the root `skills/`, `agents/`, `casefile-workflow/`, and `verification/` components; only `plugin.json` belongs under `.claude-plugin`.

Do not install, enable, or behaviourally exercise the package without separate authority. Structural validation does not prove loading, triggering, model routing, effort routing, or Casefile behaviour.
