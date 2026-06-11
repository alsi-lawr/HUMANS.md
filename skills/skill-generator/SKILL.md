---
name: skill-generator
description: Use when creating, revising, or auditing an AI-agent skill for a recurring task capability. Trigger for requests to write SKILL.md files, design skill metadata, narrow skill scope, decide whether a request belongs in a skill, or adapt a skill across Codex, Claude, Gemini, or another progressive-disclosure skill system.
---

# Skill Generator

## Purpose

Create agent-facing skills that help with a recurring class of work without becoming always-loaded instructions, repository encyclopedias, generic prompts, or miniature software projects.

Keep the target skill artifact primary. Guardrails, templates, scripts, schemas, validators, and platform adapters may support the skill, but they should not become the most salient feature of the work.

## Fit Gate

Before writing, decide whether the request belongs in a skill.

Proceed when the user wants reusable, task-specific guidance that should load only when relevant.

Push back when the request is better handled elsewhere:

- Standing agent behavior belongs in `AGENTS.md`, `CLAUDE.md`, system instructions, or policy.
- Broad repository knowledge belongs in a README, reference document, design note, or a narrowly triggered repo skill.
- Generic assistant tone or persona belongs in prompt/configuration, not a task skill.
- Rules with hard consequences belong in hooks, permissions, linting, tests, CI, policy checks, or other enforcement.
- Large workflow families should be split into smaller skills with visible boundaries.

When the request is close but too broad, propose the narrowest useful skill boundary before drafting.

## Artifact First

Write the smallest useful `SKILL.md` before adding machinery around it.

Formal structure must earn its place by protecting a concrete boundary, reducing repeated work, lowering review burden, or enforcing a rule prose cannot safely carry. If that need is speculative, keep the skill manual and local. If a generator, schema, script, or validator seems useful, name the failure it prevents and leave it as a proposed follow-up unless the user asked for it.

## Required Inputs

Use the user's request, any concrete example prompts, and the target platform if named.

Ask only when the missing answer changes scope, public behavior, persistence, security, platform compatibility, or generated files. Otherwise choose conservative defaults and record the assumption.

If examples are missing, invent a small set of likely positive and negative trigger prompts and mark them as assumed. Ask the user only when the boundary remains consequentially ambiguous.

Default assumptions:

- Target platform is portable unless the user names a platform.
- The first artifact is a `SKILL.md`.
- The skill is instruction-only unless a supporting file clearly reduces context load, repeated work, or fragility.
- Generated skills should be brief but not thin: compressed guidance with enough force to change agent behavior.

## Workflow

1. State the intended skill boundary in one sentence.
2. Gather or infer concrete example prompts before writing instructions.
3. List positive triggers: requests that should activate the skill.
4. List negative triggers: nearby requests that should not activate it.
5. Identify what is in scope, what is out of scope, and what assumptions affect behavior.
6. Choose the package shape: portable source, installable package for a named platform, or update to an existing package.
7. Draft the `SKILL.md` metadata first; put trigger conditions in `description`.
8. Draft the body as operational instructions for the agent that will use the skill.
9. Add supporting files only after the main skill shows a real need for them.
10. Review for scope drift, context bloat, hidden decisions, weak verification, over-engineering, and package validity.
11. Revise once or twice if needed; stop after three correction passes and report unresolved risk.

## Naming And Package Shape

Use a short, action-oriented skill name. Normalize folders to lowercase letters, digits, and hyphens. Namespace by a tool or domain only when it improves triggering, such as `gh-address-comments` or `pdf-redline`.

Default package shape:

```text
skill-name/
  SKILL.md
  references/   # optional, loaded only when needed
  scripts/      # optional, deterministic repeated operations
  assets/       # optional, templates or reusable output material
  examples/     # optional, validation fixtures or platform convention
```

Do not create optional folders until they have contents with a clear use.

## Writing The Skill

Use direct instructions. Prefer concrete agent behavior over explanations of theory.

The skill body should usually include:

- `Purpose`: the recurring capability.
- `Fit Gate` or `When To Use`: the decision boundary, if the metadata needs reinforcement.
- `Workflow`: the actions to take.
- `Boundaries`: in-scope work and non-goals.
- `Ask Or Stop`: choices the agent should surface before continuing.
- `Verification`: the narrow checks that matter.
- `Report Back`: what the agent should tell the user.

Avoid sections that explain why skills exist, narrate the authoring process, or preserve design rationale for humans. Put human rationale in project documentation, not in the generated runtime skill.

## Supporting Files

Add supporting files only when they improve the skill's use.

- Use `references/` for details the agent should load only in specific cases.
- Use `scripts/` for deterministic, repeated, or fragile operations.
- Use `assets/` for templates or reusable output material.
- Use examples only when they test or clarify realistic use.

Every referenced file needs a clear load condition in `SKILL.md`. Avoid duplicating the same content in the body and a reference.

Do not add README, changelog, install guide, broad examples, or scaffolding just because they look complete.

Test any script that is shipped with the skill. If a script cannot be run safely in the current environment, report that explicitly.

## Platform Notes

Keep the core skill portable. Add platform-specific fields only when the target platform supports them and the behavior needs them.

- Codex-style skills commonly use `name` and `description` frontmatter plus optional bundled resources.
- Claude-style skills may support additional frontmatter, invocation controls, and tool restrictions.
- Gemini or registry-based systems may impose packaging, naming, description-length, and archive-safety constraints.
- Other systems may need a small manifest or adapter, but the core instructions should still stand on their own.

Do not assume all platforms share resource folders, invocation syntax, metadata fields, or automatic loading behavior.

Packaging decision:

- If the user wants a portable skill, create the source package and report that platform installation was not verified.
- If the user names a platform, produce that platform's expected package shape and run its validator when available.
- If the platform is Codex and local helper scripts exist, prefer the helper scripts for initialization or validation rather than hand-building generated metadata.
- If packaging rules are unknown, keep the portable package complete and list the target-specific packaging uncertainty as risk.
- If packaging the skill requires writing outside the current workspace or installing dependencies, surface that before proceeding.

Treat packaging as an adapter over a coherent skill, not as the source of the skill's design.

## Verification

Before finalizing, check:

- The description clearly says when to use the skill.
- Positive and negative triggers create a visible boundary.
- The body is short enough to load comfortably but specific enough to change behavior.
- Non-goals prevent the skill from absorbing adjacent workflows.
- Supporting files are justified and conditionally loaded.
- Rules needing hard consequences point to enforcement rather than prose alone.
- Tooling or scaffolding has not become the main artifact.
- The package shape matches the target platform, or packaging uncertainty is visible where it matters.

If a platform validator or local skill audit exists, run the narrowest relevant check. Treat its output as evidence, not authority.

For complex or high-risk skills, forward-test with realistic prompts in a fresh context when possible. Pass the skill and the user-like task, not your expected answer. Review the output as evidence, then clean up scratch artifacts before handoff.

## Report Back

Leave a review surface in the shape the task deserves.

For a small skill edit, a short paragraph may be enough. For a new package, packaging work, validation, or unresolved uncertainty, make the useful facts easy to find: files changed, target platform, assumptions that affected the result, why supporting files were or were not added, what was checked, and remaining risk.

Do not emit a fixed checklist or fill empty categories. Reviewability should reduce the human's inspection burden, not add ceremony.
