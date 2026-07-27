#!/usr/bin/env python3
"""Assemble and verify the fixed Casefile 0.4.0 executable matrix."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
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
HASH = re.compile(r"^[0-9a-f]{64}$")


class ArtifactError(ValueError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def executable_name(target: str) -> str:
    return "casefile.exe" if target.endswith("windows-msvc") else "casefile"


def relative_path(target: str) -> str:
    return f"bin/{target}/{executable_name(target)}"


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def validate_format(path: Path, target: str) -> None:
    data = path.read_bytes()
    if target.endswith("linux-musl"):
        if len(data) < 20 or data[:4] != b"\x7fELF" or data[5] != 1:
            raise ArtifactError(f"wrong executable format for {target}: expected little-endian ELF")
        expected = 183 if target.startswith("aarch64") else 62
        if struct.unpack_from("<H", data, 18)[0] != expected:
            raise ArtifactError(f"wrong executable architecture for {target}")
    elif target.endswith("apple-darwin"):
        if len(data) < 8 or data[:4] not in {b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe"}:
            raise ArtifactError(f"wrong executable format for {target}: expected 64-bit Mach-O")
        endian = ">" if data[:4] == b"\xfe\xed\xfa\xcf" else "<"
        expected = 0x0100000C if target.startswith("aarch64") else 0x01000007
        if struct.unpack_from(f"{endian}I", data, 4)[0] != expected:
            raise ArtifactError(f"wrong executable architecture for {target}")
    else:
        if len(data) < 64 or data[:2] != b"MZ":
            raise ArtifactError(f"wrong executable format for {target}: expected PE")
        offset = struct.unpack_from("<I", data, 0x3C)[0]
        expected = 0xAA64 if target.startswith("aarch64") else 0x8664
        if len(data) < offset + 6 or data[offset:offset + 4] != b"PE\0\0" or struct.unpack_from("<H", data, offset + 4)[0] != expected:
            raise ArtifactError(f"wrong executable architecture for {target}")


def load(root: Path, expected_version: str | None = None, expected_source: str | None = None) -> dict:
    root = root.expanduser().resolve(strict=True)
    manifest_path = root / "artifacts.json"
    try:
        raw = manifest_path.read_bytes()
        raw.decode("ascii")
        document = json.loads(raw)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ArtifactError(f"invalid artifact manifest: {error}") from error
    if canonical(document) != raw:
        raise ArtifactError("artifact manifest is not canonical ASCII JSON")
    if not isinstance(document, dict) or set(document) != {
        "schema_version", "version", "source_commit", "artifacts"
    }:
        raise ArtifactError("artifact manifest has unsupported keys")
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
        if not isinstance(row, dict) or set(row) != {"path", "sha256", "size", "target"}:
            raise ArtifactError("artifact entry has unsupported keys")
        target = row["target"]
        if target not in TARGETS or target in seen:
            raise ArtifactError(f"missing, duplicate, or unsupported target: {target!r}")
        seen.add(target)
        expected_path = relative_path(target)
        if row["path"] != expected_path:
            raise ArtifactError(f"unexpected artifact path for {target}")
        pure = PurePosixPath(row["path"])
        if pure.is_absolute() or ".." in pure.parts or "\\" in row["path"]:
            raise ArtifactError(f"unsafe artifact path for {target}")
        path = root / Path(*pure.parts)
        if path.is_symlink() or not path.is_file():
            raise ArtifactError(f"missing or unsafe artifact for {target}")
        size = path.stat().st_size
        if not isinstance(row["size"], int) or isinstance(row["size"], bool) or row["size"] <= 0:
            raise ArtifactError(f"invalid artifact size for {target}")
        if size != row["size"]:
            raise ArtifactError(f"artifact size mismatch for {target}")
        if not isinstance(row["sha256"], str) or not HASH.fullmatch(row["sha256"]):
            raise ArtifactError(f"invalid artifact hash for {target}")
        if digest(path) != row["sha256"]:
            raise ArtifactError(f"artifact hash mismatch for {target}")
        validate_format(path, target)
    if seen != set(TARGETS) or [row["target"] for row in rows] != list(TARGETS):
        raise ArtifactError("artifact entries must contain the sorted complete matrix")
    expected_files = {Path("artifacts.json"), *(Path(relative_path(target)) for target in TARGETS)}
    actual_files = {
        path.relative_to(root)
        for path in root.rglob("*")
        if path.is_file() or path.is_symlink()
    }
    if actual_files != expected_files:
        extras = sorted(path.as_posix() for path in actual_files - expected_files)
        missing = sorted(path.as_posix() for path in expected_files - actual_files)
        raise ArtifactError(f"artifact root inventory differs; missing={missing}; extra={extras}")
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
            validate_format(source_path, target)
            destination = staging / relative_path(target)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_path, destination)
            if os.name == "posix" and not target.endswith("windows-msvc"):
                destination.chmod(0o755)
            rows.append({
                "path": relative_path(target),
                "sha256": digest(destination),
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
