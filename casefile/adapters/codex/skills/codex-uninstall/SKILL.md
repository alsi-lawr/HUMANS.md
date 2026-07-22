---
name: casefile-codex-uninstall
description:
  "Use to restore the Casefile Codex integration receipt and remove only the casefile plugin."
---

# Casefile Codex uninstall

Preview first, then after approval run
`${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py uninstall --apply`. It restores Casefile-owned config
and catalog state, removes only `casefile@humans-md`, and preserves the marketplace and sibling
plugins.
