---
name: claude-setup
description: "Use immediately after installing humans-md for Claude to preview and install the global CLAUDE.md contract with durable recovery of the previous file."
---

# Claude Setup

Validate `${CLAUDE_PLUGIN_ROOT}` with `claude plugin validate ${CLAUDE_PLUGIN_ROOT} --strict`. Then resolve the configuration directory as `${CLAUDE_CONFIG_DIR}` when set, otherwise `~/.claude`; the destination is `<config>/CLAUDE.md`, the source is `${CLAUDE_PLUGIN_ROOT}/templates/AGENTS.md`, and the installer is `${CLAUDE_PLUGIN_ROOT}/scripts/bootstrap-contract.py`.

Refuse to replace a symbolic-link destination or to continue when `<config>/backups/humans-md/claude` already exists. Run the installer with an available Python 3 interpreter without `--apply`, show the complete diff, and ask once before changing anything. If it reports no changes, do not create a receipt or claim ownership of the existing file.

After approval, rerun the preview and stop if the reviewed diff changed. Create `<config>/backups/humans-md/claude`; atomically move an existing destination to `CLAUDE.md.before`, or create `CLAUDE.md.was-missing` when no destination existed. Run the installer with `--apply` against the now-missing destination. Verify that source and destination bytes match.

If installation fails, remove any partial destination, restore `CLAUDE.md.before` when present or preserve the prior absence, verify the rollback, and only then remove the receipt directory. On success, retain the receipt for `claude-uninstall` and tell the human to restart Claude Code.
