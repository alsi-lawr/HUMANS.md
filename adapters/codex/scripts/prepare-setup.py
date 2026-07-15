#!/usr/bin/env python3
"""Render opt-in Codex configuration candidates without editing active state."""
from __future__ import annotations

import argparse
import hashlib
import os
import tempfile
import tomllib
from pathlib import Path


TOKEN = "__HUMANS_MD_PLUGIN_ROOT__"


def atomic_create_or_match(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        if path.read_bytes() != data:
            raise ValueError(f"refusing to replace different candidate: {path}")
        return
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_path, 0o600)
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def render(plugin_root: Path) -> dict[str, bytes]:
    required = [
        plugin_root / "config" / "config-fragment.toml.in",
        plugin_root / "config" / "root.config.toml",
        plugin_root / "config" / "profiles.toml",
    ]
    for path in required:
        if not path.is_file():
            raise ValueError(f"missing packaged setup input: {path}")
    root = str(plugin_root).replace("\\", "/")
    fragment = required[0].read_text(encoding="ascii")
    profiles = tomllib.loads(required[2].read_text(encoding="ascii"))
    expected_paths = len(profiles.get("matrix_profiles", []))
    if not expected_paths or fragment.count(TOKEN) != expected_paths:
        raise ValueError("configuration template has an unexpected path-token count")
    return {
        "config-fragment.toml": fragment.replace(TOKEN, root).encode("ascii"),
        "root.config.toml": required[1].read_bytes(),
        "profiles.toml": required[2].read_bytes(),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugin-root", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()
    plugin_root = arguments.plugin_root.resolve(strict=True)
    output = arguments.output_dir.resolve()
    candidates = render(plugin_root)
    for name, data in candidates.items():
        print(f"{output / name} sha256={hashlib.sha256(data).hexdigest()}")
    if not arguments.apply:
        print("preview only; active configuration unchanged")
        return 0
    for name, data in candidates.items():
        atomic_create_or_match(output / name, data)
    print("candidate configuration written; active configuration unchanged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
