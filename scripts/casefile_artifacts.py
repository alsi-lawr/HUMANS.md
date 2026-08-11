#!/usr/bin/env python3
"""Assemble and verify the fixed Casefile executable matrix."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import tempfile
from pathlib import Path, PurePosixPath


SCHEMA_VERSION = 1
TARGETS = (
    "aarch64-apple-darwin",
    "aarch64-pc-windows-msvc",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-musl",
)
SHA = re.compile(r"^[0-9a-f]{40}$")
SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


class ArtifactError(ValueError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def executable_name(target: str) -> str:
    return "casefile.exe" if target.endswith("windows-msvc") else "casefile"


def relative_path(target: str) -> str:
    return f"bin/{target}/{executable_name(target)}"


def normalized_artifact_path(value: object, target: str) -> Path:
    if not isinstance(value, str) or not value or "\0" in value:
        raise ArtifactError(f"invalid artifact path for {target}")
    if value.startswith(("/", "\\")) or re.match(r"^[A-Za-z]:", value):
        raise ArtifactError(f"unsafe artifact path for {target}")
    parts = [part for part in value.replace("\\", "/").split("/") if part]
    if not parts or any(part in {".", ".."} for part in parts):
        raise ArtifactError(f"unsafe artifact path for {target}")
    normalized = PurePosixPath(*parts).as_posix()
    if normalized != relative_path(target):
        raise ArtifactError(f"unexpected artifact path for {target}")
    return Path(*parts)


def landed(root: Path, relative: Path, target: str) -> Path:
    candidate = root / relative
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(root)
    except (OSError, ValueError) as error:
        raise ArtifactError(f"missing or unsafe artifact for {target}") from error
    if candidate.is_symlink() or not candidate.is_file() or candidate.stat().st_size <= 0:
        raise ArtifactError(f"missing, empty, or unsafe artifact for {target}")
    return candidate


def load(root: Path, expected_version: str | None = None, expected_source: str | None = None) -> dict:
    root = root.expanduser().resolve(strict=True)
    manifest_path = root / "artifacts.json"
    try:
        if manifest_path.is_symlink() or not manifest_path.is_file() or manifest_path.stat().st_size <= 0:
            raise ArtifactError("artifact manifest is missing, empty, or unsafe")
        document = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ArtifactError(f"invalid artifact manifest: {error}") from error
    if not isinstance(document, dict) or not {
        "schema_version", "version", "source_commit", "artifacts"
    }.issubset(document):
        raise ArtifactError("artifact manifest is incomplete")
    version = document["version"]
    source = document["source_commit"]
    rows = document["artifacts"]
    if document["schema_version"] != SCHEMA_VERSION:
        raise ArtifactError("unsupported artifact manifest schema")
    if not isinstance(version, str) or not SEMVER.fullmatch(version):
        raise ArtifactError("invalid artifact version")
    if expected_version is not None and version != expected_version:
        raise ArtifactError(f"artifact version {version!r} does not match {expected_version!r}")
    if not isinstance(source, str) or not SHA.fullmatch(source):
        raise ArtifactError("invalid artifact source commit")
    if expected_source is not None and source != expected_source:
        raise ArtifactError(f"artifact source {source!r} does not match {expected_source!r}")
    if not isinstance(rows, list) or len(rows) != len(TARGETS):
        raise ArtifactError("artifact matrix is incomplete")
    seen: set[str] = set()
    for row in rows:
        if not isinstance(row, dict) or not {"path", "target"}.issubset(row):
            raise ArtifactError("artifact entry is incomplete")
        target = row["target"]
        if target not in TARGETS or target in seen:
            raise ArtifactError(f"missing, duplicate, or unsupported target: {target!r}")
        seen.add(target)
        landed(root, normalized_artifact_path(row["path"], target), target)
    if seen != set(TARGETS):
        raise ArtifactError("artifact entries must contain the complete matrix")
    return document


def assemble(output: Path, inputs: Path, version: str, source: str) -> dict:
    if not SEMVER.fullmatch(version):
        raise ArtifactError("invalid artifact version")
    if not SHA.fullmatch(source):
        raise ArtifactError("invalid artifact source commit")
    inputs = inputs.resolve(strict=True)
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{output.name}.assemble-", dir=output.parent))
    try:
        rows = []
        for target in TARGETS:
            source_path = inputs / target / executable_name(target)
            if source_path.is_symlink() or not source_path.is_file() or source_path.stat().st_size <= 0:
                raise ArtifactError(f"missing or unsafe build input for {target}")
            destination = staging / relative_path(target)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_path, destination)
            if os.name == "posix" and not target.endswith("windows-msvc"):
                destination.chmod(0o755)
            rows.append({
                "path": relative_path(target),
                "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
                "size": destination.stat().st_size,
                "target": target,
            })
        document = {
            "schema_version": SCHEMA_VERSION,
            "version": version,
            "source_commit": source,
            "artifacts": rows,
        }
        (staging / "artifacts.json").write_bytes(canonical(document))
        load(staging, version, source)
        if output.exists():
            raise ArtifactError(f"refusing to replace artifact root: {output}")
        os.replace(staging, output)
        return document
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    verify = commands.add_parser("verify")
    verify.add_argument("--artifact-root", type=Path, required=True)
    verify.add_argument("--version")
    verify.add_argument("--source-commit")
    build = commands.add_parser("assemble")
    build.add_argument("--input-root", type=Path, required=True)
    build.add_argument("--artifact-root", type=Path, required=True)
    build.add_argument("--version", required=True)
    build.add_argument("--source-commit", required=True)
    args = parser.parse_args()
    try:
        if args.command == "verify":
            document = load(args.artifact_root, args.version, args.source_commit)
        else:
            document = assemble(args.artifact_root, args.input_root, args.version, args.source_commit)
        print(json.dumps(document, indent=2, sort_keys=True))
        return 0
    except (OSError, UnicodeError, ArtifactError, json.JSONDecodeError) as error:
        print(f"artifact {args.command} failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
