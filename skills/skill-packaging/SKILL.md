---
name: skill-packaging
description: "Use when packaging, installing, validating, or porting an agent skill for a specific platform: preparing a skill for Codex, Claude, Gemini, or another skill system, adapting metadata or archive shape, or diagnosing why a platform will not load a skill. Hand writing or revising the skill's instructions to skill-generator."
---

# Skill Packaging

## Purpose

Carry a finished skill onto a target platform without letting the platform reshape the skill. Packaging is an adapter over a coherent skill, never the source of its design.

## Job In Hand

- **Package**: produce the platform's expected shape from a portable source skill.
- **Validate**: run the platform's checks against an existing package.
- **Port**: move a skill between platforms, preserving its task model.
- **Diagnose**: find why a platform refuses or mishandles a skill.

If the target platform is not named and cannot be inferred, ask: the platform decides the package shape. Keep the source skill portable regardless of target.

## Platform Knowledge

Do not assume platforms share metadata fields, resource folders, invocation syntax, or loading behavior. Verify against the platform's current documentation or local tooling, not memory.

- Codex-style skills commonly use `name` and `description` frontmatter plus optional bundled resources. Prefer local helper scripts for initialization and validation when they exist over hand-building metadata.
- Claude-style skills may support additional frontmatter, invocation controls, and tool restrictions.
- Registry-based systems may impose packaging, naming, description-length, and archive-safety constraints.
- For an unknown system, deliver the complete portable package and name the target-specific packaging gap as open risk.

## Boundaries

Change packaging artifacts, not the skill's instructions. If packaging pressure demands an instruction change, such as a length or metadata constraint, surface it for skill-generator work rather than quietly rewording the body.

Surface before proceeding when packaging requires writing outside the current workspace or installing dependencies.

## Verification

Run the platform's own validator when one exists; treat its output as evidence, not authority. When no validator exists, report exactly what was not verified: installation, loading, triggering. A package that was never loaded on its platform is unproven; say so.
