---
name: migrations
description: "Use only to migrate a supported humans-md 0.1.5 installation to the split 0.2.0 marketplace before installing casefile or coding."
---

# humans-md migrations

Support exactly `0.1.5 -> 0.2.0`. Do not install `casefile` or `coding` until this migration has completed.

For Codex, run `${CODEX_PLUGIN_ROOT}/scripts/migrate-v0.1.5.py` without `--apply` and record its `approval_fingerprint`. It validates the active legacy receipt, prints the restore-and-reseed plan, and shows a focused `git diff --no-index` for every managed file it will replace. Ask once after that preview. On approval, run it with `--apply --approval <fingerprint>`; it validates the receipt and current state again immediately before mutation, restores the legacy baseline while retaining later unowned config content, retires the active legacy receipt, and performs fresh contract-only core setup. It never removes the marketplace.

For Claude, inspect `${CLAUDE_CONFIG_DIR:-~/.claude}/backups/humans-md/claude` using the same preview/approval/revalidation sequence in `scripts/migrate-v0.1.5-claude.py`; pass its displayed fingerprint as `--approval` on apply. It restores `CLAUDE.md` from the legacy receipt and reseeds the core contract receipt. It never runs a plugin or marketplace removal command.

Stop with recovery guidance for missing, unsafe, changed, or ambiguous receipts. This is restore and reseed, not receipt adoption; do not generalize it to other versions.
