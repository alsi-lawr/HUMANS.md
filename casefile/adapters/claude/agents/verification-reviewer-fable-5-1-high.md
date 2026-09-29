---
name: verification-reviewer-fable-5-1-high
description:
  Verify concrete primary Casefile review findings, pinned to Claude Fable 5.1 at high effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-fable-5-1
effort: high
---

{{include:casefile/casefile-workflow/roles/verification-reviewer.md}}

Obey repository authority, do not repeat the full review, and edit neither source nor tickets.
