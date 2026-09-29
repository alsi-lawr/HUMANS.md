---
name: verification-reviewer-fable-5-1-medium
description:
  Verify concrete primary Casefile review findings, pinned to Claude Fable 5.1 at medium effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-fable-5-1
effort: medium
---

{{include:casefile/casefile-workflow/roles/verification-reviewer.md}}

Obey repository authority, do not repeat the full review, and edit neither source nor tickets.
