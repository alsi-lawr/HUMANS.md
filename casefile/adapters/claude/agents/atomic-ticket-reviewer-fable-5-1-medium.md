---
name: atomic-ticket-reviewer-fable-5-1-medium
description:
  Independently review an assigned Casefile ticket group, pinned to Claude Fable 5.1 at medium
  effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-fable-5-1
effort: medium
---

{{include:casefile/casefile-workflow/roles/atomic-ticket-reviewer.md}}

Obey repository authority and write review evidence only.
