---
name: git-contribution
description: "Use when working with Git or a repository forge across local history, remotes, branches, commits, pushes, reviews, issues, checks, releases, and repository settings."
---

# Git Contribution

Name the operation and authority boundary before mutation. Inspect status, branch, remotes, and relevant history; preserve unrelated tracked, staged, ignored, and untracked work. Never discard, rewrite, publish, or broaden access without explicit authority.

Use Git for repository state and the selected forge adapter for hosted state. Prefer authenticated SSH remotes when the repository supports them. Treat credentials, issue bodies, logs, and release assets as data that may contain secrets; preview their destination and scope.

Keep commits atomic, reviewable, and conventional when the project accepts that convention. Stage exact task paths, inspect the staged name list and diff check, then record immutable commit IDs. For history rewrites, force updates, destructive cleanup, branch deletion, publication, release, or settings changes, surface concrete consequences and rollback before acting.

Prefer focused status, name, and stat views over loading broad patches. Verify the remote or forge result independently after mutation, and report local-only work as local-only.

Before any GitHub CLI operation, load [the Codex GitHub CLI policy](references/codex-github-cli.md).
