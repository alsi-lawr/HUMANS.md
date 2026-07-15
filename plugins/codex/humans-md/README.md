# Codex Adapter

This directory binds portable Casefile contracts to named Codex profiles and selectable matrices. Runtime model IDs, reasoning levels, feature flags, marketplace metadata, setup, and catalog policy live here and nowhere in portable source.

Version 0.1.0 requires the root profile on Sol/xhigh, worker bindings from `profiles.toml`, `multi_agent = true`, and `multi_agent_v2 = false`. The exact fresh-process inspector model and effort remain a release gate.

Installation is opt-in through a marketplace. `casefile-codex-setup` renders candidates but never edits user configuration. `casefile-codex-catalog-profile` accepts only an explicit fresh export, never a cache file. After approved setup, validate strict configuration, restart the host, open a new root thread, and probe every exact child profile. Restore backups on any failure; do not claim cutover from package or discovery checks alone.
