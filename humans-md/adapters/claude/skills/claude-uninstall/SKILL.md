---
name: claude-uninstall
description: "Use to restore the CLAUDE.md state saved by claude-setup and remove only the humans-md Claude plugin."
---

# Claude uninstall

Preview a complete `git diff --no-index` between `${CLAUDE_CONFIG_DIR:-~/.claude}/CLAUDE.md` and the core receipt, ask once, then restore the recorded state and remove only `humans-md@humans-md --scope user`. Never remove the shared marketplace or sibling plugins.
