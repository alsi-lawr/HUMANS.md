---
name: verification-reviewer-opus-5-5-low
description:
  Verify concrete primary Casefile review findings, pinned to Claude Opus 5.5 at low effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-opus-5-5
effort: low
---

{{include:casefile/casefile-workflow/roles/verification-reviewer.md}}

Obey repository authority, do not repeat the full review, and edit neither source nor tickets.
