---
name: casefile-codex-setup
description:
  "Use after installing casefile for Codex to preview and install the Casefile runtime, named roles,
  and planning Store binding."
---

# Casefile Codex setup

Require one explicit absolute activated planning Store root. Run
`${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install --plugin-root ${CODEX_PLUGIN_ROOT} --planning-root <absolute-root>`
without `--apply`, review the plan, then ask once before applying. Setup keeps the active runtime on
an upgrade and defaults to V1 on a first install. If the active flags are ambiguous, choose a
runtime with `--multi-agent-version v1` or `--multi-agent-version v2`.

Setup uses the host's model configuration. When upgrading an old integration, review any catalog
files the receipt proves Casefile owned and any earlier catalog setting it will restore. The
recovery snapshot retains the bytes present before the upgrade.
