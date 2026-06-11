# HUMANS.md

HUMANS.md is a compact instruction-system project for AI coding agents. It separates always-loaded agent conduct, human-maintainer rationale, and task-specific skills so agents can move quickly without losing scope, reviewability, or human authority.

## Why It Exists

Most agent instruction files drift toward encyclopedias: setup notes, style fragments, project lore, and generic advice all compete for always-loaded context. This project takes the opposite stance. Durable instructions should shape default behavior; detailed task knowledge should load only when needed; rules that need hard edges should move into tooling.

The result is a smaller, sharper control surface for agent work:

- `AGENTS.md` defines the active behavior contract.
- `HUMANS.md` preserves the design rationale for humans maintaining that contract.
- `skills/` holds focused, progressive-disclosure task guidance.

## Core Ideas

`AGENTS.md` is not a repository map. It is the standing behavior layer agents should carry into most tasks: stay bounded, surface assumptions, verify what matters, leave reviewable work, and keep the target artifact primary.

`HUMANS.md` is not loaded as agent instruction by default. It explains why the contract is shaped this way, including the preference for layer boundaries, progressive disclosure, useful friction, scratch-state closure, and guardrails against both under-modelled work and over-engineering.

Skills are the place for repeatable task knowledge. They should be narrow, triggerable, and practical. A skill may be detailed because it is loaded deliberately, not because every task needs it.

## Included Skills

### `skill-generator`

Guides agents through creating, revising, or auditing other skills. It emphasizes artifact-first authoring, clear trigger metadata, non-goals, platform-aware packaging, and resistance to turning every skill into a miniature framework.

### `readme-generator`

Guides agents through creating project READMEs with staggered disclosure: description first, clean pitch next, then only the overview, installation, usage, and development sections the repository actually warrants.

## Using This Repo

Use `AGENTS.md` as the canonical active contract for agents working in the repository.

Use `HUMANS.md` when editing or evaluating the instruction system itself. It is human rationale, not runtime instruction text to copy wholesale into agent context.

Use skills from `skills/` when the task matches their trigger. Keep new skills focused on recurring task capabilities rather than general conduct, broad project documentation, or rules that need enforcement.

## Status

This repository is small by design. There is no build step, package manager, or test suite in the current project state.
