---
name: claude-uninstall
description: "Use to restore the CLAUDE.md state saved by claude-setup and remove the humans-md Claude plugin through Claude's canonical uninstall command."
---

# Claude Uninstall

Resolve the configuration directory as `${CLAUDE_CONFIG_DIR}` when set, otherwise `~/.claude`. The managed destination is `<config>/CLAUDE.md` and the setup receipt is `<config>/backups/humans-md/claude`.

When the receipt exists, require exactly one prior-state record: `CLAUDE.md.before` or `CLAUDE.md.was-missing`. Use `git diff --no-index` to show the complete difference between the current destination and the saved file. Represent a missing endpoint with a temporary empty file and clearly state when approval will delete or recreate `CLAUDE.md`; do not build or summarize a separate diff engine. Ask once before applying the reviewed recovery.

After approval, atomically move `CLAUDE.md.before` back over the destination, or remove the destination when `CLAUDE.md.was-missing` records prior absence. Verify the restored bytes or absence, then remove the receipt directory. If no receipt exists, leave `CLAUDE.md` unchanged and say that no humans-md setup recovery is available.

Finally run Claude's canonical user-scope removal command:

```sh
claude plugin uninstall humans-md@humans-md --scope user
```

If plugin removal fails after recovery, report that the old `CLAUDE.md` state is restored and the plugin remains installed; do not reinstall the managed contract.
