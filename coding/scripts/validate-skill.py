#!/usr/bin/env python3
"""Validate portable skills, generated packages, matrices, and verification inputs."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath


NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
PHASES = {"planning", "investigation", "review", "implementation", "closeout"}
EVIDENCE = {
    "mechanical",
    "sampled_behavior",
    "comparative",
    "model_judgement",
    "human_judgement",
    "unverified",
}


def frontmatter(path: Path) -> tuple[dict[str, str], str]:
    text = path.read_text(encoding="ascii")
    if not text.startswith("---\n") or "\n---\n" not in text[4:]:
        raise ValueError("missing delimited frontmatter")
    header, body = text[4:].split("\n---\n", 1)
    metadata: dict[str, str] = {}
    for line in header.splitlines():
        if ":" not in line:
            raise ValueError(f"invalid frontmatter line: {line!r}")
        key, value = line.split(":", 1)
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] == '"':
            value = value[1:-1]
        metadata[key.strip()] = value
    return metadata, body.lstrip("\n")


def validate_skill(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        metadata, body = frontmatter(path)
    except (OSError, UnicodeError, ValueError) as error:
        return [f"{path}: {error}"]
    name = metadata.get("name")
    description = metadata.get("description")
    if not isinstance(name, str) or not NAME.fullmatch(name):
        errors.append(f"{path}: invalid skill name")
    elif path.parent.name != name:
        errors.append(f"{path}: name does not match directory")
    if not description or len(description) > 1024:
        errors.append(f"{path}: description must contain 1..1024 characters")
    if not body.startswith("# ") or len(body.strip()) < 40:
        errors.append(f"{path}: body must start with a heading and contain instructions")
    for link in LINK.findall(body):
        target_text = link.split("#", 1)[0]
        if not target_text or "://" in target_text or target_text.startswith(("#", "${")):
            continue
        pure = PurePosixPath(target_text)
        if pure.is_absolute() or ".." in pure.parts:
            errors.append(f"{path}: unsafe local link: {link}")
        elif not (path.parent / Path(*pure.parts)).exists():
            errors.append(f"{path}: broken local link: {link}")
    for current, directories, names in os.walk(path.parent, followlinks=False):
        current_path = Path(current)
        for entry in directories + names:
            candidate = current_path / entry
            if candidate.is_symlink():
                errors.append(f"{path}: bundled symlink is forbidden: {candidate}")
        if current_path.name == "scripts":
            for name_entry in names:
                script = current_path / name_entry
                if script.is_file() and not script.stat().st_mode & 0o111:
                    errors.append(f"{path}: bundled script is not executable: {script}")
    return errors


def validate_matrix(path: Path, profiles: set[str] | None = None) -> list[str]:
    errors: list[str] = []
    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        return [f"{path}: invalid matrix: {error}"]
    for key in ("schema_version", "strategy_id", "phase", "adapter", "orchestrator", "limits", "requirements", "coordination"):
        if key not in document:
            errors.append(f"{path}: missing {key}")
    if document.get("schema_version") != 1 or document.get("phase") not in PHASES:
        errors.append(f"{path}: invalid schema or phase")
    if document.get("orchestrator", {}).get("binding") != "root":
        errors.append(f"{path}: orchestrator must remain root")
    capabilities = document.get("requirements", {}).get("capabilities")
    if not isinstance(capabilities, list) or not all(isinstance(item, str) for item in capabilities):
        errors.append(f"{path}: capabilities must be a string array")
    limits = document.get("limits", {})
    concurrency = limits.get("max_concurrent_subagents")
    depth = limits.get("max_depth")
    if not isinstance(concurrency, int) or concurrency < 1 or not isinstance(depth, int) or depth < 0:
        errors.append(f"{path}: invalid limits")
    minimum = 0
    for worker in document.get("workers", []):
        required = {"role", "platform_profile", "model", "reasoning", "minimum_count", "maximum_count", "can_spawn_subagents"}
        if not isinstance(worker, dict) or required - worker.keys():
            errors.append(f"{path}: incomplete worker")
            continue
        if profiles is not None and worker["platform_profile"] not in profiles:
            errors.append(f"{path}: unknown profile {worker['platform_profile']}")
        if worker["minimum_count"] < 1 or worker["minimum_count"] > worker["maximum_count"]:
            errors.append(f"{path}: invalid worker counts")
        if worker["can_spawn_subagents"] and depth < 2:
            errors.append(f"{path}: spawning worker requires depth 2")
        minimum += worker["minimum_count"]
    if isinstance(concurrency, int) and minimum > concurrency:
        errors.append(f"{path}: worker minima exceed concurrency")
    for key in ("batch_when_capacity_exceeded", "candidate_review_before_ticket", "shared_ticket_storage_required"):
        if not isinstance(document.get("coordination", {}).get(key), bool):
            errors.append(f"{path}: coordination {key} must be boolean")
    pipeline = document.get("coordination", {}).get("pipeline")
    if pipeline is not None:
        required_pipeline = {
            "maximum_active_tickets",
            "look_ahead_read_only",
            "require_dependency_independence",
            "require_disjoint_write_paths",
            "immutable_review_commits",
            "corrections_preempt_forward_work",
        }
        if not isinstance(pipeline, dict) or set(pipeline) != required_pipeline:
            errors.append(f"{path}: invalid pipeline coordination fields")
        else:
            active = pipeline["maximum_active_tickets"]
            if type(active) is not int or active < 2:
                errors.append(f"{path}: pipeline active-ticket limit must be at least two")
            for key in required_pipeline - {"maximum_active_tickets"}:
                if not isinstance(pipeline[key], bool):
                    errors.append(f"{path}: pipeline {key} must be boolean")
        if document.get("phase") != "implementation":
            errors.append(f"{path}: pipeline coordination requires implementation phase")
    return errors


def walk_safe(root: Path) -> list[str]:
    errors: list[str] = []
    if root.is_symlink() or not root.is_dir():
        return [f"unsafe or missing directory: {root}"]
    for current, directories, names in os.walk(root, followlinks=False):
        current_path = Path(current)
        for name in directories + names:
            path = current_path / name
            if path.is_symlink():
                errors.append(f"symlink is forbidden: {path}")
        for name in names:
            path = current_path / name
            if path.name == "models_cache.json":
                errors.append(f"model cache is forbidden: {path}")
            if path.is_file() and path.stat().st_size == 0:
                errors.append(f"empty file: {path}")
    return errors


def validate_package(root: Path) -> list[str]:
    errors = walk_safe(root)
    codex = root / ".codex-plugin/plugin.json"
    claude = root / ".claude-plugin/plugin.json"
    if codex.is_file() == claude.is_file():
        errors.append(f"{root}: package must have exactly one vendor manifest")
        return errors
    manifest_path = codex if codex.is_file() else claude
    try:
        manifest = json.loads(manifest_path.read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        return errors + [f"{manifest_path}: invalid JSON: {error}"]
    if manifest.get("name") != root.name or not SEMVER.fullmatch(str(manifest.get("version", ""))):
        errors.append(f"{manifest_path}: invalid package identity")
    if not manifest.get("description"):
        errors.append(f"{manifest_path}: description is required")
    for field in ("repository", "license"):
        if not isinstance(manifest.get(field), str) or not manifest[field]:
            errors.append(f"{manifest_path}: {field} is required")
    if claude.is_file():
        if manifest.get("author", {}).get("name") != "alsi-lawr":
            errors.append(f"{manifest_path}: author.name must declare the publisher")
        entries = sorted(path.relative_to(root / ".claude-plugin").as_posix() for path in (root / ".claude-plugin").rglob("*") if path.is_file())
        if entries != ["plugin.json"]:
            errors.append("only plugin.json may be beneath .claude-plugin")
        if not (root / "skills").is_dir():
            errors.append("Claude skills must be package-root components")
    else:
        if manifest.get("author", {}).get("name") != "alsi-lawr":
            errors.append(f"{manifest_path}: author.name must declare the publisher")
        interface = manifest.get("interface", {})
        if interface.get("displayName") != manifest.get("name"):
            errors.append(f"{manifest_path}: interface.displayName must declare the plugin")
        if not interface.get("shortDescription"):
            errors.append(f"{manifest_path}: interface.shortDescription is required")
    if manifest.get("name") == "humans-md" and not (root / "templates/AGENTS.md").is_file():
        errors.append("core packaged AGENTS.md contract template is missing")
    excluded = {"build-code", "test-benchmark-code"}
    present = {path.parent.name for path in (root / "skills").glob("*/SKILL.md")}
    if excluded & present:
        errors.append(f"excluded code skills are packaged: {sorted(excluded & present)}")
    for skill in sorted((root / "skills").glob("*/SKILL.md")):
        errors += validate_skill(skill)
    profiles = {path.stem for path in (root / "agents").glob("*.toml")} | {path.stem for path in (root / "agents").glob("*.md")}
    for matrix in sorted((root / "matrices").glob("*.toml")):
        errors += validate_matrix(matrix, profiles)
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--all", action="store_true", help="validate source skills, matrices, and verification")
    parser.add_argument("--skill", type=Path, action="append", default=[])
    parser.add_argument("--package", type=Path, action="append", default=[])
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    errors: list[str] = []
    package_layout = (root / ".codex-plugin/plugin.json").is_file() or (
        root / ".claude-plugin/plugin.json"
    ).is_file()
    skills = list(arguments.skill)
    if arguments.all:
        skills += sorted((root / "skills").glob("*/SKILL.md"))
    for skill in skills:
        path = skill if skill.is_absolute() else root / skill
        errors += validate_skill(path)
    if arguments.all:
        if package_layout:
            profiles = {path.stem for path in (root / "agents").glob("*.toml")} | {
                path.stem for path in (root / "agents").glob("*.md")
            }
            for matrix in sorted((root / "matrices").glob("*.toml")):
                errors += validate_matrix(matrix, profiles)
            errors += validate_package(root)
        else:
            for adapter in ("codex", "claude"):
                profile_dir = root / "adapters" / adapter / "agents"
                profiles = {path.stem for path in profile_dir.glob("*.toml")} | {
                    path.stem for path in profile_dir.glob("*.md")
                }
                for matrix in sorted((root / "adapters" / adapter / "matrices").glob("*.toml")):
                    errors += validate_matrix(matrix, profiles)
        verify = root / "scripts/verify-skill.py"
        for suite in sorted((root / "verification/suites").glob("*.toml")):
            for strategy in sorted((root / "verification/strategies").glob("*.toml")):
                process = subprocess.run(
                    [sys.executable, str(verify), "validate", "--strategy", str(strategy), "--suite", str(suite)],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                if process.returncode:
                    errors.append(process.stdout.strip() or process.stderr.strip())
    for package in arguments.package:
        path = package if package.is_absolute() else root / package
        errors += validate_package(path)
    if errors:
        print("skill validation failed:", *errors, sep="\n- ")
        return 1
    count = len(skills)
    package_count = len(arguments.package) + (1 if arguments.all and package_layout else 0)
    print(f"validated {count} skill(s) and {package_count} package(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
