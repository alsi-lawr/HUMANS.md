---
name: casefile-codex-setup
description: "Use when a human explicitly asks to complete an opt-in humans-md Codex setup, including reviewed candidates, contract and catalog decisions, transactional cutover, and recoverable uninstall state."
---

# Casefile Codex Setup

Require the installed plugin root and a repository-local or temporary output directory. Run `${CODEX_PLUGIN_ROOT}/scripts/prepare-setup.py` without `--apply`, review every rendered absolute profile path and feature flag, then rerun with `--apply` only to create candidate files. Build a complete candidate from the active configuration without dropping unrelated settings.

Inventory the configured marketplace and installed plugin before choosing explicit `add`, `upgrade`, or `reuse` actions. A marketplace source is a marketplace root or Git source, never `${CODEX_PLUGIN_ROOT}`. Record whether uninstall owns removal of the plugin and marketplace; do not remove a shared marketplace implicitly.

For the complete opinionated setup, load `contract-bootstrap` for an explicitly selected `AGENTS.md` destination and `casefile-codex-catalog-profile` for a caller-supplied fresh catalog export. Then load `casefile-codex-cutover`. Its plan preserves unrelated state, runs strict configuration and discovery, restarts the host into fresh probes, and proves V1 plus exact root and inspector bindings. Keep the successful install record and backup directory: `casefile-codex-uninstall` requires both. Never edit `models_cache.json` or treat package validation as live evidence.
