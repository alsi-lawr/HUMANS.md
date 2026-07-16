#!/usr/bin/env python3
"""Validate Casefile source or installed-package boundaries and adapter bindings."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import tomllib
from pathlib import Path


CASEFILE_SKILLS = {
    "casefile",
    "casefile-investigate",
    "casefile-review",
    "casefile-implement",
    "casefile-switch",
    "casefile-close",
}
SUPERSEDED_SKILL_DIRS = {
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
    "look-ahead-investigator",
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
LEGACY_CLEANUP_FILES = {
    "adapters/codex/scripts/setup-codex.py",
    "scripts/setup-codex.py",
    "tests/test_codex_setup.py",
}
EXPECTED_BINDINGS = {
    "codex": {
        "inspector": ("gpt-5.6-terra", "xhigh"),
        "detective": ("gpt-5.6-terra", "medium"),
        "dialogue-review-chair": ("gpt-5.6-terra", "xhigh"),
        "dialogue-review-challenger": ("gpt-5.6-terra", "xhigh"),
        "atomic-ticket-reviewer": ("gpt-5.6-terra", "xhigh"),
        "verification-reviewer": ("gpt-5.6-terra", "medium"),
        "implementation-writer": ("gpt-5.6-terra", "high"),
        "look-ahead-investigator": ("gpt-5.6-luna", "medium"),
    },
    "claude": {
        "inspector": ("opus", "high"),
        "detective": ("sonnet", "medium-high"),
        "dialogue-review-chair": ("opus", "high"),
        "dialogue-review-challenger": ("sonnet", "medium-high"),
        "atomic-ticket-reviewer": ("sonnet", "medium-high"),
        "verification-reviewer": ("haiku", "medium"),
        "implementation-writer": ("sonnet", "medium-high"),
        "look-ahead-investigator": ("haiku", "medium"),
    },
}
EXPECTED_MATRIX_IDS = {
    "casefile-investigate-solo",
    "casefile-investigate-atomic",
    "casefile-investigate-inspector-tree",
    "casefile-review-atomic",
    "casefile-review-dialogue",
    "casefile-review-two-stage",
    "casefile-implement-ticket-batch",
    "casefile-implement-pipeline",
}

PIPELINE_COORDINATION = {
    "maximum_active_tickets": 2,
    "look_ahead_read_only": True,
    "require_dependency_independence": True,
    "require_disjoint_write_paths": True,
    "immutable_review_commits": True,
    "corrections_preempt_forward_work": True,
}


def text_files(root: Path, source_layout: bool):
    ignored = {".git", ".agent-workspace", "__pycache__"}
    if source_layout:
        ignored.add("plugins")
    for current, directories, names in os.walk(root):
        directories[:] = sorted(name for name in directories if name not in ignored)
        for name in sorted(names):
            path = Path(current) / name
            if path.suffix.lower() in TEXT_SUFFIXES or name == "LICENSE":
                yield path


def load_toml(path: Path, errors: list[str]) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        errors.append(f"invalid TOML {path}: {error}")
        return {}


def load_json(path: Path, errors: list[str]) -> dict:
    try:
        value = json.loads(path.read_text(encoding="ascii"))
        if not isinstance(value, dict):
            raise ValueError("JSON root is not an object")
        return value
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        errors.append(f"invalid JSON {path}: {error}")
        return {}


def layout(root: Path) -> str:
    if (root / "adapters/codex").is_dir() and (root / "packaging/plugins").is_dir():
        return "source"
    if (root / ".codex-plugin/plugin.json").is_file():
        return "codex-package"
    if (root / ".claude-plugin/plugin.json").is_file():
        return "claude-package"
    return "unknown"


def common_validation(root: Path, kind: str, errors: list[str]) -> None:
    skills = root / "skills"
    workflow = root / "casefile-workflow"
    for name in sorted(CASEFILE_SKILLS | REUSABLE_SKILLS):
        if not (skills / name / "SKILL.md").is_file():
            errors.append(f"missing included skill: {name}")
    for old in OLD_PUBLIC_NAMES | SUPERSEDED_SKILL_DIRS:
        if (skills / old).exists():
            errors.append(f"superseded skill directory remains: {old}")
    if kind == "source" and (
        (root / "-".join(("planning", "workflow"))).exists()
        or (root / ".agents/skills").exists()
        or (root / ".claude/skills").exists()
        or (root / "CLAUDE.md").exists()
    ):
        errors.append("superseded workflow, root pointer, or discovery shim remains")

    portable_names = CASEFILE_SKILLS | REUSABLE_SKILLS
    if kind == "codex-package":
        portable_names = portable_names - {"git-contribution"}
    portable_paths = [skills / name / "SKILL.md" for name in portable_names]
    portable_paths += sorted((workflow / "roles").glob("*.md"))
    portable_paths += sorted((workflow / "schemas").glob("*.md"))
    portable_text = "\n".join(
        path.read_text(encoding="ascii") for path in portable_paths if path.is_file()
    )
    for pattern in (
        r"\bCodex\b",
        r"\bClaude\b",
        r"gpt-",
        r"request_user_input",
        r"models_cache",
        r"\bsandbox\b",
    ):
        if re.search(pattern, portable_text, re.IGNORECASE):
            errors.append(f"vendor contract leaked into portable source: {pattern}")

    for path in text_files(root, kind == "source"):
        try:
            text = path.read_text(encoding="ascii")
        except UnicodeError:
            errors.append(f"non-ASCII active text: {path.relative_to(root)}")
            continue
        relative = path.relative_to(root).as_posix()
        if relative not in LEGACY_CLEANUP_FILES:
            for old in OLD_PUBLIC_NAMES:
                if old in text:
                    errors.append(f"superseded public name {old!r} in {relative}")

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
    template = root / "AGENTS.md" if kind == "source" else root / "templates/AGENTS.md"
    if not template.is_file() or template.read_bytes() != (
        (root / "AGENTS.md").read_bytes() if kind == "source" else template.read_bytes()
    ):
        errors.append("AGENTS.md contract template is missing")
    suite = load_toml(root / "verification/suites/casefile.toml", errors)
    suite_skills = {
        case.get("skill") for case in suite.get("cases", []) if isinstance(case, dict)
    }
    if suite_skills != CASEFILE_SKILLS | REUSABLE_SKILLS:
        errors.append("verification suite does not cover every included portable skill")


def matrix_validation(
    adapter: str,
    matrix_dir: Path,
    errors: list[str],
) -> list[tuple[dict, dict]]:
    matrices = sorted(matrix_dir.glob("*.toml"))
    if {path.stem for path in matrices} != EXPECTED_MATRIX_IDS:
        errors.append(f"{adapter} matrix set does not match Casefile strategy set")
    workers: list[tuple[dict, dict]] = []
    for path in matrices:
        document = load_toml(path, errors)
        if document.get("strategy_id") != path.stem or document.get("adapter") != adapter:
            errors.append(f"matrix identity mismatch: {path}")
        if document.get("orchestrator", {}).get("binding") != "root":
            errors.append(f"matrix root mismatch: {path}")
        matrix_roles = set()
        for worker in document.get("workers", []):
            role = worker.get("role")
            matrix_roles.add(role)
            if role not in EXPECTED_BINDINGS[adapter] or (
                worker.get("model"), worker.get("reasoning")
            ) != EXPECTED_BINDINGS[adapter][role]:
                errors.append(f"matrix binding mismatch: {path.name}:{role}")
            workers.append((document, worker))
        pipeline = document.get("coordination", {}).get("pipeline")
        if path.stem == "casefile-implement-pipeline":
            expected_roles = {
                "implementation-writer",
                "look-ahead-investigator",
                "atomic-ticket-reviewer",
                "verification-reviewer",
            }
            if matrix_roles != expected_roles:
                errors.append(f"pipeline worker set mismatch: {path}")
            if pipeline != PIPELINE_COORDINATION:
                errors.append(f"pipeline coordination mismatch: {path}")
        elif pipeline is not None:
            errors.append(f"pipeline coordination on non-pipeline matrix: {path}")
    return workers


def codex_validation(adapter_root: Path, errors: list[str]) -> None:
    package_root = adapter_root.parent if adapter_root.name == "config" else None
    matrix_dir = package_root / "matrices" if package_root else adapter_root / "matrices"
    agent_dir = package_root / "agents" if package_root else adapter_root / "agents"
    skill_dir = package_root / "skills" if package_root else adapter_root / "skills"
    for name in ("codex-setup", "codex-uninstall"):
        if not (skill_dir / name / "SKILL.md").is_file():
            errors.append(f"Codex lifecycle skill is missing: {name}")
    for name in (
        "casefile-codex-setup",
        "casefile-codex-cutover",
        "casefile-codex-catalog-profile",
        "casefile-codex-uninstall",
    ):
        if (skill_dir / name).exists():
            errors.append(f"superseded Codex lifecycle skill remains: {name}")
    workers = matrix_validation("codex", matrix_dir, errors)
    profiles = load_toml(adapter_root / "profiles.toml", errors)
    if "root" in profiles or (adapter_root / "root.config.toml").exists():
        errors.append("Codex must not bind the request-receiving orchestrator model")
    canonical = {
        (item.get("strategy_id"), item.get("role")): item
        for item in profiles.get("matrix_profiles", [])
        if isinstance(item, dict)
    }
    expected_keys = {
        (matrix.get("strategy_id"), worker.get("role")) for matrix, worker in workers
    }
    if set(canonical) != expected_keys:
        errors.append("Codex canonical matrix-profile set does not match matrix workers")
    for matrix, worker in workers:
        key = (matrix.get("strategy_id"), worker.get("role"))
        row = canonical.get(key, {})
        expected_profile = f"{key[0]}-{key[1]}"
        if worker.get("platform_profile") != expected_profile or row.get("profile") != expected_profile:
            errors.append(f"Codex matrix-specific profile mismatch: {key}")
            continue
        if (row.get("model"), row.get("reasoning")) != (
            worker.get("model"),
            worker.get("reasoning"),
        ):
            errors.append(f"Codex canonical profile binding mismatch: {expected_profile}")
        agent = agent_dir / Path(row.get("agent_file", "")).name
        document = load_toml(agent, errors)
        if (document.get("model"), document.get("model_reasoning_effort")) != (
            worker.get("model"),
            worker.get("reasoning"),
        ):
            errors.append(f"Codex agent binding mismatch: {agent}")

    fragment = (adapter_root / "config-fragment.toml.in").read_text(encoding="ascii")
    if "multi_agent = true" not in fragment or "multi_agent_v2 = false" not in fragment:
        errors.append("Codex V1 feature contract is missing")
    for row in canonical.values():
        profile = row["profile"]
        if f"[agents.{profile}]" not in fragment or f"agents/{profile}.toml" not in fragment:
            errors.append(f"Codex config fragment lacks named profile: {profile}")

    catalog = profiles.get("catalog", {})
    if catalog.get("selector_fields") != ["multi_agent_version"]:
        errors.append("Codex catalog selector policy must target multi_agent_version only")
    targets = catalog.get("targets", [])
    if len(targets) != 8 or len({item.get("id") for item in targets}) != 8:
        errors.append("Codex catalog must target all eight authored model resources")
    declared_nulls = set()
    for target in targets:
        model_id = target.get("id")
        for field, hash_field in (
            ("base_instructions_file", "base_instructions_sha256"),
            ("model_messages_file", "model_messages_sha256"),
        ):
            path = adapter_root / target.get(field, "")
            if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != target.get(
                hash_field
            ):
                errors.append(f"Codex authored resource missing or hash-mismatched: {model_id}:{field}")
        nulls = target.get("null_selectors", [])
        if set(nulls) - {"multi_agent_version"}:
            errors.append(f"Codex target declares an unsupported selector: {model_id}")
        if "multi_agent_version" in nulls:
            declared_nulls.add(model_id)
    if not {"gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"} <= declared_nulls:
        errors.append("Codex V1 models must null multi_agent_version")

    git_reference = adapter_root.parent / "skills/git-contribution/references/codex-github-cli.md"
    if adapter_root.name == "config":
        git_reference = adapter_root.parent / "skills/git-contribution/references/codex-github-cli.md"
    else:
        git_reference = adapter_root / "skills/git-contribution/references/codex-github-cli.md"
    if not git_reference.is_file() or "first attempt" not in git_reference.read_text(encoding="ascii"):
        errors.append("Codex Git contribution elevation reference is missing")
    for script in ("setup-codex.py",):
        path = adapter_root.parent / "scripts" / script if adapter_root.name == "config" else adapter_root / "scripts" / script
        if not path.is_file() or not os.access(path, os.X_OK):
            errors.append(f"Codex adapter script missing or non-executable: {script}")


def claude_validation(adapter_root: Path, errors: list[str]) -> None:
    package_root = adapter_root.parent if adapter_root.name == "config" else None
    matrix_dir = package_root / "matrices" if package_root else adapter_root / "matrices"
    skill_dir = package_root / "skills" if package_root else adapter_root / "skills"
    if not (skill_dir / "claude-setup/SKILL.md").is_file():
        errors.append("Claude setup skill is missing: claude-setup")
    if (skill_dir / "casefile-claude-setup").exists():
        errors.append("superseded Claude setup skill remains: casefile-claude-setup")
    matrix_validation("claude", matrix_dir, errors)
    profiles = load_toml(adapter_root / "profiles.toml", errors)
    workers = {
        item.get("role"): (
            item.get("model"),
            item.get("policy_tier"),
            item.get("frontmatter_effort"),
        )
        for item in profiles.get("workers", [])
    }
    agents = package_root / "agents" if package_root else adapter_root / "agents"
    for role, (model, policy_tier) in EXPECTED_BINDINGS["claude"].items():
        expected_effort = "high" if policy_tier == "medium-high" else policy_tier
        if workers.get(role) != (model, policy_tier, expected_effort):
            errors.append(f"Claude profile mapping mismatch: {role}")
        agent = agents / f"{role}.md"
        try:
            header = agent.read_text(encoding="ascii").split("---", 2)[1]
        except (OSError, UnicodeError, IndexError):
            errors.append(f"invalid Claude agent: {agent}")
            continue
        metadata = {
            key.strip(): value.strip()
            for line in header.splitlines()
            if ":" in line
            for key, value in [line.split(":", 1)]
        }
        if (metadata.get("model"), metadata.get("effort")) != (model, expected_effort):
            errors.append(f"Claude agent frontmatter mismatch: {role}")


def manifest_validation(root: Path, errors: list[str]) -> None:
    manifests = sorted((root / "packaging/plugins").glob("*.toml"))
    if [path.name for path in manifests] != ["humans-md.toml"]:
        errors.append("source must contain one portable humans-md product manifest")
        return
    manifest = load_toml(manifests[0], errors)
    expected_identity = {
        "name": "humans-md",
        "version": "0.1.4",
        "publisher": "alsi-lawr",
        "repository": "alsi-lawr/HUMANS.md",
        "license": "MIT",
    }
    for field, value in expected_identity.items():
        if manifest.get(field) != value:
            errors.append(f"portable product manifest identity mismatch: {field}")
    if set(manifest.get("vendors", {})) != {"codex", "claude"}:
        errors.append("portable product manifest must declare Codex and Claude adapters")
    shared_sources = {
        item.get("source") for item in manifest.get("shared", []) if isinstance(item, dict)
    }
    if "AGENTS.md" not in shared_sources:
        errors.append("portable product manifest must bundle the AGENTS.md template")
    for excluded in EXCLUDED_SKILLS:
        if f"skills/{excluded}" in shared_sources:
            errors.append(f"excluded skill appears in product manifest: {excluded}")


def package_metadata(root: Path, vendor: str, errors: list[str]) -> None:
    manifest_path = (
        root / ".codex-plugin/plugin.json"
        if vendor == "codex"
        else root / ".claude-plugin/plugin.json"
    )
    manifest = load_json(manifest_path, errors)
    expected = {
        "name": "humans-md",
        "version": "0.1.4",
        "repository": "https://github.com/alsi-lawr/HUMANS.md",
        "license": "MIT",
    }
    for field, value in expected.items():
        if manifest.get(field) != value:
            errors.append(f"{vendor} package metadata mismatch: {field}")
    if vendor == "codex" and manifest.get("author", {}).get("name") != "alsi-lawr":
        errors.append("codex package metadata mismatch: author.name")
    if vendor == "claude" and manifest.get("author", {}).get("name") != "alsi-lawr":
        errors.append("claude package metadata mismatch: author.name")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    arguments = parser.parse_args()
    root = arguments.source.resolve()
    kind = layout(root)
    errors: list[str] = []
    if kind == "unknown":
        errors.append(f"unrecognised Casefile layout: {root}")
    else:
        common_validation(root, kind, errors)
        if kind == "source":
            codex_validation(root / "adapters/codex", errors)
            claude_validation(root / "adapters/claude", errors)
            manifest_validation(root, errors)
        elif kind == "codex-package":
            codex_validation(root / "config", errors)
            package_metadata(root, "codex", errors)
        else:
            claude_validation(root / "config", errors)
            package_metadata(root, "claude", errors)
    if errors:
        print("Casefile validation failed:", *errors, sep="\n- ")
        return 1
    print(f"validated Casefile {kind}: {len(CASEFILE_SKILLS)} workflow skills and {len(REUSABLE_SKILLS)} reusable skills")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
