---
name: claude-uninstall
description:
  "Use to restore the versioned CLAUDE.md and settings.json state saved by claude-setup and remove
  only the humans-md Claude plugin."
---

# Claude uninstall

Read the active `claude-v0.2.0` receipt, preview a complete `git diff --no-index` between
`${CLAUDE_CONFIG_DIR:-~/.claude}/CLAUDE.md` and its recorded prior state, and ask once before
restoring it.

Restore the managed settings keys in the same approval. Read `settings_before` from the receipt and
apply it leaf by leaf against the live `settings.json`: a recorded value is written back at that
dotted path, and `null` means the key did not exist and must be removed rather than set. Change
nothing else in the file, and leave a managed parent object in place when it still holds unrelated
keys. `settings_file_before` names the verbatim pre-install copy in the receipt directory (`missing`
when there was no file); use it to show the diff, not as a wholesale overwrite, because it would
discard any unrelated key the operator added after install.

Then remove only `humans-md@humans-md --scope user`. Never remove the shared marketplace or sibling
plugins.
