---
name: casefile-codex-model-migrate
description:
  "Use when explicitly asked to migrate or refresh an existing Casefile-owned Codex model catalog
  and its profile registrations to the installed package. Not for fresh setup, runtime switching,
  full plugin updates, investigation writer reselection, or Codex-owned models_cache.json edits."
---

# Migrate Casefile Codex models

Use the already installed target Casefile package and identify the selected Codex home. Run
`${CODEX_PLUGIN_ROOT}/scripts/setup-codex.py migrate-models --plugin-root ${CODEX_PLUGIN_ROOT}`
without `--apply`; pass `--codex-home <absolute-home>` when selecting a non-default home.

Show the preview's catalog and profile additions, removals, changed fields, target paths, runtime,
and restart requirement. This replaces the selected Casefile catalog with the maintained package
catalog; it does not merge local customizations. Review any removals or overridden customizations
before asking for explicit approval of the exact `approval_digest`.

Repeat the same command with `--apply --expect-digest <approved-digest>` only after approval. If the
digest or inputs changed, obtain a fresh preview and approval; never silently accept the new digest.
Stop on unavailable required models or an unowned catalog. If the active runtime and receipt
disagree, surface that disagreement and require an explicit setup reconciliation, not an inferred
V1/V2 choice. Route first installations to `casefile-codex-setup` and full reinstalls to
`casefile-update`.

The command preserves the runtime, MCP binding and executable, root model and effort, unrelated
configuration, other catalog variant, and original pre-install recovery state. Do not edit Codex's
`models_cache.json`, patch an installed plugin cache by hand, or rewrite investigation matrices or
writer bindings. A new recommendation is not consent to replace an existing writer.

Report `migrated` or `unchanged` from the command result. A failure must remain a failure even when
rollback succeeds. After migration, restart Codex to reload profiles and the catalog, then run
`${CODEX_PLUGIN_ROOT}/scripts/resolve-writer-binding.py offer` against the same home to check the
offered model/effort pairs. Report availability without selecting or persisting a binding.
