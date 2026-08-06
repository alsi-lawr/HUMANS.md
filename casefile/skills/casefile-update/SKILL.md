---
name: casefile-update
description:
  "Use to reinstall Casefile over an existing installation, keeping the recovery state captured
  before Casefile was first installed."
---

# Casefile update

Reinstall Casefile over its own installed state. Use this to re-apply a setup after changing the
planning root or repairing a partial install.

Claude refuses to reinstall an installed version unless `--overwrite` is passed:

```
${CLAUDE_PLUGIN_ROOT}/scripts/setup-claude.py install \
  --plugin-root ${CLAUDE_PLUGIN_ROOT} \
  --planning-root <absolute-root> \
  --overwrite
```

Codex reinstalls over an active receipt without a flag:

```
${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install \
  --plugin-root ${CODEX_PLUGIN_ROOT} \
  --planning-root <absolute-root>
```

Show the complete preview and take explicit approval before repeating the command with `--apply`.
Require one absolute planning root; never infer it from the previous receipt without confirming it.

## Recovery state

The receipt written by this operation carries forward the pre-Casefile backup recorded by the first
install. Casefile's own installed state is not backed up: restoring it is never the goal, and
capturing it would overwrite the only record of the host's original configuration.

An install with no active receipt is a first install and captures the pre-Casefile state as
normal.

## Refusals

Do not use `--overwrite` to take ownership of a Claude installation this plugin does not own.
Claude setup still refuses an unowned MCP server or a binding that differs from its receipt, and
those refusals are correct: resolve them by hand. Codex setup owns its config keys and tables
outright and overwrites them on every install.

Restart the host after a successful receipt-backed update.
