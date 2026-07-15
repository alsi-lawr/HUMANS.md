---
name: casefile-codex-uninstall
description: "Use when a human explicitly asks to uninstall a humans-md Codex setup and recover the pre-install state from a selected successful cutover backup."
---

# Casefile Codex Uninstall

Require the exact successful install record, its bound install backup directory, a new empty rollback-backup directory, and an external uninstall record. Preview with `${CODEX_PLUGIN_ROOT}/scripts/cutover-codex.py uninstall ...`. Verify the record, recovery manifest, inventory, backup objects, and current managed-state hashes. Refuse changed managed state rather than overwrite later configuration or plugin work.

Show whether plugin and marketplace removal occurs before or after restoration and identify any shared marketplace retained by the install plan. Run `uninstall --apply` only after explicit approval. The transaction restores the selected pre-install inventory, performs only its declared removals, runs recovery gates, and records the result. On any failure it restores and verifies the complete pre-uninstall state from the new rollback backup. Never guess the latest backup or delete either backup automatically.
