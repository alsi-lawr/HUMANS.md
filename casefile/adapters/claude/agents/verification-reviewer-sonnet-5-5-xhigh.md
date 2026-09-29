---
name: verification-reviewer-sonnet-5-5-xhigh
description:
  Verify concrete primary Casefile review findings, pinned to Claude Sonnet 5.5 at xhigh effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-sonnet-5-5
effort: xhigh
---

{{include:casefile/casefile-workflow/roles/verification-reviewer.md}}

Obey repository authority, do not repeat the full review, and edit neither source nor tickets.
