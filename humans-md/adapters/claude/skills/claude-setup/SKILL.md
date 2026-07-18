---
name: claude-setup
description: "Use immediately after installing humans-md for Claude to preview and install the global CLAUDE.md standing contract with durable recovery."
---

# Claude setup

Validate `${CLAUDE_PLUGIN_ROOT}` with `claude plugin validate ${CLAUDE_PLUGIN_ROOT} --strict`. Preview `${CLAUDE_PLUGIN_ROOT}/scripts/bootstrap-contract.py` against `${CLAUDE_CONFIG_DIR:-~/.claude}/CLAUDE.md`, show the complete diff, and ask once before replacement. Keep the exact prior state at `<config>/backups/humans-md/claude`. This setup owns only the standing contract; it does not install or configure Casefile.
