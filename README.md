<div align="center">

<img src="assets/humans-md.svg" alt="humans.md logo" width="128" height="128">

# humans-md

[![Reproducibility](https://github.com/alsi-lawr/HUMANS.md/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/alsi-lawr/HUMANS.md/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/alsi-lawr/HUMANS.md?display_name=tag&sort=semver)](https://github.com/alsi-lawr/HUMANS.md/releases/latest)
[![MIT licence](https://img.shields.io/github/license/alsi-lawr/HUMANS.md)](LICENSE)

**Give coding agents boundaries, not a manual.**

Three independently installable plugins for standing contracts, governed Casefile work, and reusable
coding guidance.

</div>

## Install

Add the `humans-md` marketplace at `v0.2.3`, then install the identities you need. Start with core;
Casefile and coding remain optional siblings.

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.2.3
codex plugin add humans-md@humans-md
# optional after core setup or migration succeeds
codex plugin add casefile@humans-md
codex plugin add coding@humans-md
```

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.2.3
claude plugin install humans-md@humans-md --scope user
# optional after core setup or migration succeeds
claude plugin install casefile@humans-md --scope user
claude plugin install coding@humans-md --scope user
```

`humans-md` owns only the standing `AGENTS.md` / `CLAUDE.md` contract lifecycle and recovery.
`casefile` owns Casefile workflows and Codex model, selected multi-agent runtime, profile, and role
integration. `coding` owns Git contribution, README, skill-generation, and generic verification
guidance. Removing one plugin never removes the shared marketplace or another plugin.

## Upgrade from v0.1.5

Do **not** install `casefile` or `coding` yet. Update the existing `humans-md` plugin to `v0.2.0`,
restart the host, then invoke its `migrations` skill. It supports only `0.1.5 -> 0.2.0`: it previews
restoration of the old managed baseline, shows focused Git diffs for managed files, records a
preview fingerprint for the approval, revalidates that every managed target still matches it, and
then reseeds a fresh contract-only core receipt. It preserves the marketplace.

After that succeeds, install optional sibling plugins and run their own setup skills where needed.
Missing, altered, unsafe, or ambiguous legacy receipts stop with recovery guidance rather than being
adopted.

## Project

[Casefile guide](casefile/casefile-workflow/README.md) | [Thesis](HUMANS.md) |
[Generated marketplace](https://github.com/alsi-lawr/humans-md-marketplace) | [MIT licence](LICENSE)

## Development

Enter the pinned development environment from the repository root:

```sh
nix develop
(cd casefile/web && bun install)
```

It provides the Rust, Python, Node.js, workflow, GitHub, Docker-client, and PTY tools used by the
repository checks. The one-time Bun install materializes the lock-resolved web tooling; manifest
ranges remain upgrade-forward for deliberate updates. Docker itself remains an external service.
Format Markdown and TypeScript with Prettier and Rust with rustfmt through the shared command:

```sh
scripts/format-source.sh --write
```

Build and inspect the Casefile browser workbench against a planning root with:

```sh
cd casefile/web
bun run typecheck
bun run test
bun run build
cd ..
cargo run -p casefile-cli -- --root ~/dev/agent-planning serve --write
```

Open the printed loopback URL. Read-only browsing works immediately; paste the printed write
capability into the workbench only when you intend to preview and apply governed ticket, epic, or
board edits. The server never needs Node at runtime because it embeds the tracked static build.

Run `nix flake check` to evaluate the flake. Replay CI with a cached runner image from inside the
shell:

```sh
act pull_request -j validate --pull=false \
  -P ubuntu-latest=catthehacker/ubuntu:act-latest
```
