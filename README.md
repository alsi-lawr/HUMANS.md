<div align="center">

# humans-md

**Give coding agents boundaries, not a manual.**

A portable plugin of behaviour contracts, task-shaped skills, and governed Casefile workflows for Codex and Claude.

`v0.1.0` | `Codex` | `Claude` | `MIT`

</div>

---

`humans-md` packages a complete instruction system for agent-assisted repository work. Install it to get a focused contract template, reusable skills, and an explicit workflow for moving from investigation to reviewed implementation without giving up human control of strategy, scope, or writes.

## Install

Clone the repository, then use the generated package for your runtime.

```sh
git clone git@github.com:alsi-lawr/HUMANS.md.git
cd HUMANS.md
```

### Codex

Add the local package as a personal marketplace and install it:

```sh
codex plugin marketplace add "$PWD/plugins/codex/humans-md" --json
codex plugin add humans-md@humans-md --json
codex plugin list --json
```

Installation loads the plugin but does not rewrite active instructions or runtime profiles. The packaged `casefile-codex-setup`, catalog-profile, and cutover skills keep those changes preview-first and opt-in.

### Claude

Validate the package, then load it for a session:

```sh
claude plugin validate plugins/claude/humans-md --strict
claude --plugin-dir "$PWD/plugins/claude/humans-md"
```

The package is also ready to be placed in a Claude marketplace. It is not published to one by this repository.

## What ships

| Surface | What it provides |
| --- | --- |
| **Contract** | A portable `AGENTS.md` template and an explicit, backup-producing bootstrap tool. Installation never applies the template automatically. |
| **Skills** | Focused workflows for skill creation and packaging, README work, Git contributions, contract bootstrap, and every Casefile phase. |
| **Casefile** | Roles, schemas, matrices, and guarded scripts for investigation, ticket review, accepted implementation, strategy changes, and closeout. |
| **Adapters** | Runtime-specific metadata, model bindings, setup tools, and generated packages for Codex and Claude. |
| **Verification** | Deterministic package checks plus separate structural, balanced, and comparative skill-verification presets. |

The Casefile path is deliberately legible:

```text
request -> investigate -> tickets -> review -> implement -> closeout
```

The request-receiving root remains in charge. Strategies are selected explicitly, investigators report candidates before tickets are reserved, overlapping writes receive one owner, and review evidence stays separate from correction work.

## Use it

Ask for the outcome; the plugin supplies the task model.

```text
Investigate this repository and preserve accepted findings as governed tickets.
```

```text
Implement the accepted ticket batch using the selected Casefile matrix.
```

```text
Create a skill for our migration-planning workflow and help me choose how to verify it.
```

```text
Preview the packaged behaviour contract against this repository's AGENTS.md.
```

Planning persistence remains user-configured. Public packages do not assume a private planning path, and contract bootstrap refuses to merge or replace existing instructions without explicit authority.

## Portable source, generated packages

The source of truth is split by responsibility:

```text
skills/                  portable task instructions
casefile-workflow/       portable roles, schemas, and scripts
adapters/                Codex and Claude bindings
packaging/plugins/       portable product manifests
plugins/                 committed generated packages
```

Both packages are generated from [`packaging/plugins/humans-md.toml`](packaging/plugins/humans-md.toml):

- [`plugins/codex/humans-md/`](plugins/codex/humans-md/)
- [`plugins/claude/humans-md/`](plugins/claude/humans-md/)

Regenerate or check them with Python 3.14 and the standard library:

```sh
python3 scripts/package-plugin.py build --all
python3 scripts/package-plugin.py check --all
python3 scripts/validate-skill.py --all --root .
python3 scripts/validate-casefile.py --source .
```

Generation rejects unsafe paths, symlinks, missing resources, output collisions, stale files, mode drift, and byte drift. CI repeats package parity, ASCII, vendor, and isolated discovery checks.

## Status

| Runtime | Evidence |
| --- | --- |
| **Codex** | Package generation, isolated installation, strict configuration, V1 selection, Sol/xhigh root, Terra/xhigh inspector, and installed-byte parity verified on the maintainer's machine. |
| **Claude** | Deterministic generation and strict plugin validation verified. Installation, triggering, routing, and behavioural execution remain unverified. |

The balanced candidate/baseline suite is specified but has not been executed, so sampled behavioural claims remain unverified. See the evidenced [Casefile migration report](docs/2026-07-15-casefile-plugin-workflow.md) for verbatim human prompts and the implementation, review, and verification trail.

## Why this shape?

[`HUMANS.md`](HUMANS.md) contains the argument and evidence behind the system. The short version: standing instructions should govern conduct; task knowledge should load only for the task; hard guarantees belong in tools.

Released under the [MIT licence](LICENSE).
