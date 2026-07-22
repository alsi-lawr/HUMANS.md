---
name: codex-uninstall
description:
  "Use to restore the AGENTS.md state saved by codex-setup and remove only the humans-md Codex
  plugin."
---

# Codex uninstall

Run `${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py uninstall` to preview the active receipt, then ask
once before adding `--apply`. It restores only the core contract receipt and removes
`humans-md@humans-md`. It never removes the shared humans-md marketplace or any sibling plugin.
