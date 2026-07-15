#!/usr/bin/env python3
"""Validate Casefile source boundaries, public names, and adapter bindings."""
from __future__ import annotations

import argparse
import os
import re
import sys
import tomllib
from pathlib import Path


CASEFILE_SKILLS = {
    "casefile-workflow",
    "casefile-investigate-solo",
    "casefile-investigate-atomic",
    "casefile-investigate-inspector-tree",
    "casefile-review-atomic",
    "casefile-review-dialogue",
    "casefile-review-two-stage",
    "casefile-implement-ticket-batch",
    "casefile-switch-strategy",
    "casefile-closeout",
}
REUSABLE_SKILLS = {
    "contract-bootstrap",
    "git-contribution",
    "skill-generator",
    "skill-packaging",
    "readme-generator",
}
EXCLUDED_SKILLS = {"build-code", "test-benchmark-code"}
ROLES = {
    "inspector",
    "detective",
    "dialogue-review-chair",
    "dialogue-review-challenger",
    "atomic-ticket-reviewer",
    "verification-reviewer",
    "implementation-writer",
}
OLD_PUBLIC_NAMES = {
    "-".join(parts)
    for parts in (
        ("planning", "workflow"),
        ("ticketed", "repository", "investigation"),
        ("investigation", "solo"),
        ("investigation", "atomic"),
        ("investigation", "inspector", "tree"),
        ("investigation", "review", "atomic"),
        ("investigation", "review", "dialogue"),
        ("investigation", "review", "two", "stage"),
        ("ticket", "batch", "subagent", "pipeline"),
        ("ticket", "scratch", "closeout"),
        ("implementation", "ticket", "batch"),
    )
}
TEXT_SUFFIXES = {".json", ".md", ".py", ".toml", ".txt", ".yaml", ".yml", ".in"}


def text_files(root: Path):
    ignored = {".git", ".agent-workspace", "plugins", "__pycache__"}
    for current, directories, names in os.walk(root):
        directories[:] = sorted(name for name in directories if name not in ignored)
        for name in sorted(names):
            path = Path(current) / name
            if path.suffix.lower() in TEXT_SUFFIXES or name in {"LICENSE"}:
                yield path


def load(path: Path, errors: list[str]) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        errors.append(f"invalid TOML {path}: {error}")
        return {}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    arguments = parser.parse_args()
    root = arguments.source.resolve()
    skills = root / "skills"
    workflow = root / "casefile-workflow"
    adapters = root / "adapters"
    errors: list[str] = []

    for name in sorted(CASEFILE_SKILLS | REUSABLE_SKILLS):
        if not (skills / name / "SKILL.md").is_file():
            errors.append(f"missing source skill: {name}")
    for old in OLD_PUBLIC_NAMES:
        if (skills / old).exists():
            errors.append(f"superseded skill directory remains: {old}")
    if (root / "-".join(("planning", "workflow"))).exists() or (root / ".agents/skills").exists() or (root / ".claude/skills").exists():
        errors.append("superseded workflow or discovery shim remains")

    portable_paths = [skills / name / "SKILL.md" for name in CASEFILE_SKILLS | REUSABLE_SKILLS]
    portable_paths += sorted((workflow / "roles").glob("*.md"))
    portable_paths += sorted((workflow / "schemas").glob("*.md"))
    portable_text = "\n".join(path.read_text(encoding="ascii") for path in portable_paths if path.is_file())
    for pattern in (r"\bCodex\b", r"\bClaude\b", r"gpt-", r"request_user_input", r"models_cache", r"\bsandbox\b"):
        if re.search(pattern, portable_text, re.IGNORECASE):
            errors.append(f"vendor contract leaked into portable source: {pattern}")

    for path in text_files(root):
        try:
            text = path.read_text(encoding="ascii")
        except UnicodeError:
            errors.append(f"non-ASCII active text: {path.relative_to(root)}")
            continue
        for old in OLD_PUBLIC_NAMES:
            if old in text:
                errors.append(f"superseded public name {old!r} in {path.relative_to(root)}")

    for role in ROLES | {"orchestrator"}:
        if not (workflow / "roles" / f"{role}.md").is_file():
            errors.append(f"missing portable role: {role}")
    for schema in (
        "decision.md",
        "investigation-layout.md",
        "project-map.md",
        "strategy-matrix.md",
        "strategy-transition.md",
        "ticket.md",
        "verification.md",
    ):
        if not (workflow / "schemas" / schema).is_file():
            errors.append(f"missing workflow schema: {schema}")
    for script in ("validate-project-map.py", "switch-strategy.py"):
        path = workflow / "scripts" / script
        if not path.is_file() or not os.access(path, os.X_OK):
            errors.append(f"missing or non-executable workflow script: {script}")

    expected_bindings = {
        "codex": {
            "inspector": ("gpt-5.6-terra", "xhigh"),
            "detective": ("gpt-5.6-terra", "medium"),
            "dialogue-review-chair": ("gpt-5.6-terra", "xhigh"),
            "dialogue-review-challenger": ("gpt-5.6-terra", "xhigh"),
            "atomic-ticket-reviewer": ("gpt-5.6-terra", "xhigh"),
            "verification-reviewer": ("gpt-5.6-terra", "medium"),
            "implementation-writer": ("gpt-5.6-terra", "high"),
        },
        "claude": {
            "inspector": ("opus", "high"),
            "detective": ("sonnet", "medium-high"),
            "dialogue-review-chair": ("opus", "high"),
            "dialogue-review-challenger": ("sonnet", "medium-high"),
            "atomic-ticket-reviewer": ("sonnet", "medium-high"),
            "verification-reviewer": ("haiku", "medium"),
            "implementation-writer": ("sonnet", "medium-high"),
        },
    }
    expected_matrix_ids = CASEFILE_SKILLS - {"casefile-workflow", "casefile-switch-strategy", "casefile-closeout"}
    for adapter, bindings in expected_bindings.items():
        matrix_dir = adapters / adapter / "matrices"
        matrices = sorted(matrix_dir.glob("*.toml"))
        if {path.stem for path in matrices} != expected_matrix_ids:
            errors.append(f"{adapter} matrix set does not match Casefile strategy set")
        for path in matrices:
            document = load(path, errors)
            if document.get("strategy_id") != path.stem or document.get("adapter") != adapter:
                errors.append(f"matrix identity mismatch: {path}")
            if document.get("orchestrator", {}).get("binding") != "root":
                errors.append(f"matrix root mismatch: {path}")
            for worker in document.get("workers", []):
                role = worker.get("role")
                if role not in bindings or (worker.get("model"), worker.get("reasoning")) != bindings[role]:
                    errors.append(f"matrix binding mismatch: {path.name}:{role}")

    profiles = load(adapters / "codex/profiles.toml", errors)
    if (profiles.get("root", {}).get("model"), profiles.get("root", {}).get("reasoning")) != ("gpt-5.6-sol", "xhigh"):
        errors.append("Codex root profile must bind Sol/xhigh")
    workers = {item.get("role"): (item.get("model"), item.get("reasoning")) for item in profiles.get("workers", [])}
    if workers != expected_bindings["codex"]:
        errors.append("Codex canonical worker profiles do not match accepted bindings")
    fragment = (adapters / "codex/config-fragment.toml.in").read_text(encoding="ascii")
    if "multi_agent = true" not in fragment or "multi_agent_v2 = false" not in fragment:
        errors.append("Codex V1 feature contract is missing")

    claude_profiles = load(adapters / "claude/profiles.toml", errors)
    claude_workers = {
        item.get("role"): (
            item.get("model"),
            item.get("policy_tier"),
            item.get("frontmatter_effort"),
        )
        for item in claude_profiles.get("workers", [])
    }
    for role, (model, policy_tier) in expected_bindings["claude"].items():
        expected_effort = "high" if policy_tier == "medium-high" else policy_tier
        if claude_workers.get(role) != (model, policy_tier, expected_effort):
            errors.append(f"Claude profile mapping mismatch: {role}")
        agent = adapters / "claude/agents" / f"{role}.md"
        header = agent.read_text(encoding="ascii").split("---", 2)[1]
        metadata = {
            key.strip(): value.strip()
            for line in header.splitlines()
            if ":" in line
            for key, value in [line.split(":", 1)]
        }
        if (metadata.get("model"), metadata.get("effort")) != (model, expected_effort):
            errors.append(f"Claude agent frontmatter mismatch: {role}")

    for manifest_path in sorted((root / "packaging/plugins").glob("*.toml")):
        manifest = load(manifest_path, errors)
        sources = {item.get("source") for item in manifest.get("copy", []) if isinstance(item, dict)}
        for excluded in EXCLUDED_SKILLS:
            if f"skills/{excluded}" in sources:
                errors.append(f"excluded skill appears in {manifest_path}: {excluded}")
    if errors:
        print("Casefile validation failed:", *errors, sep="\n- ")
        return 1
    print(
        f"validated {len(CASEFILE_SKILLS)} Casefile skills, {len(REUSABLE_SKILLS)} reusable skills, "
        f"{len(ROLES)} roles, and 14 adapter matrices"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
