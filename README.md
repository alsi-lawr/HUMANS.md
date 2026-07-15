<div align="center">

# humans-md

**Give coding agents boundaries, not a manual.**

Portable behaviour contracts, task-shaped skills, and governed Casefile
workflows for Codex and Claude.

`v0.1.1` | `Codex` | `Claude` | `MIT`

</div>

## Install

### Codex

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.1.1
codex plugin add humans-md@humans-md
```

**Immediately start a new Codex thread and invoke the setup skill:**

```text
Use $humans-md:casefile-codex-setup to complete setup. Preview every change and tell me what requires approval.
```

Installing the plugin exposes its skills. The setup skill prepares, but does
not silently apply, the Codex configuration.

### Claude

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.1.1
claude plugin install humans-md@humans-md
```

Then invoke:

```text
/humans-md:casefile-claude-setup
```

This validates and inspects the package. Claude runtime behaviour has not yet
been forward-tested by this project.

## Use it

Start governed work with:

```text
Use $humans-md:casefile-workflow to investigate this repository. Show me the compatible strategies and wait for my selection.
```

Casefile then moves through:

```text
request -> investigate -> tickets -> review -> implement -> closeout
```

Implement accepted tickets with:

```text
Use $humans-md:casefile-implement-ticket-batch to implement the accepted tickets with exclusive write ownership and review.
```

Preview the repository contract with:

```text
Use $humans-md:contract-bootstrap to preview the packaged AGENTS.md contract against this repository. Do not apply it yet.
```

Installation never replaces standing instructions or active configuration
automatically. Planning storage remains user-configured.

## Read more

[Thesis](HUMANS.md) | [Migration evidence](docs/2026-07-15-casefile-plugin-workflow.md) | [Research and citation](docs/research-use.md) | [Packages](https://github.com/alsi-lawr/humans-md-marketplace) | [MIT licence](LICENSE)
