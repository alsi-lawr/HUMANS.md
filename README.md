<div align="center">

# humans-md

**Give coding agents boundaries, not a manual.**

Three independently installable plugins for standing contracts, governed
Casefile work, and reusable coding guidance.

`v0.2.0` | `MIT`

</div>

## Install

Add the `humans-md` marketplace at `v0.2.0`, then install the identities you
need. Start with core; Casefile and coding remain optional siblings.

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.2.0
codex plugin add humans-md@humans-md
# optional after core setup or migration succeeds
codex plugin add casefile@humans-md
codex plugin add coding@humans-md
```

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.2.0
claude plugin install humans-md@humans-md --scope user
# optional after core setup or migration succeeds
claude plugin install casefile@humans-md --scope user
claude plugin install coding@humans-md --scope user
```

`humans-md` owns only the standing `AGENTS.md` / `CLAUDE.md` contract lifecycle
and recovery. `casefile` owns Casefile workflows and Codex model, V1, profile,
and role integration. `coding` owns Git contribution, README, skill-generation,
and generic verification guidance. Removing one plugin never removes the shared
marketplace or another plugin.

## Upgrade from v0.1.5

Do **not** install `casefile` or `coding` yet. Update the existing `humans-md`
plugin to `v0.2.0`, restart the host, then invoke its `migrations` skill. It
supports only `0.1.5 -> 0.2.0`: it previews restoration of the old managed
baseline, shows focused Git diffs for managed files, records a preview fingerprint for the
approval, revalidates that every managed target still matches it, and then reseeds a fresh contract-only core receipt. It preserves the marketplace.

After that succeeds, install optional sibling plugins and run their own setup
skills where needed. Missing, altered, unsafe, or ambiguous legacy receipts stop
with recovery guidance rather than being adopted.

## Project

[Casefile guide](casefile/casefile-workflow/README.md) |
[Thesis](HUMANS.md) |
[Generated marketplace](https://github.com/alsi-lawr/humans-md-marketplace) |
[MIT licence](LICENSE)
