<div align="center">

<img src="assets/humans-md.svg" alt="humans.md logo" width="128" height="128">

# humans-md

[![Reproducibility](https://github.com/alsi-lawr/HUMANS.md/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/alsi-lawr/HUMANS.md/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/alsi-lawr/HUMANS.md?display_name=tag&sort=semver)](https://github.com/alsi-lawr/HUMANS.md/releases/latest)

**Give coding agents boundaries, not a manual.**

Three independently installable plugins for standing contracts, governed Casefile work, and reusable
coding guidance.

</div>

## Install

Add the `humans-md` marketplace at `v0.5.0`, then install the identities you need. Start with core;
Casefile and coding remain optional siblings.

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.5.0
codex plugin add humans-md@humans-md
# optional siblings
codex plugin add casefile@humans-md
codex plugin add coding@humans-md
```

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.5.0
claude plugin install humans-md@humans-md --scope user
# optional siblings
claude plugin install casefile@humans-md --scope user
claude plugin install coding@humans-md --scope user
```

`humans-md` owns only the standing `AGENTS.md` / `CLAUDE.md` contract lifecycle and recovery.
`casefile` owns Casefile workflows and Codex model, selected multi-agent runtime, profile, and role
integration. `coding` owns Git contribution, README, skill-generation, and generic verification
guidance. Removing one plugin never removes the shared marketplace or another plugin.

The Casefile plugin bundles the complete supported Linux, macOS, and Windows x64/ARM64 executable
matrix. After installing it, invoke its host-specific Casefile setup skill with one absolute
activated current-v1 planning Store. Setup verifies and installs the matching executable, probes its
identity and 12-tool Provider contract, and receipt-binds the host directly to
`casefile mcp-package`. MCP startup then needs no Cargo, Python, Node, network, or `PATH` lookup.
MCP retains exact previews in the live session and returns compact review envelopes that apply by
`preview_id`. Routine writes proceed without a second confirmation; record deletions require one.

Implementation offers three explicit governed shapes: a strictly serial ticket batch, a serial
ticket batch with one read-only look-ahead, and a bounded pipeline that may overlap one independent,
write-disjoint next ticket with exact-commit review.

## Project

[Documentation](https://github.com/alsi-lawr/HUMANS.md/wiki) |
[Casefile guide](https://github.com/alsi-lawr/HUMANS.md/wiki/Casefile) | [Thesis](HUMANS.md) |
[Contributing](CONTRIBUTING.md) |
[Generated marketplace](https://github.com/alsi-lawr/humans-md-marketplace) | [MIT licence](LICENSE)
