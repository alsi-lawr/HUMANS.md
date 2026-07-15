# Codex Adapter

This directory binds portable Casefile contracts to matrix-qualified Codex profiles and selectable matrices. Runtime model IDs, reasoning levels, feature flags, marketplace metadata, setup/cutover, a Codex-only GitHub CLI reference, and catalog policy live here and nowhere in portable source.

Version 0.1.1 requires the root profile on Sol/xhigh, 11 exact matrix-qualified worker bindings from `profiles.toml`, `multi_agent = true`, `multi_agent_v2 = false`, and declared `multi_agent_version` selectors set to JSON null. The canonical profile also binds eight authored instruction/message resource pairs without storing a full catalog. The exact fresh-process inspector model and effort remain a release gate.

Installation is opt-in through a marketplace. `casefile-codex-setup` renders candidates but never edits user configuration. `casefile-codex-catalog-profile` accepts a caller-asserted fresh export, never a cache path, and restores prior bytes and metadata on failure. `casefile-codex-cutover` is preview-first and requires a complete inventory plus strict, discovery, fresh V1, root, and inspector gates before selective old-copy removal. It has never been run against live state; do not claim cutover from package or discovery checks alone.
