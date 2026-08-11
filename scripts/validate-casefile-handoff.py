#!/usr/bin/env python3
"""Validate the reviewed Casefile binary workflow and retained release handoff."""
from __future__ import annotations

import argparse
import importlib.util
import json
import re
from pathlib import Path

try:
    from casefile_artifacts import TARGETS, ArtifactError, load
except ModuleNotFoundError:
    _artifact_path = Path(__file__).resolve().with_name("casefile_artifacts.py")
    _artifact_spec = importlib.util.spec_from_file_location("casefile_artifacts", _artifact_path)
    if _artifact_spec is None or _artifact_spec.loader is None:
        raise
    _artifact_module = importlib.util.module_from_spec(_artifact_spec)
    _artifact_spec.loader.exec_module(_artifact_module)
    TARGETS = _artifact_module.TARGETS
    ArtifactError = _artifact_module.ArtifactError
    load = _artifact_module.load


WORKFLOW_NAME = "Build Casefile executable matrix"
WORKFLOW_PATH = ".github/workflows/build-casefile-binaries.yml"
BUILD_BRANCH = re.compile(r"^casefile/build-[^/]*$")


class HandoffError(ValueError):
    pass


def read_json(path: Path, label: str) -> dict:
    try:
        if path.is_symlink() or not path.is_file() or path.stat().st_size <= 0:
            raise HandoffError(f"missing, empty, or unsafe {label}")
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise HandoffError(f"invalid {label}: {error}") from error
    if not isinstance(value, dict):
        raise HandoffError(f"invalid {label} object")
    return value


def validate_run(path: Path, run_id: str, source: str) -> dict:
    run = read_json(path, "workflow run metadata")
    expected = {
        "id": int(run_id),
        "name": WORKFLOW_NAME,
        "path": WORKFLOW_PATH,
        "status": "completed",
        "conclusion": "success",
        "head_sha": source,
    }
    mismatches = [f"{key}={run.get(key)!r}" for key, value in expected.items() if run.get(key) != value]
    if mismatches:
        raise HandoffError("workflow run is not the reviewed binary build: " + ", ".join(mismatches))
    event = run.get("event")
    if event == "workflow_dispatch":
        return run
    if event == "push" and isinstance(run.get("head_branch"), str) and BUILD_BRANCH.fullmatch(run["head_branch"]):
        return run
    raise HandoffError(
        "workflow run is not an allowed Casefile binary event: "
        f"event={event!r}, head_branch={run.get('head_branch')!r}"
    )


def validate_provenance(path: Path, run: dict, source: str) -> None:
    provenance = read_json(path, "retained build provenance")
    head_branch = run.get("head_branch") if run["event"] == "push" else None
    expected = {
        "event": run["event"],
        "head_branch": head_branch,
        "run_id": run["id"],
        "schema_version": 1,
        "source_commit": source,
        "workflow_name": WORKFLOW_NAME,
        "workflow_path": WORKFLOW_PATH,
    }
    if provenance != expected:
        raise HandoffError("retained build provenance differs from the reviewed workflow run")


def validate_handoff(
    root: Path,
    run_metadata: Path,
    run_id: str,
    version: str,
    source: str,
) -> None:
    root = root.resolve(strict=True)
    run = validate_run(run_metadata, run_id, source)
    validate_provenance(root / "casefile-build-provenance.json", run, source)
    runtime = root / "casefile-runtime"
    load(runtime, version, source)

    smoke_root = root / "native-smoke"
    expected_smoke = {f"{target}.json" for target in TARGETS}
    actual_smoke = {path.name for path in smoke_root.iterdir()} if smoke_root.is_dir() else set()
    if not expected_smoke.issubset(actual_smoke):
        raise HandoffError("retained native smoke inventory is incomplete")
    for target in TARGETS:
        smoke = read_json(smoke_root / f"{target}.json", f"native smoke for {target}")
        if any(
            smoke.get(key) != value
            for key, value in {"identity": "casefile", "tools": 12, "version": version}.items()
        ):
            raise HandoffError(f"native smoke result is invalid for {target}")

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handoff-root", type=Path, required=True)
    parser.add_argument("--run-metadata", type=Path, required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-commit", required=True)
    args = parser.parse_args()
    try:
        validate_handoff(
            args.handoff_root,
            args.run_metadata,
            args.run_id,
            args.version,
            args.source_commit,
        )
        print("validated reviewed Casefile binary workflow handoff")
        return 0
    except (OSError, ValueError, ArtifactError, HandoffError) as error:
        print(f"Casefile handoff validation failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
