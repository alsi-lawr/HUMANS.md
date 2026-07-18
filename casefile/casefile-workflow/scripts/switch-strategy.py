#!/usr/bin/env python3
"""Validate, preview, and transactionally record a Casefile strategy transition."""
from __future__ import annotations

import argparse
import difflib
import hashlib
import os
import re
import tempfile
import tomllib
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Callable


SAFE_ID = re.compile(r"^[a-z0-9][a-z0-9-]*$")
PHASES = {"planning", "investigation", "review", "implementation", "closeout"}
COORDINATION_KEYS = (
    "batch_when_capacity_exceeded",
    "candidate_review_before_ticket",
    "shared_ticket_storage_required",
)


@dataclass(frozen=True)
class FileState:
    data: bytes
    mode: int
    mtime_ns: int


def load_toml(path: Path) -> tuple[dict, bytes]:
    data = path.read_bytes()
    if not data:
        raise ValueError(f"empty TOML: {path}")
    return tomllib.loads(data.decode("utf-8")), data


def overlaps(left: str, right: str) -> bool:
    a, b = PurePosixPath(left), PurePosixPath(right)
    return a == b or a in b.parents or b in a.parents


def safe_work_path(value: object) -> bool:
    if not isinstance(value, str) or not value.strip() or value.startswith("/"):
        return False
    pure = PurePosixPath(value)
    return ".." not in pure.parts and "\\" not in value


def validate_matrix(matrix: dict) -> list[str]:
    errors: list[str] = []
    required_root = {
        "schema_version",
        "strategy_id",
        "phase",
        "adapter",
        "orchestrator",
        "limits",
        "requirements",
        "coordination",
    }
    missing = required_root - matrix.keys()
    if missing:
        errors.append(f"matrix missing root keys: {', '.join(sorted(missing))}")
    if matrix.get("schema_version") != 1:
        errors.append("matrix schema_version must be 1")
    strategy_id = matrix.get("strategy_id")
    if not isinstance(strategy_id, str) or not SAFE_ID.fullmatch(strategy_id):
        errors.append("matrix strategy_id is invalid")
    if matrix.get("phase") not in PHASES:
        errors.append("matrix phase is invalid")
    if not isinstance(matrix.get("adapter"), str) or not matrix.get("adapter"):
        errors.append("matrix adapter is required")
    if matrix.get("orchestrator", {}).get("binding") != "root":
        errors.append("selected matrix would change the root")

    limits = matrix.get("limits", {})
    concurrency = limits.get("max_concurrent_subagents")
    depth = limits.get("max_depth")
    if not isinstance(concurrency, int) or isinstance(concurrency, bool) or concurrency < 1:
        errors.append("matrix max_concurrent_subagents must be a positive integer")
    if not isinstance(depth, int) or isinstance(depth, bool) or depth < 0:
        errors.append("matrix max_depth must be a non-negative integer")

    required_capabilities = matrix.get("requirements", {}).get("capabilities")
    if not isinstance(required_capabilities, list) or not all(
        isinstance(item, str) and item for item in required_capabilities
    ):
        errors.append("matrix capabilities must be a string array")

    workers = matrix.get("workers", [])
    if not isinstance(workers, list):
        errors.append("matrix workers must be an array")
        workers = []
    minimum_total = 0
    for index, worker in enumerate(workers):
        required = {
            "role",
            "platform_profile",
            "minimum_count",
            "maximum_count",
            "can_spawn_subagents",
        }
        if not isinstance(worker, dict) or required - worker.keys():
            errors.append(f"matrix worker {index} is incomplete")
            continue
        if not isinstance(worker["role"], str) or not worker["role"]:
            errors.append(f"matrix worker {index} role is invalid")
        if not isinstance(worker["platform_profile"], str) or not worker["platform_profile"]:
            errors.append(f"matrix worker {index} profile is invalid")
        minimum = worker["minimum_count"]
        maximum = worker["maximum_count"]
        if (
            not isinstance(minimum, int)
            or isinstance(minimum, bool)
            or not isinstance(maximum, int)
            or isinstance(maximum, bool)
            or minimum < 1
            or minimum > maximum
        ):
            errors.append(f"matrix worker {index} counts are invalid")
        else:
            minimum_total += minimum
        if not isinstance(worker["can_spawn_subagents"], bool):
            errors.append(f"matrix worker {index} spawn flag must be boolean")
        elif worker["can_spawn_subagents"] and isinstance(depth, int) and depth < 2:
            errors.append(f"matrix worker {index} cannot spawn at depth {depth}")
    if isinstance(concurrency, int) and minimum_total > concurrency:
        errors.append("matrix worker minima exceed concurrency")

    coordination = matrix.get("coordination", {})
    for key in COORDINATION_KEYS:
        if not isinstance(coordination.get(key), bool):
            errors.append(f"matrix coordination {key} must be boolean")
    return errors


def validate(
    state: dict,
    matrix: dict,
    capabilities: set[str],
    mode: str = "governed",
) -> list[str]:
    errors = validate_matrix(matrix)
    if state.get("schema_version") != 1:
        errors.append("state schema_version must be 1")
    phase = state.get("phase")
    if phase not in PHASES:
        errors.append("state phase is invalid")
    if matrix.get("phase") != phase:
        errors.append("matrix phase does not match current phase")
    if state.get("root", {}).get("binding") != "root":
        errors.append("current root binding is not root")
    previous = state.get("strategy_id")
    if mode == "governed" and (
        not isinstance(previous, str) or not SAFE_ID.fullmatch(previous)
    ):
        errors.append("governed state strategy_id is invalid")
    elif mode == "ad-hoc" and previous is not None and (
        not isinstance(previous, str) or not SAFE_ID.fullmatch(previous)
    ):
        errors.append("ad-hoc state strategy_id is invalid")

    required = matrix.get("requirements", {}).get("capabilities", [])
    if isinstance(required, list) and all(isinstance(item, str) for item in required):
        missing = sorted(set(required) - capabilities)
        if missing:
            errors.append(f"unavailable capabilities: {', '.join(missing)}")

    work_paths = state.get("work", {}).get("paths", [])
    if not isinstance(work_paths, list) or not all(safe_work_path(item) for item in work_paths):
        errors.append("state work paths must be safe relative strings")

    claims: list[tuple[str, str]] = []
    ownership_entries = state.get("ownership", [])
    if not isinstance(ownership_entries, list):
        errors.append("state ownership must be an array")
        ownership_entries = []
    for ownership in ownership_entries:
        if not isinstance(ownership, dict) or not ownership.get("active", False):
            continue
        owner = ownership.get("owner")
        paths = ownership.get("paths")
        if not isinstance(owner, str) or not owner or not isinstance(paths, list):
            errors.append("active ownership entries require owner and paths")
            continue
        for path in paths:
            if not safe_work_path(path):
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
    backup_path: Path | None,
) -> bytes:
    work = state.get("work", {}).get("paths", [])
    lines = [
        "schema_version = 1",
        f"timestamp = {quote(timestamp)}",
        f"phase = {quote(state['phase'])}",
        f"mode = {quote(mode)}",
        f"previous_strategy_id = {quote(state.get('strategy_id', ''))}",
        f"selected_strategy_id = {quote(matrix['strategy_id'])}",
        f"selected_matrix = {quote(str(matrix_path))}",
        f"selected_matrix_sha256 = {quote(hashlib.sha256(matrix_bytes).hexdigest())}",
        'root_binding = "root"',
        f"governed_state_updated = {'true' if mode == 'governed' else 'false'}",
        f"backup_path = {quote(str(backup_path) if backup_path else '')}",
        f"rationale = {quote(rationale)}",
        "available_capabilities = ["
        + ", ".join(quote(item) for item in sorted(capabilities))
        + "]",
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


def snapshot(path: Path) -> FileState | None:
    if not path.exists():
        return None
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"transition target is not a regular file: {path}")
    stat = path.stat()
    return FileState(path.read_bytes(), stat.st_mode & 0o777, stat.st_mtime_ns)


def atomic_write(path: Path, data: bytes, mode: int = 0o644, mtime_ns: int | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_path, mode)
        if mtime_ns is not None:
            os.utime(temporary_path, ns=(mtime_ns, mtime_ns))
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def restore(path: Path, state: FileState | None) -> None:
    if state is None:
        path.unlink(missing_ok=True)
        return
    atomic_write(path, state.data, state.mode, state.mtime_ns)


def create_or_match(path: Path, data: bytes) -> bool:
    current = snapshot(path)
    if current is not None:
        if current.data != data:
            raise ValueError(f"refusing to replace different transition artifact: {path}")
        return False
    atomic_write(path, data)
    return True


def backup_path_for(output: Path, phase: str, state: FileState | None) -> Path | None:
    if state is None:
        return None
    digest = hashlib.sha256(state.data).hexdigest()
    return output / "backups" / f"{phase}-{digest}.toml"


def matrix_diff(current: FileState | None, selected: bytes, selected_path: Path) -> str:
    old = [] if current is None else current.data.decode("utf-8").splitlines(True)
    new = selected.decode("utf-8").splitlines(True)
    return "".join(
        difflib.unified_diff(old, new, fromfile=str(selected_path), tofile="selected-matrix")
    )


def apply_transaction(
    selected_path: Path,
    matrix_bytes: bytes,
    record_path: Path,
    record: bytes,
    backup_path: Path | None,
    record_writer: Callable[[Path, bytes], bool] = create_or_match,
) -> None:
    selected_before = snapshot(selected_path)
    record_before = snapshot(record_path)
    if record_before is not None and record_before.data != record:
        raise ValueError(f"refusing to replace different transition artifact: {record_path}")
    backup_created = False
    selected_changed = selected_before is None or selected_before.data != matrix_bytes
    try:
        if backup_path is not None:
            backup_created = create_or_match(backup_path, selected_before.data)
        if selected_changed:
            atomic_write(selected_path, matrix_bytes)
        record_writer(record_path, record)
        if selected_path.read_bytes() != matrix_bytes or record_path.read_bytes() != record:
            raise RuntimeError("transition post-write verification failed")
    except BaseException:
        restore(selected_path, selected_before)
        restore(record_path, record_before)
        if backup_created and backup_path is not None:
            backup_path.unlink(missing_ok=True)
        if snapshot(selected_path) != selected_before or snapshot(record_path) != record_before:
            raise RuntimeError("strategy transition rollback verification failed")
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

    try:
        state, _ = load_toml(arguments.state.resolve(strict=True))
        matrix_path = arguments.matrix.resolve(strict=True)
        matrix, matrix_bytes = load_toml(matrix_path)
        errors = validate(state, matrix, set(arguments.capability), arguments.mode)
        if errors:
            raise ValueError("strategy switch refused:\n- " + "\n- ".join(errors))

        timestamp = arguments.timestamp or datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        datetime.strptime(timestamp, "%Y-%m-%dT%H:%M:%SZ")
        output = arguments.output_dir.resolve()
        token = timestamp.replace(":", "").replace("-", "")
        record_path = output / "transitions" / f"{token}-{matrix['strategy_id']}.toml"
        if arguments.mode == "governed":
            selected_path = output / f"{state['phase']}.toml"
        else:
            digest = hashlib.sha256(matrix_bytes).hexdigest()[:12]
            selected_path = output / "ad-hoc" / f"{matrix['strategy_id']}-{digest}.toml"
        current = snapshot(selected_path)
        backup_path = (
            backup_path_for(output, state["phase"], current)
            if arguments.mode == "governed" and current is not None and current.data != matrix_bytes
            else None
        )
        record = render_record(
            state,
            matrix,
            matrix_path,
            matrix_bytes,
            arguments.mode,
            timestamp,
            set(arguments.capability),
            arguments.rationale,
            backup_path,
        )
        print(
            f"root: root\nphase: {state['phase']}\n"
            f"work_items: {len(state.get('work', {}).get('paths', []))}"
        )
        print(f"record: {record_path}\nselected_matrix: {selected_path}")
        print(matrix_diff(current, matrix_bytes, selected_path) or "matrix unchanged")
        if not arguments.apply:
            print("preview only; no files changed")
            return 0
        apply_transaction(selected_path, matrix_bytes, record_path, record, backup_path)
        print("strategy switch recorded transactionally")
        return 0
    except (OSError, UnicodeError, ValueError, RuntimeError, tomllib.TOMLDecodeError) as error:
        print(error)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
