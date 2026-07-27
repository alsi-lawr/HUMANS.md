---
name: casefile-claude-setup
description:
  Install the bundled Casefile MCP executable and bind one explicit planning Store in Claude Code.
---

# Casefile Claude setup

Require one explicit absolute activated planning Store root. Run
`${CLAUDE_PLUGIN_ROOT}/scripts/setup-claude.py install --plugin-root ${CLAUDE_PLUGIN_ROOT} --planning-root <absolute-root>`
and show the complete preview. Only after explicit approval repeat with `--apply`. Restart Claude
Code after a successful receipt-backed install.
