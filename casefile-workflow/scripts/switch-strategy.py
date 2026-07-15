#!/usr/bin/env python3
"""Validate and record a Casefile strategy transition."""
from __future__ import annotations

import argparse
import hashlib
import os
import re
import tempfile
import tomllib
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath


SAFE_ID = re.compile(r"^[a-z0-9][a-z0-9-]*$")


def load_toml(path: Path) -> tuple[dict, bytes]:
    data = path.read_bytes()
    if not data:
        raise ValueError(f"empty TOML: {path}")
    return tomllib.loads(data.decode("utf-8")), data


def overlaps(left: str, right: str) -> bool:
    a, b = PurePosixPath(left), PurePosixPath(right)
    return a == b or a in b.parents or b in a.parents


def validate(state: dict, matrix: dict, capabilities: set[str]) -> list[str]:
    errors: list[str] = []
    if state.get("schema_version") != 1:
        errors.append("state schema_version must be 1")
    if matrix.get("schema_version") != 1:
        errors.append("matrix schema_version must be 1")
    strategy_id = matrix.get("strategy_id")
    if not isinstance(strategy_id, str) or not SAFE_ID.fullmatch(strategy_id):
        errors.append("matrix strategy_id is invalid")
    phase = state.get("phase")
    if matrix.get("phase") != phase:
        errors.append("matrix phase does not match current phase")
    if state.get("root", {}).get("binding") != "root":
        errors.append("current root binding is not root")
    if matrix.get("orchestrator", {}).get("binding") != "root":
        errors.append("selected matrix would change the root")

    required = matrix.get("requirements", {}).get("capabilities", [])
    if not isinstance(required, list) or not all(isinstance(item, str) for item in required):
        errors.append("matrix capabilities must be a string array")
    else:
        missing = sorted(set(required) - capabilities)
        if missing:
            errors.append(f"unavailable capabilities: {', '.join(missing)}")

    work_paths = state.get("work", {}).get("paths", [])
    if not isinstance(work_paths, list) or not all(
        isinstance(item, str) and item.strip() for item in work_paths
    ):
        errors.append("state work paths must be non-empty strings")

    claims: list[tuple[str, str]] = []
    for ownership in state.get("ownership", []):
        if not isinstance(ownership, dict) or not ownership.get("active", False):
            continue
        owner = ownership.get("owner")
        paths = ownership.get("paths")
        if not isinstance(owner, str) or not isinstance(paths, list):
            errors.append("active ownership entries require owner and paths")
            continue
        for path in paths:
            if not isinstance(path, str) or not path or path.startswith("/") or ".." in PurePosixPath(path).parts:
                errors.append(f"unsafe ownership path: {path!r}")
            else:
                claims.append((owner, path))
    for index, (owner, path) in enumerate(claims):
        for other_owner, other_path in claims[index + 1 :]:
            if owner != other_owner and overlaps(path, other_path):
                errors.append(
                    f"overlapping active writers: {owner}:{path} and {other_owner}:{other_path}"
                )
    return errors


def quote(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def render_record(
    state: dict,
    matrix: dict,
    matrix_path: Path,
    matrix_bytes: bytes,
    mode: str,
    timestamp: str,
    capabilities: set[str],
    rationale: str,
) -> bytes:
    work = state.get("work", {}).get("paths", [])
    lines = [
        "schema_version = 1",
        f"timestamp = {quote(timestamp)}",
        f"phase = {quote(state['phase'])}",
        f"mode = {quote(mode)}",
        f"previous_strategy_id = {quote(state['strategy_id'])}",
        f"selected_strategy_id = {quote(matrix['strategy_id'])}",
        f"selected_matrix = {quote(str(matrix_path))}",
        f"selected_matrix_sha256 = {quote(hashlib.sha256(matrix_bytes).hexdigest())}",
        'root_binding = "root"',
        f"governed_state_updated = {'true' if mode == 'governed' else 'false'}",
        f"rationale = {quote(rationale)}",
        "available_capabilities = [" + ", ".join(quote(item) for item in sorted(capabilities)) + "]",
        "preserved_work_paths = [" + ", ".join(quote(item) for item in work) + "]",
    ]
    for item in state.get("ownership", []):
        if not isinstance(item, dict) or not item.get("active", False):
            continue
        lines.extend(
            [
                "",
                "[[active_ownership]]",
                f"owner = {quote(item['owner'])}",
                "paths = [" + ", ".join(quote(path) for path in item["paths"]) + "]",
            ]
        )
    return ("\n".join(lines) + "\n").encode("utf-8")


def atomic_create_or_match(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        if path.read_bytes() != data:
            raise ValueError(f"refusing to replace different transition artifact: {path}")
        return
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_path, 0o644)
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--mode", choices=("governed", "ad-hoc"), required=True)
    parser.add_argument("--capability", action="append", default=[])
    parser.add_argument("--rationale", required=True)
    parser.add_argument("--timestamp")
    parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()

    state, _ = load_toml(arguments.state.resolve(strict=True))
    matrix_path = arguments.matrix.resolve(strict=True)
    matrix, matrix_bytes = load_toml(matrix_path)
    errors = validate(state, matrix, set(arguments.capability))
    if errors:
        print("strategy switch refused:", *errors, sep="\n- ")
        return 1

    timestamp = arguments.timestamp or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    datetime.strptime(timestamp, "%Y-%m-%dT%H:%M:%SZ")
    record = render_record(
        state,
        matrix,
        matrix_path,
        matrix_bytes,
        arguments.mode,
        timestamp,
        set(arguments.capability),
        arguments.rationale,
    )
    token = timestamp.replace(":", "").replace("-", "")
    output = arguments.output_dir.resolve()
    record_path = output / "transitions" / f"{token}-{matrix['strategy_id']}.toml"
    selected_path = (
        output / f"{state['phase']}.toml"
        if arguments.mode == "governed"
        else output / "ad-hoc" / f"{matrix['strategy_id']}-{hashlib.sha256(matrix_bytes).hexdigest()[:12]}.toml"
    )
    print(f"root: root\nphase: {state['phase']}\nwork_items: {len(state.get('work', {}).get('paths', []))}")
    print(f"record: {record_path}\nselected_matrix: {selected_path}")
    if not arguments.apply:
        print("preview only; no files changed")
        return 0
    atomic_create_or_match(selected_path, matrix_bytes)
    atomic_create_or_match(record_path, record)
    print("strategy switch recorded")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
