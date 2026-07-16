# humans-md marketplace

Installable Codex and Claude packages generated from
[`alsi-lawr/HUMANS.md`](https://github.com/alsi-lawr/HUMANS.md).

## Codex

```sh
codex plugin marketplace add alsi-lawr/humans-md-marketplace --ref v0.1.5
codex plugin add humans-md@humans-md
```

Then run `$humans-md:codex-setup`. Use `$humans-md:codex-uninstall` to review
managed-file diffs and restore the pre-install receipt.

## Claude

```sh
claude plugin marketplace add alsi-lawr/humans-md-marketplace@v0.1.5
claude plugin install humans-md@humans-md --scope user
```

After installation, run `/humans-md:claude-setup` to preview and install the
global `CLAUDE.md` contract. Run `/humans-md:claude-uninstall` to review a Git
diff, restore the previous `CLAUDE.md` state, and remove the user-scope plugin.

This repository is generated. Release tags contain the installable trees;
source and contribution history belong in the source repository. Releases do
not use attached archives.
