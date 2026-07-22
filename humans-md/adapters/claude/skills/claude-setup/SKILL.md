---
name: claude-setup
description:
  "Use immediately after installing humans-md for Claude to preview and install the global CLAUDE.md
  standing contract with a versioned recovery receipt."
---

# Claude setup

Run `${CLAUDE_PLUGIN_ROOT}/scripts/setup-claude.py --plugin-root ${CLAUDE_PLUGIN_ROOT}`. Review its
focused preview and record `approval_fingerprint`; after one approval rerun with
`--apply --approval <fingerprint>`. The script rechecks `CLAUDE.md` immediately before replacement
and refuses stale approval.

Recovery state is recorded beneath `<config>/backups/humans-md/claude-v0.2.0/` with an active
pointer at `<config>/state/humans-md/claude-v0.2.0.json`. This setup owns only the standing
contract; it does not install or configure Casefile.
