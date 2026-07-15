<div align="center">

# HUMANS.md

**A behavioural instruction system for AI coding agents, built on one claim: always-loaded instructions should be a conduct contract, not a repository encyclopedia.**

</div>

Most agent instruction files drift toward encyclopedias. Setup commands, style fragments, project lore, and generic advice all compete for always-loaded context, and the evidence this repository cites suggests that posture can reduce task success while increasing cost. This project takes the opposite stance and ships the working system that follows from it: a small standing contract that shapes conduct, skills that carry task knowledge only when a task needs it, tooling for rules that must be guaranteed, and a rationale document that keeps the design from collapsing back into boilerplate.

The aim, in the project's own words, is controlled acceleration: faster agent work under tighter human ownership.

## The Claim

Always-loaded instructions change an agent's defaults before any task is understood. That makes them powerful and dangerous for the same reason, so they must earn their place. The system here splits the instruction surface into layers, each with one job:

- **Conduct** lives in the standing contract: stay inside the task, surface consequential choices, keep scope visible, leave reviewable work.
- **Task knowledge** lives in skills, loaded deliberately through progressive disclosure.
- **Rules with hard consequences** belong in tooling: hooks, permissions, tests, CI. Prose is not enforcement.
- **Rationale** lives with the humans who maintain the system, not in agent context.

The full argument, with its references and anti-patterns, is in [`HUMANS.md`](HUMANS.md).

## The Layers

| Artifact | Role | Loaded |
| --- | --- | --- |
| [`AGENTS.md`](AGENTS.md) | The active behaviour contract: bounded scope, human authority, explicit assumptions, verification, reviewable handoff | Always, by agents |
| [`CLAUDE.md`](CLAUDE.md) | Compatibility pointer to `AGENTS.md`, never a second source of truth | Always, by Claude Code |
| [`HUMANS.md`](HUMANS.md) | The design rationale, written for humans maintaining the instruction system | Never, unless a human asks |
| [`skills/`](skills/) | Task models loaded on demand when their task is in hand | Per task |
| [`docs/`](docs/) | Records of experiments run on the instruction system itself | Never |
| `.agent-workspace/` | Session-scoped scratch state, disposable by design | Per session |

## Skills

The repository ships focused skills, including:

- [`skill-generator`](skills/skill-generator/SKILL.md): create, revise, or audit a skill. It treats the task model as the skill: jobs, variation, requester-owned choices, and activation boundary are built and confirmed before any body is written.
- [`skill-packaging`](skills/skill-packaging/SKILL.md): package, validate, port, or diagnose a skill for a specific platform, without letting the platform reshape the skill.
- [`readme-generator`](skills/readme-generator/SKILL.md): create or maintain a repository README as a capture of project intent, grounded in repository facts rather than invention.
- [`ticketed-repository-investigation`](skills/ticketed-repository-investigation/SKILL.md): coordinate portable, ticket-producing investigation with explicit strategy selection and human-owned contention.
- Investigation and review strategy skills select solo, atomic, hierarchical, dialogue, atomic-review, or two-stage execution without embedding platform defaults.
- [`ticket-batch-subagent-pipeline`](skills/ticket-batch-subagent-pipeline/SKILL.md) and [`ticket-scratch-closeout`](skills/ticket-scratch-closeout/SKILL.md): consume selected implementation matrices and promote resolved planning records through configured adapters.

Reusable role contracts, schemas, and the first platform adapter live in [`planning-workflow/`](planning-workflow/). Operational records remain outside this repository in the private `agent-planning` store.

## Method

Skills here are not taken on faith. They are forward-tested: a fresh-context agent is given the skill and a realistic request seeded with traps, and its behaviour is judged against the skill's own task model. [`docs/2026-06-11-skill-creation-forward-test.md`](docs/2026-06-11-skill-creation-forward-test.md) records one such experiment, a head-to-head comparison of two skill-creation skills that ended in a merge, with the surviving instruments named and the open questions kept visible.

## Engaging With This Repository

- **To adopt the posture**: read [`AGENTS.md`](AGENTS.md). It is the canonical active contract and small enough to read in a minute.
- **To maintain or evaluate the system**: read [`HUMANS.md`](HUMANS.md) first. Every layer boundary in this repository exists for a reason recorded there, including the criteria for what may enter the contract at all.
- **To add task knowledge**: write a skill, using `skill-generator`. If a candidate addition only explains the codebase, it belongs in documentation; if violating it would be materially harmful, it needs enforcement, not prose.

## Status

Small by design. There is no build step, package manager, or test suite; the artifacts are the instruction files themselves and the experiments run against them. Released under the [MIT licence](LICENSE).
