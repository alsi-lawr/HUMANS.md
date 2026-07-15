#!/usr/bin/env python3
"""Preview or atomically install one behaviour contract file."""
from __future__ import annotations

import argparse
import difflib
import hashlib
import os
import tempfile
from pathlib import Path


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def atomic_write(path: Path, data: bytes, mode: int = 0o644) -> None:
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_path, mode)
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def preview(source: Path, destination: Path) -> tuple[bytes, bytes | None, str]:
    source_bytes = source.read_bytes()
    if not source_bytes:
        raise ValueError(f"source contract is empty: {source}")
    source_bytes.decode("ascii")
    destination_bytes = destination.read_bytes() if destination.exists() else None
    old = [] if destination_bytes is None else destination_bytes.decode("ascii").splitlines(True)
    new = source_bytes.decode("ascii").splitlines(True)
    diff = "".join(
        difflib.unified_diff(old, new, fromfile=str(destination), tofile=str(source))
    )
    return source_bytes, destination_bytes, diff


def install(source: Path, destination: Path, replace: bool) -> str:
    source_bytes, destination_bytes, _ = preview(source, destination)
    if destination_bytes == source_bytes:
        return "unchanged"
    if destination_bytes is not None and not replace:
        raise ValueError("destination differs; rerun with --replace after reviewing the diff")
    if not destination.parent.is_dir():
        raise ValueError(f"destination directory does not exist: {destination.parent}")

    if destination_bytes is not None:
        backup = destination.with_name(f"{destination.name}.backup-{digest(destination_bytes)}")
        if backup.exists() and backup.read_bytes() != destination_bytes:
            raise ValueError(f"conflicting backup: {backup}")
        if not backup.exists():
            atomic_write(backup, destination_bytes, 0o600)

    atomic_write(destination, source_bytes)
    if destination.read_bytes() != source_bytes:
        raise RuntimeError(f"post-write verification failed: {destination}")
    return "installed"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--replace", action="store_true")
    arguments = parser.parse_args()

    source = arguments.source.resolve(strict=True)
    destination = arguments.destination.expanduser().absolute()
    source_bytes, destination_bytes, diff = preview(source, destination)
    print(f"destination: {destination}")
    print(f"source_sha256: {digest(source_bytes)}")
    print(f"destination_sha256: {digest(destination_bytes) if destination_bytes is not None else 'missing'}")
    print(diff or "no changes")
    if not arguments.apply:
        print("preview only; no files changed")
        return 0
    print(install(source, destination, arguments.replace))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
