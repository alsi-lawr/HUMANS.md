# Claude Adapter

This adapter binds portable Casefile contracts to Claude plugin components. The generated package keeps only `plugin.json` beneath `.claude-plugin`; `skills/` and `agents/` remain package-root directories. Every resource reference is relative to `${CLAUDE_PLUGIN_ROOT}`.

The optional implementation pipeline binds its read-only look-ahead role to
Haiku at medium effort. Writer and review bindings remain separate from the
request-receiving orchestrator.

After user-scope plugin installation, `claude-setup` previews and installs the
packaged contract at `${CLAUDE_CONFIG_DIR:-~/.claude}/CLAUDE.md`. It moves any
previous file into a durable receipt before replacement. `claude-uninstall`
shows a Git diff, restores that exact prior state, and then runs Claude's
canonical user-scope plugin uninstall command. The generic
`contract-bootstrap` skill is intentionally absent from the Claude package.

The package is structurally generated and intended for
`claude plugin validate <path> --strict`. It is not behaviourally installed by
this repository batch, so loading, triggering, and exact child effort remain
release gates.
