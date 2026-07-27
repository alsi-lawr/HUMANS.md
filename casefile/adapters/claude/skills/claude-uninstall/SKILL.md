---
name: casefile-claude-uninstall
description: Remove only the receipt-owned Casefile MCP binding and executable from Claude Code.
---

# Casefile Claude uninstall

Run `${CLAUDE_PLUGIN_ROOT}/scripts/setup-claude.py uninstall`, show its complete preview, and obtain
explicit approval before repeating with `--apply`. Refuse if the active binding differs from the
receipt; preserve unrelated MCP servers, plugins, marketplaces, and configuration.
