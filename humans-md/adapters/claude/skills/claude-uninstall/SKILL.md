---
name: claude-uninstall
description:
  "Use to restore the versioned CLAUDE.md state saved by claude-setup and remove only the humans-md
  Claude plugin."
---

# Claude uninstall

Read the active `claude-v0.2.0` receipt, preview a complete `git diff --no-index` between
`${CLAUDE_CONFIG_DIR:-~/.claude}/CLAUDE.md` and its recorded prior state, and ask once before
restoring it. Then remove only `humans-md@humans-md --scope user`. Never remove the shared
marketplace or sibling plugins. Legacy `0.1.5` recovery remains the exclusive job of `migrations`.
