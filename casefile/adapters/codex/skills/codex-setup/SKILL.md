---
name: casefile-codex-setup
description:
  "Use after installing casefile for Codex to preview and install Casefile model, multi-agent
  runtime, profile, and role integration."
---

# Casefile Codex setup

Require one explicit absolute activated planning Store root. Run
`${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py install --plugin-root ${CODEX_PLUGIN_ROOT} --planning-root <absolute-root>`
without `--apply`, review the plan, then ask once before applying. Omit `--multi-agent-version` for
the compatible V1 default, or pass `--multi-agent-version v2` with Codex 0.145.0 or newer. This
lifecycle owns only Casefile model-catalog, selected multi-agent feature, profile, and role
configuration. Setup requires authenticated app-server availability for Sol, Terra, Luna, and Spark
through a private configuration-free acquisition before mutation and never invokes a Codex debug
model command. Safe Codex file auth and environment API-key auth are supported; keyring-only auth
fails closed rather than being copied across home identities. It never installs or replaces
`AGENTS.md`.
