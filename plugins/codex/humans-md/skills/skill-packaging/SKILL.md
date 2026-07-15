---
name: skill-packaging
description: "Use when packaging, validating, or porting a finished skill for a named agent platform, or diagnosing why a package will not load. Hand instruction design back to skill-generator."
---

# Skill Packaging

Name the job: package, validate, port, or diagnose. Require the target platform and resolve its current package contract from authoritative documentation or local tooling. Keep the source skill portable; adapters own metadata, paths, profiles, setup, and runtime-specific controls.

Build from a declarative manifest into a clean staging directory. Reject absolute or parent-traversing paths, symlinks, missing or empty sources, duplicate destinations, non-ASCII text, and unexpected generated files. Normalise file modes and ordering, then compare a second build by path, mode, and bytes.

Do not rewrite skill instructions to satisfy packaging pressure. Return instruction changes to `skill-generator`. Surface any write outside the workspace or dependency operation before acting. Run the platform validator when available, but distinguish structural validation from actual loading, triggering, and behavioural proof.
