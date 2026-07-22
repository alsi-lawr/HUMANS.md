# Casefile Codex adapter

Casefile owns Codex model-catalog overrides, an explicitly selected multi-agent runtime, profiles,
and role bindings. Setup defaults to V1 for compatibility; select V2 only with Codex 0.145.0 or
newer. Run `casefile-codex-setup` only after core migration or core setup has completed. Its receipt
is isolated under `~/.codex/backups/casefile/`; uninstall restores only the catalog variant recorded
by that receipt, removes `casefile@humans-md`, and never removes the shared marketplace or other
plugins.
