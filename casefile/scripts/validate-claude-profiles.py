#!/usr/bin/env python3
"""Check that Claude matrices, profiles, and agent files resolve to each other."""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

WRITER_STRATEGIES = {
    "casefile-implement-ticket-batch",
    "casefile-implement-ticket-batch-look-ahead",
    "casefile-implement-pipeline",
}


def frontmatter(path: Path) -> dict[str, str]:
    lines = path.read_text(encoding="ascii").splitlines()
    if not lines or lines[0] != "---" or "---" not in lines[1:]:
        raise ValueError(f"{path}: no frontmatter")
    fields = {}
    for line in lines[1 : lines[1:].index("---") + 1]:
        if ":" in line and not line.startswith((" ", "\t")):
            key, value = line.split(":", 1)
            fields[key.strip()] = value.strip()
    return fields


def check(adapter: Path) -> list[str]:
    profiles = tomllib.loads((adapter / "profiles.toml").read_text(encoding="ascii"))
    workers = {row["role"]: row for row in profiles.get("workers", [])}
    writers = profiles.get("writer_profiles", [])
    variants = profiles.get("worker_profiles", [])
    agents = {path.stem: path for path in (adapter / "agents").glob("*.md")}
    bound = set()
    failures = []

    for row in list(workers.values()) + writers + variants:
        target = adapter / row["agent_file"]
        if not target.is_file():
            failures.append(f"{row['profile']}: agent_file does not exist: {row['agent_file']}")
            continue
        bound.add(target.stem)
        fields = frontmatter(target)
        if fields.get("name") != row["profile"]:
            failures.append(f"{row['profile']}: agent name is {fields.get('name')!r}")
        if fields.get("model") != row["model"]:
            failures.append(
                f"{row['profile']}: model {fields.get('model')!r} != profile {row['model']!r}"
            )

    for name in sorted(set(agents) - bound):
        failures.append(f"{name}: agent file has no profiles.toml row")

    # Every worker a matrix declares must have a role binding.
    declared: dict[str, set[str]] = {}
    for path in sorted((adapter / "matrices").glob("*.toml")):
        matrix = tomllib.loads(path.read_text(encoding="ascii"))
        for worker in matrix.get("workers", []):
            if worker["role"] not in workers:
                failures.append(f"{path.name}: role {worker['role']!r} has no workers row")
            declared.setdefault(worker["role"], set()).add(matrix["strategy_id"])

    # A writer binding must reach every implementation strategy.
    for row in writers:
        if set(row.get("strategy_ids", [])) != WRITER_STRATEGIES:
            failures.append(f"{row['profile']}: strategy_ids do not cover the writer strategies")

    # A worker variant must cover exactly the strategies whose matrices declare its role.
    for row in variants:
        if row["role"] not in workers:
            failures.append(f"{row['profile']}: variant role {row['role']!r} has no workers row")
        if set(row.get("strategy_ids", [])) != declared.get(row["role"], set()):
            failures.append(f"{row['profile']}: strategy_ids do not cover the role's strategies")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--adapter",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "adapters/claude",
    )
    failures = check(parser.parse_args().adapter)
    for failure in failures:
        print(f"claude profile binding failed: {failure}")
    if not failures:
        print("claude profile bindings resolve")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
