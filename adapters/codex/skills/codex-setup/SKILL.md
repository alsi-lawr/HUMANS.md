---
name: codex-setup
description: "Use immediately after installing humans-md to configure Codex V1, model overrides, Casefile roles, the global AGENTS.md contract, and durable uninstall recovery."
---

# Codex Setup

Run `${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install --plugin-root ${CODEX_PLUGIN_ROOT}` without `--apply`. Show the concise JSON preview and ask once for approval.

After approval, rerun with `--apply`. The script alone owns discovery, catalog generation, configuration, contract installation, legacy removal, backup, rollback, and mechanical verification. Report its result and the required host restart. Do not construct plans, merge configuration, choose probes, or perform model-based verification.

If the script fails, say only: "Setup failed. Do you want me to debug it?" Do not investigate unless the human agrees.
