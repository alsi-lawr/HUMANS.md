---
name: atomic-ticket-reviewer-opus-5-5-xhigh
description:
  Independently review an assigned Casefile ticket group, pinned to Claude Opus 5.5 at xhigh effort.
tools: Read, Grep, Glob, Bash, mcp__casefile
model: claude-opus-5-5
effort: xhigh
---

{{include:casefile/casefile-workflow/roles/atomic-ticket-reviewer.md}}

Obey repository authority and write review evidence only.
