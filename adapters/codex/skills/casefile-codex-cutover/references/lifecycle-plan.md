# Codex lifecycle plan

The install plan uses schema version 1 and requires these top-level values:

- `marketplace_source`: absolute local marketplace root containing `.agents/plugins/marketplace.json`, or a Git source accepted by Codex.
- `marketplace_ref`: optional Git ref; forbidden for local sources.
- `marketplace_name` and `install_ref`: the selector must end in `@<marketplace_name>`.
- `marketplace_action`: `add`, `upgrade`, or `reuse`.
- `plugin_action`: `add` or `reuse`.
- `remove_plugin_on_uninstall` and `remove_marketplace_on_uninstall`: explicit ownership decisions. Marketplace removal requires plugin removal. Keep the marketplace when it is shared.
- `codex_executable`, `codex_home`, `candidate_config`, and `active_config`.
- Exactly one managed path of each kind: `active_config`, `direct_skills`, `direct_agents`, `workflow_resources`, and `marketplace_state`. Each declares `remove_after_success`; only superseded direct copies may be removed.
- Install gates: `strict_config`, `discovery`, `v1_runtime`, `root_profile`, and `inspector_profile`. Runtime gates declare `fresh_process = true` and expected values.
- Recovery gates: `strict_config` and `discovery`, with commands that verify the intended recovered state.

An already-installed bootstrap plugin normally uses `reuse` after its exact source and version are verified. A clean source-driven transaction uses `add`. Never derive `marketplace_source` from `CODEX_PLUGIN_ROOT`.

Install preview and apply:

```text
cutover-codex.py install --plan PLAN --plugin-root PLUGIN_ROOT --backup-dir INSTALL_BACKUP --record INSTALL_RECORD
cutover-codex.py install --plan PLAN --plugin-root PLUGIN_ROOT --backup-dir INSTALL_BACKUP --record INSTALL_RECORD --apply
```

Uninstall preview and apply:

```text
cutover-codex.py uninstall --install-record INSTALL_RECORD --install-backup-dir INSTALL_BACKUP --rollback-backup-dir UNINSTALL_BACKUP --record UNINSTALL_RECORD
cutover-codex.py uninstall --install-record INSTALL_RECORD --install-backup-dir INSTALL_BACKUP --rollback-backup-dir UNINSTALL_BACKUP --record UNINSTALL_RECORD --apply
```

The successful install record and install backup are one recovery unit. Never pair artifacts from different transactions or select a backup by recency.
