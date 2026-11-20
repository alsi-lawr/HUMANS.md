---
name: casefile-codex-uninstall
description:
  "Use to restore the Casefile Codex integration receipt and remove only the casefile plugin."
---

# Casefile Codex uninstall

Preview first, then after approval run
`${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py uninstall --apply`. It removes the Casefile integration
and plugin and preserves the marketplace, sibling plugins and host configuration. For a legacy
install, review any receipt-owned catalog recovery in the preview.
