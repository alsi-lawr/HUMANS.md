#!/usr/bin/env python3
"""Validate the reviewed Casefile binary workflow and retained release handoff."""
from __future__ import annotations

import argparse
import importlib.util
import json
import re
from pathlib import Path

try:
    from casefile_artifacts import TARGETS, ArtifactError, digest, load
except ModuleNotFoundError:
    _artifact_path = Path(__file__).resolve().with_name("casefile_artifacts.py")
    _artifact_spec = importlib.util.spec_from_file_location("casefile_artifacts", _artifact_path)
    if _artifact_spec is None or _artifact_spec.loader is None:
        raise
    _artifact_module = importlib.util.module_from_spec(_artifact_spec)
    _artifact_spec.loader.exec_module(_artifact_module)
    TARGETS = _artifact_module.TARGETS
    ArtifactError = _artifact_module.ArtifactError
    digest = _artifact_module.digest
    load = _artifact_module.load


WORKFLOW_NAME = "Build Casefile executable matrix"
WORKFLOW_PATH = ".github/workflows/build-casefile-binaries.yml"
INVENTORY_LINE = re.compile(r"^([0-9a-f]{64})  (.+)$")


class HandoffError(ValueError):
    pass


def read_json(path: Path, label: str) -> dict:
    try:
        value = json.loads(path.read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise HandoffError(f"invalid {label}: {error}") from error
    if not isinstance(value, dict):
        raise HandoffError(f"invalid {label} object")
    return value


def validate_run(path: Path, run_id: str, source: str) -> None:
    run = read_json(path, "workflow run metadata")
    expected = {
        "id": int(run_id),
        "name": WORKFLOW_NAME,
        "path": WORKFLOW_PATH,
        "event": "workflow_dispatch",
        "status": "completed",
        "conclusion": "success",
        "head_sha": source,
    }
    mismatches = [f"{key}={run.get(key)!r}" for key, value in expected.items() if run.get(key) != value]
    if mismatches:
        raise HandoffError("workflow run is not the reviewed binary build: " + ", ".join(mismatches))


def read_inventory(path: Path) -> dict[str, str]:
    try:
        lines = path.read_text(encoding="ascii").splitlines()
    except (OSError, UnicodeError) as error:
        raise HandoffError(f"invalid retained package inventory {path.name}: {error}") from error
    if not lines:
        raise HandoffError(f"retained package inventory is empty: {path.name}")
    inventory: dict[str, str] = {}
    for line in lines:
        match = INVENTORY_LINE.fullmatch(line)
        if match is None:
            raise HandoffError(f"invalid retained package inventory line: {line!r}")
        checksum, source_path = match.groups()
        marker = "/casefile/"
        if marker not in source_path:
            raise HandoffError(f"inventory path is outside the Casefile package: {source_path}")
        relative = source_path.split(marker, 1)[1]
        if relative in inventory:
            raise HandoffError(f"duplicate inventory path: {relative}")
        inventory[relative] = checksum
    return inventory


def validate_handoff(
    root: Path,
    run_metadata: Path,
    run_id: str,
    version: str,
    source: str,
    manifest_sha256: str,
) -> None:
    root = root.resolve(strict=True)
    validate_run(run_metadata, run_id, source)
    runtime = root / "casefile-runtime"
    manifest = load(runtime, version, source)
    actual_manifest_hash = digest(runtime / "artifacts.json")
    if actual_manifest_hash != manifest_sha256:
        raise HandoffError("reviewed artifact manifest digest differs from the publication input")
    checksum_file = root / "casefile-runtime-manifest.sha256"
    expected_checksum_line = f"{manifest_sha256}  casefile-runtime/artifacts.json"
    try:
        checksum_line = checksum_file.read_text(encoding="ascii").strip()
    except (OSError, UnicodeError) as error:
        raise HandoffError(f"invalid retained manifest checksum: {error}") from error
    if checksum_line != expected_checksum_line:
        raise HandoffError("retained manifest checksum does not match the reviewed digest")

    smoke_root = root / "native-smoke"
    expected_smoke = {f"{target}.json" for target in TARGETS}
    actual_smoke = {path.name for path in smoke_root.iterdir()} if smoke_root.is_dir() else set()
    if actual_smoke != expected_smoke:
        raise HandoffError("retained native smoke inventory is incomplete or contains extra files")
    for target in TARGETS:
        smoke = read_json(smoke_root / f"{target}.json", f"native smoke for {target}")
        if smoke != {"identity": "casefile", "tools": 12, "version": version}:
            raise HandoffError(f"native smoke result is invalid for {target}")

    inventories = [
        read_inventory(root / "codex-casefile-inventory.sha256"),
        read_inventory(root / "claude-casefile-inventory.sha256"),
    ]
    expected_runtime = {"runtime/artifacts.json": manifest_sha256}
    expected_runtime.update(
        {f"runtime/{row['path']}": row["sha256"] for row in manifest["artifacts"]}
    )
    for inventory in inventories:
        retained_runtime = {
            relative: checksum for relative, checksum in inventory.items() if relative.startswith("runtime/")
        }
        if retained_runtime != expected_runtime:
            raise HandoffError("retained package inventory does not prove the reviewed runtime bytes")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handoff-root", type=Path, required=True)
    parser.add_argument("--run-metadata", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--manifest-sha256", required=True)
    args = parser.parse_args()
    try:
        validate_handoff(
            args.handoff_root,
            args.run_metadata,
            args.run_id,
            args.version,
            args.source_commit,
            args.manifest_sha256,
        )
        print("validated reviewed Casefile binary workflow handoff")
        return 0
    except (OSError, ValueError, ArtifactError, HandoffError) as error:
        print(f"Casefile handoff validation failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
