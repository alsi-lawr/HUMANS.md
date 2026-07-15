---
name: casefile-codex-cutover
description: "Use only when a human explicitly authorises an opt-in transactional cutover of an installed humans-md Casefile plugin into active Codex state. Never run during installation or ordinary setup."
---

# Casefile Codex Cutover

Require an explicit complete cutover-plan TOML, installed plugin root, empty backup directory, and external record path. Preview with `${CODEX_PLUGIN_ROOT}/scripts/cutover-codex.py` before requesting apply authority. The plan inventories active configuration, direct skills, direct agents, workflow resources, and marketplace state by path and hash; names only superseded direct copies for post-success removal; and supplies strict config, discovery, fresh V1, root-profile, and exact inspector-profile gates.

Run `--apply` only after the human approves the preview and has arranged the required host restart and fresh-process probes. The tool installs the marketplace plugin, installs reviewed complete configuration, runs every gate, and removes named old copies only after success. Any command, probe, verification, removal, or record failure restores and hash-verifies the complete inventory. Never infer missing paths or gates, never edit `models_cache.json`, and never treat a package smoke check as cutover evidence.
