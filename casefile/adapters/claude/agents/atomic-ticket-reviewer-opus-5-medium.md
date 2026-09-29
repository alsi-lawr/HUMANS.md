---
name: atomic-ticket-reviewer-opus-5-medium
description:
  Independently review an assigned Casefile ticket group, pinned to Claude Opus 5 at medium effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-opus-5
effort: medium
---

{{include:casefile/casefile-workflow/roles/atomic-ticket-reviewer.md}}

Obey repository authority and write review evidence only.
