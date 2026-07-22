# Contributing

HUMANS.md is a multi-plugin source repository. Contributions must preserve the authority boundaries
in the closest `AGENTS.md`, keep generated packages reproducible, and avoid coupling independently
installable plugins.

## Repository layout

- `humans-md/` owns the standing `AGENTS.md` / `CLAUDE.md` contract lifecycle.
- `casefile/` owns the governed workflow, Rust CLI/TUI/server/store, browser workbench, skills,
  strategies, roles, and optional Codex integration.
- `coding/` owns reusable coding, Git, README, and skill-generation guidance.
- `scripts/` owns shared source, package, and marketplace validation.
- `packaging/marketplace/` contains inputs copied into the generated marketplace repository.

Skills, role prompts, schemas, fixtures, and model instruction files are product inputs rather than
developer documentation. User and reference documentation belongs in the
[project wiki](https://github.com/alsi-lawr/HUMANS.md/wiki).

## Development environment

Enter the pinned environment and install the lock-resolved browser dependencies once:

```sh
nix develop
(cd casefile/web && bun install)
```

The shell supplies Rust, Python, Node.js, Bun, workflow, GitHub, Docker-client, and PTY tools.
Docker itself remains an external service.

Format Markdown and TypeScript with Prettier and Rust with rustfmt:

```sh
scripts/format-source.sh --write
```

Run the same source checks used by CI:

```sh
scripts/format-source.sh --check
python scripts/strip-non-ascii.py --check .
python scripts/test-all.py

(cd casefile && cargo fmt --check)
(cd casefile && cargo clippy --workspace --all-targets -- -D warnings)
(cd casefile && cargo test --workspace)

(cd casefile/web && bun run typecheck)
(cd casefile/web && bun run test)
(cd casefile/web && bun run build)

python scripts/validate-package-roots.py
python casefile/scripts/validate-casefile.py --source casefile
python coding/scripts/validate-skill.py --all --root coding
python scripts/package-plugin.py build --all
python scripts/package-plugin.py check --all
```

`nix flake check` evaluates the flake. CI can also be replayed from inside the shell with a cached
runner image:

```sh
act pull_request -j validate --pull=false \
  -P ubuntu-latest=catthehacker/ubuntu:act-latest
```

## Casefile development

The Casefile Rust workspace owns canonical parsing, validation, querying, preview, and apply
semantics. The SQLite adapter is a disposable derived index. The loopback server fixes one planning
root at launch and embeds the tracked browser build; the browser does not parse or write planning
files directly.

Build and inspect the browser workbench against a planning root with:

```sh
cd casefile/web
bun run typecheck
bun run test
bun run build
cd ..
cargo run -p casefile-cli -- --root ~/dev/agent-planning serve --write
```

Read-only browsing works immediately. Supply the printed, non-persisted write capability only when
testing governed ticket, epic, or board replacement.

The Codex adapter owns the selected Casefile model catalog and multi-agent runtime. Setup defaults
to V1; V2 requires Codex 0.145.0 or newer. The Claude adapter supplies workflow skills, matrices,
and role agents without owning the standing contract. Neither adapter removes the shared marketplace
or sibling plugins.

The source CLI is optional infrastructure, not part of installed plugin setup:

```sh
cargo build --manifest-path casefile/Cargo.toml --release -p casefile-cli
casefile/target/release/casefile --root "$CASEFILE_ROOT" check --require-activation
```

## Packages and generated assets

The three `*/packaging/plugin.toml` manifests are the package-version authority. Package metadata is
rendered from those manifests; do not hard-code a release version in generated templates or tests.

The Casefile browser build under `casefile/casefile-server/web/` is tracked and embedded by Rust.
After changing `casefile/web/`, rebuild it and verify that the committed assets are intentional.

To validate the full generated marketplace tree:

```sh
python scripts/package-plugin.py build --all
python scripts/build-marketplace-catalog.py
cp -R packaging/marketplace/. build/marketplace/
cp LICENSE build/marketplace/LICENSE
python scripts/package-plugin.py check --all
python scripts/validate-package-roots.py
```

Generated marketplace history is published from source; do not edit the marketplace repository by
hand.

## Pull requests and releases

Keep commits atomic and conventional. Open source changes through a branch and require the hosted
reproducibility checks before merge. Update relevant wiki pages in the wiki repository rather than
adding new developer or reference documents beside the source.

For a release:

1. Update the synchronized versions in all three package manifests and the README install ref.
2. Run the full source and package checks.
3. Merge a green release pull request.
4. Create an annotated source tag on the release merge and publish a GitHub Release for that tag.
5. Dispatch `publish-marketplace.yml` with the manifest version.
6. Verify the annotated marketplace tag, generated versions, and `Source-Commit` provenance.

Release, history rewrite, branch deletion, and repository-setting changes require explicit human
authority.
