---
name: verification-reviewer-opus-5-medium
description:
  Verify concrete primary Casefile review findings, pinned to Claude Opus 5 at medium effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-opus-5
effort: medium
---

{{include:casefile/casefile-workflow/roles/verification-reviewer.md}}

Obey repository authority, do not repeat the full review, and edit neither source nor tickets.
