<div align="center">

# humans-md

**Give coding agents boundaries, not a manual.**

A portable plugin of behaviour contracts, task-shaped skills, and governed Casefile workflows for Codex and Claude.

`v0.1.0` | `Codex` | `Claude` | `MIT`

</div>

---

`humans-md` packages a complete instruction system for agent-assisted repository work. Install it to get a focused contract template, reusable skills, and an explicit workflow for moving from investigation to reviewed implementation without giving up human control of strategy, scope, or writes.

## Install

Install the versioned package from the dedicated
[`humans-md-marketplace`](https://github.com/alsi-lawr/humans-md-marketplace)
repository.

### Codex

Add the tagged marketplace and install the plugin:

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.1.0 --json
codex plugin add humans-md@humans-md --json
codex plugin list --json
```

Installation loads the plugin but does not rewrite active instructions or runtime profiles. The packaged `casefile-codex-setup`, catalog-profile, and cutover skills keep those changes preview-first and opt-in.

### Claude

Add the tagged marketplace and install the plugin:

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.1.0
claude plugin install humans-md@humans-md
```

Claude package generation and strict validation are verified; live loading and
behaviour remain unverified.

## What ships

| Surface | What it provides |
| --- | --- |
| **Contract** | A portable `AGENTS.md` template and an explicit, backup-producing bootstrap tool. Installation never applies the template automatically. |
| **Skills** | Focused workflows for skill creation and packaging, README work, Git contributions, contract bootstrap, and every Casefile phase. |
| **Casefile** | Roles, schemas, matrices, and guarded scripts for investigation, ticket review, accepted implementation, strategy changes, and closeout. |
| **Adapters** | Runtime-specific metadata, model bindings, setup tools, and package inputs for Codex and Claude. |
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

## Portable source, released packages

The source of truth is split by responsibility:

```text
skills/                  portable task instructions
casefile-workflow/       portable roles, schemas, and scripts
adapters/                Codex and Claude bindings
packaging/plugins/       portable product manifests
packaging/marketplace/   marketplace catalogs and release README
build/marketplace/       ignored local staging tree
```

Both packages are generated from
[`packaging/plugins/humans-md.toml`](packaging/plugins/humans-md.toml) with
Python 3.14 and the standard library:

```sh
python3 scripts/package-plugin.py build --all
cp -R packaging/marketplace/. build/marketplace/
cp LICENSE build/marketplace/LICENSE
python3 scripts/package-plugin.py check --all
python3 scripts/validate-skill.py --all --root .
python3 scripts/validate-casefile.py --source .
```

Generation rejects unsafe paths, symlinks, missing resources, output
collisions, stale files, mode drift, and byte drift. CI builds and validates the
ignored staging tree. The manual release workflow publishes that exact tree to
the marketplace repository and tags its generated commit; this source
repository does not commit package outputs or attach release archives.
Publishing uses the `MARKETPLACE_DEPLOY_KEY` Actions secret. Its public key has
write access only to `alsi-lawr/humans-md-marketplace`.

## Status

| Runtime | Evidence |
| --- | --- |
| **Codex** | Package generation, isolated installation, strict configuration, V1 selection, Sol/xhigh root, Terra/xhigh inspector, and installed-byte parity verified on the maintainer's machine. |
| **Claude** | Deterministic generation and strict plugin validation verified. Installation, triggering, routing, and behavioural execution remain unverified. |

The balanced candidate/baseline suite is specified but has not been executed, so sampled behavioural claims remain unverified. See the evidenced [Casefile migration report](docs/2026-07-15-casefile-plugin-workflow.md) for verbatim human prompts and the planning, implementation, review, and verification trail.

## Research and citation

`humans-md` is also maintained as a research-adjacent software artifact: the
thesis, executable contracts, verification design, released packages, and
development evidence are independently inspectable. It is not presented as a
peer-reviewed study, and no general behavioural-effectiveness claim is made.

See [Research use and citation](docs/research-use.md) for the artifact map,
evidence classes, reproduction protocol, and current limits. Machine-readable
citation metadata is available in [`CITATION.cff`](CITATION.cff); until an
archival DOI exists, identify the exact commit evaluated.

## Why this shape?

[`HUMANS.md`](HUMANS.md) contains the argument and evidence behind the system. The short version: standing instructions should govern conduct; task knowledge should load only for the task; hard guarantees belong in tools.

Released under the [MIT licence](LICENSE).
