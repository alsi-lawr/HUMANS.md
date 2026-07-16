<div align="center">

# humans-md

**Give coding agents boundaries, not a manual.**

Portable behaviour contracts and governed Casefile workflows for Codex and
Claude.

`v0.1.5` | `MIT`

</div>

## Install for Codex

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.1.5
codex plugin add humans-md@humans-md
```

Then immediately invoke the setup skill in Codex:

```text
Use $humans-md:codex-setup. Preview the setup, then ask me once before applying it.
```

Setup is one deterministic transaction. It installs the global contract,
Casefile roles, V1 feature flags, and a generated model-catalog override; it
also backs up everything it replaces. After setup succeeds, fully restart the
Codex host and start a new root thread.

The model and reasoning effort of the agent invoking Casefile remain yours;
setup does not select or replace them.

Uninstall previews focused diffs for modified managed files, then after approval
removes or restores humans-md-managed state while preserving unrelated config changes.

To restore the backed-up state and remove the plugin and marketplace:

```text
Use $humans-md:codex-uninstall. Preview the uninstall, then ask me once before applying it.
```

## Install for Claude

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.1.5
claude plugin install humans-md@humans-md --scope user
```

Then run `/humans-md:claude-setup`. It previews the packaged contract against
the global `${CLAUDE_CONFIG_DIR:-~/.claude}/CLAUDE.md`, asks before replacing
it, and retains the exact prior file for recovery. Restart Claude Code after a
successful setup.

To restore that prior `CLAUDE.md` state and invoke Claude's canonical plugin
uninstall, run `/humans-md:claude-uninstall`.

Claude packaging is strictly validated, but its runtime behaviour has not been
forward-tested by this project.

## Use Casefile

Start or resume governed repository work with:

```text
Use $humans-md:casefile to investigate this repository. Show me the compatible strategies and wait for my selection.
```

Casefile routes work through six discoverable skills:

```text
casefile
  -> casefile-investigate
  -> casefile-review
  -> casefile-implement
  -> casefile-close
```

Invoke `casefile-switch` when the selected strategy needs to change.
Implementation can remain serial or use the bounded pipeline to preflight one
independent next ticket and begin it while the prior commit is reviewed.

In Codex, to preview the repository behaviour contract without changing it:

```text
Use $humans-md:contract-bootstrap to preview the packaged AGENTS.md contract against this repository. Do not apply it yet.
```

Planning storage remains user-configured. Plugin installation alone never
replaces standing instructions or active configuration.

## Project

[Casefile guide](casefile-workflow/README.md) |
[Thesis](HUMANS.md) |
[Migration evidence](docs/2026-07-15-casefile-plugin-workflow.md) |
[Research and citation](docs/research-use.md) |
[Generated marketplace](https://github.com/alsi-lawr/humans-md-marketplace) |
[MIT licence](LICENSE)
