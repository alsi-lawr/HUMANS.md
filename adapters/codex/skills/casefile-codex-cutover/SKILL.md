---
name: casefile-codex-cutover
description: "Use only when a human explicitly authorises an opt-in transactional cutover of an installed humans-md Casefile plugin into active Codex state. Never run during installation or ordinary setup."
---

# Casefile Codex Cutover

Require an explicit complete cutover-plan TOML, installed plugin root, empty backup directory, and external record path. The plan declares a marketplace Git source or marketplace root separately from the installed plugin root; chooses `add`, `upgrade`, or `reuse` for that marketplace and `add` or `reuse` for the plugin; and explicitly declares uninstall ownership. It inventories active configuration, direct skills, direct agents, workflow resources, and marketplace state and supplies install and recovery gates.

Read [the lifecycle-plan contract](references/lifecycle-plan.md) before drafting or reviewing the plan.

Preview with `${CODEX_PLUGIN_ROOT}/scripts/cutover-codex.py install ...` before requesting apply authority. Run `install --apply` only after the human approves and has arranged the required host restart and fresh-process probes. Any command, probe, verification, removal, or record failure restores and hash-verifies the complete inventory. Success writes a bound recovery manifest into the backup directory; preserve it with the external install record.

Never substitute `${CODEX_PLUGIN_ROOT}` for the marketplace source, infer missing paths or actions, edit `models_cache.json`, or treat a package smoke check as cutover evidence. Load `casefile-codex-uninstall` for removal or recovery.
