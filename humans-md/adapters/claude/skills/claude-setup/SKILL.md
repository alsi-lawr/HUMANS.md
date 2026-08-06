---
name: claude-setup
description:
  "Use immediately after installing humans-md for Claude to preview and install the global CLAUDE.md
  standing contract and its settings.json keys with a versioned recovery receipt."
---

# Claude setup

Run `${CLAUDE_PLUGIN_ROOT}/scripts/setup-claude.py --plugin-root ${CLAUDE_PLUGIN_ROOT}`. Review its
focused preview and record `approval_fingerprint`; after one approval rerun with
`--apply --approval <fingerprint>`. The script rechecks both managed targets immediately before
replacement and refuses stale approval.

This setup owns two targets: the standing contract at `<config>/CLAUDE.md`, and the contract keys
it manages inside `<config>/settings.json`. Surface `settings_plan` from the preview before
approval: each entry reports the contract value against the value already on disk, and any entry
with `present: true` whose `current` differs is an operator setting the install will overwrite.
Unrelated keys are preserved, including sibling keys under a managed parent.

Recovery state is recorded beneath `<config>/backups/humans-md/claude-v0.2.0/` with an active
pointer at `<config>/state/humans-md/claude-v0.2.0.json`. The receipt records `settings_before` as
prior values keyed by dotted path, using `null` for a key that did not exist. This setup does not
install or configure Casefile.
