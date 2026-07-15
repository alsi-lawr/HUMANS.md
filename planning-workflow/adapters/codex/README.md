# Codex Adapter

This adapter binds the portable planning workflow to Codex without defining the workflow itself.

- The request-receiving root remains orchestrator.
- Role config layers in `agents/` contain role instructions but no model or reasoning defaults.
- Presets in `matrices/` bind exact Codex profiles, model IDs, reasoning values, counts, depth, and concurrency. They are selectable presets, never implicit defaults.
- Missing strategy choices use Codex `request_user_input` with concrete compatible options.
- Investigation/review workers share a fresh task-local mirror at `<source>/.agent-workspace/<session-id>/agent-planning/`; only the root synchronises the private `~/dev/agent-planning` Git repository.
- Implementation planning requires the human to switch to Codex Plan mode. Persisting an accepted plan waits until Default mode resumes.

## Install

1. Run `scripts/validate-planning-workflow.py --source <HUMANS.md checkout>`.
2. Copy `planning-workflow/` directly (not as a symlink) to `$CODEX_HOME/planning-workflow/`; this installs the portable schemas and selectable Codex matrices.
3. Copy role TOMLs directly (not as symlinks) to `$CODEX_HOME/agents/`.
4. Copy the nine workflow skill directories directly to `$CODEX_HOME/skills/`.
5. Merge only the named `[agents.<role>]` declarations and compatible `[agents]` limits from `config-fragment.toml`; preserve unrelated config.
6. Run strict Codex configuration validation in a fresh process and rerun the validator with `--codex-home` to prove installed-copy parity.

The local persistence adapter uses SSH Git and GitHub for a private `agent-planning` repository. Verify privacy and remote commit durability before removing scratch sources.
