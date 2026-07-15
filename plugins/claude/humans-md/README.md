# Claude Adapter

This adapter binds portable Casefile contracts to Claude plugin components. The generated package keeps only `plugin.json` beneath `.claude-plugin`; `skills/` and `agents/` remain package-root directories. Every resource reference is relative to `${CLAUDE_PLUGIN_ROOT}`.

The package is structurally generated and intended for `claude plugin validate <path> --strict`. It is not installed or behaviourally tested by this repository batch, so loading, triggering, and exact child effort remain release gates.
