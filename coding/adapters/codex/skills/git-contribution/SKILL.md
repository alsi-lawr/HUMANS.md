---
name: git-contribution
description:
  "Use when working with Git or a repository forge across local history, remotes, branches, commits,
  pushes, reviews, issues, checks, releases, and repository settings."
---

# Git Contribution

## Protect the repository

- State the operation and authority boundary before mutation.
- Inspect status, branch, remotes, and relevant history.
- Preserve unrelated tracked, staged, ignored, and untracked work.
- Get explicit authority before discarding work, publishing, releasing, or broadening access.
- Use Git for repository state and the forge adapter for hosted state.
- Prefer authenticated SSH remotes.
- Preview the destination and scope of credentials, issue bodies, logs, and release assets.

## Build a patch series

- Create each feature or topic branch from its declared merge target.
- Treat every unmerged feature or topic branch as a mutable patch series.
- Make each commit one logical change that leaves the project working.
- Stage exact paths. Inspect staged names and run a diff check.
- Fold fixes into the commit they repair with amend, fixup and autosquash, or interactive rebase.
- Reorder and squash until every patch reads and applies cleanly.
- Rebase the series onto its merge target before final review or merge.
- Before rewriting, verify the branch is unmerged and unshared beyond review. Record its old tip.
- State that published commit IDs will change.
- Push an authorised rewrite with `git push --force-with-lease`. Reject plain `--force`.
- Require separate explicit authority to rewrite default, release, merged, or shared history.

## Publish and verify

- Follow the repository's commit convention.
- State consequences and rollback before cleanup, branch deletion, release, settings, or access
  mutation.
- Prefer focused status, name, and stat views.
- Verify remote and forge mutations independently.
- Report local-only work as local-only.
- Load [the Codex GitHub CLI policy](references/codex-github-cli.md) before every GitHub CLI
  operation.
