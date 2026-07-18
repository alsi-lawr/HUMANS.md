---
name: codex-setup
description: "Use immediately after installing humans-md for Codex to preview and install only the global AGENTS.md contract with durable recovery."
---

# Codex setup

Run `${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install --plugin-root ${CODEX_PLUGIN_ROOT}` first. Show its preview and ask once before rerunning with `--apply`. It installs only the packaged `AGENTS.md` contract and records the prior state beneath `~/.codex/backups/humans-md/`. It does not configure models, V1 flags, profiles, roles, or Casefile. Restart Codex after a successful setup.
