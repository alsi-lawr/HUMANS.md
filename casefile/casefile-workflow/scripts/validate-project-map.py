#!/usr/bin/env python3
"""Validate a planning store's project map and namespace coverage."""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path


def validate(planning_store: Path) -> list[str]:
    errors: list[str] = []
    project_map = planning_store / "projects.toml"

    if not planning_store.is_dir():
        return [f"planning store is not a directory: {planning_store}"]
    if not project_map.is_file():
        return [f"missing project map: {project_map}"]

    try:
        document = tomllib.loads(project_map.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"invalid project map {project_map}: {error}"]

    projects = document.get("projects")
    if not isinstance(projects, dict):
        return [f"project map must contain a [projects] table: {project_map}"]

    for name, directory in projects.items():
        if not isinstance(name, str) or not name.strip():
            errors.append("project names must be non-empty strings")
            continue
        if name in {".", ".."} or "/" in name or "\\" in name:
            errors.append(f"project name must be one namespace segment: {name!r}")
        if not isinstance(directory, str) or not directory.strip():
            errors.append(f"project directory must be a non-empty string: {name!r}")
            continue
        source = Path(directory)
        if not source.is_absolute():
            errors.append(f"project directory must be absolute: {name!r} -> {directory!r}")
        elif not source.is_dir():
            errors.append(f"project directory does not exist: {name!r} -> {directory!r}")

    namespaces = planning_store / "projects"
    if namespaces.exists() and not namespaces.is_dir():
        errors.append(f"project namespaces path is not a directory: {namespaces}")
    elif namespaces.is_dir():
        for namespace in sorted(namespaces.iterdir()):
            if namespace.is_dir() and namespace.name not in projects:
                errors.append(f"project namespace has no mapping: {namespace.name!r}")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate projects.toml and its coverage of project namespaces."
    )
    parser.add_argument("--planning-store", type=Path, required=True)
    arguments = parser.parse_args()
    planning_store = arguments.planning_store.expanduser().resolve()
    errors = validate(planning_store)

    if errors:
        print("project map validation failed:", *errors, sep="\n- ", file=sys.stderr)
        return 1

    projects = tomllib.loads(
        (planning_store / "projects.toml").read_text(encoding="utf-8")
    )["projects"]
    namespaces = planning_store / "projects"
    namespace_count = (
        sum(1 for path in namespaces.iterdir() if path.is_dir())
        if namespaces.is_dir()
        else 0
    )
    print(
        f"validated {len(projects)} project mapping(s) and "
        f"{namespace_count} project namespace(s) in {planning_store}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
