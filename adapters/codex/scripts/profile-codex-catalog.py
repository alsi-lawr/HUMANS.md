#!/usr/bin/env python3
"""Guardedly profile a caller-supplied fresh Codex model-catalog export."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


@dataclass(frozen=True)
class FileState:
    data: bytes
    mode: int
    mtime_ns: int


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(document: object) -> bytes:
    return (json.dumps(document, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode(
        "ascii"
    )


def snapshot(path: Path) -> FileState | None:
    if not path.exists():
        return None
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"target is not a regular file: {path}")
    stat = path.stat()
    return FileState(
        data=path.read_bytes(),
        mode=stat.st_mode & 0o777,
        mtime_ns=stat.st_mtime_ns,
    )


def atomic_write(
    path: Path,
    data: bytes,
    mode: int = 0o600,
    mtime_ns: int | None = None,
) -> None:
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


def backup(path: Path, data: bytes, label: str) -> Path:
    target = path / f"{label}-{sha256(data)}.json"
    if target.exists() and target.read_bytes() != data:
        raise ValueError(f"conflicting hash-addressed backup: {target}")
    if not target.exists():
        atomic_write(target, data)
    return target


def get_path(document: dict, dotted: str) -> object:
    current: object = document
    for part in dotted.split("."):
        if not isinstance(current, dict) or part not in current:
            raise ValueError(f"declared selector is missing: {dotted}")
        current = current[part]
    return current


def set_path(document: dict, dotted: str, value: object) -> None:
    parts = dotted.split(".")
    current = document
    for part in parts[:-1]:
        child = current.get(part)
        if not isinstance(child, dict):
            raise ValueError(f"declared selector parent is missing: {dotted}")
        current = child
    current[parts[-1]] = value


def safe_resource(profile_path: Path, value: object, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError(f"profile target lacks {field}")
    candidate = profile_path.parent / value
    if candidate.is_symlink():
        raise ValueError(f"profile resource is a symlink: {value}")
    resolved = candidate.resolve(strict=True)
    if profile_path.parent.resolve() not in resolved.parents or not resolved.is_file():
        raise ValueError(f"profile resource escapes its directory: {value}")
    return resolved


def resource_bytes(profile_path: Path, target: dict, path_field: str, hash_field: str) -> bytes:
    path = safe_resource(profile_path, target.get(path_field), path_field)
    data = path.read_bytes()
    if not data:
        raise ValueError(f"empty profile resource: {path}")
    expected = target.get(hash_field)
    if not isinstance(expected, str) or sha256(data) != expected:
        raise ValueError(f"profile resource hash mismatch: {path}")
    return data


def load_profile(path: Path) -> dict:
    profile = tomllib.loads(path.read_text(encoding="utf-8"))
    if profile.get("schema_version") != 1 or profile.get("adapter") != "codex":
        raise ValueError("unsupported canonical profile schema")
    catalog = profile.get("catalog")
    if not isinstance(catalog, dict):
        raise ValueError("profile has no catalog policy")
    for key in (
        "id_field",
        "instruction_field",
        "model_messages_field",
        "selector_fields",
        "targets",
    ):
        if key not in catalog:
            raise ValueError(f"catalog policy missing {key}")
    if len({target.get("id") for target in catalog["targets"]}) != len(catalog["targets"]):
        raise ValueError("duplicate canonical profile target")
    return profile


def build(catalog_document: dict, profile: dict, profile_path: Path) -> tuple[dict, list[str]]:
    policy = profile["catalog"]
    models = catalog_document.get("models")
    if not isinstance(models, list) or not models:
        raise ValueError("catalog models must be a non-empty array")
    id_field = policy["id_field"]
    by_id: dict[str, dict] = {}
    for model in models:
        if not isinstance(model, dict) or not isinstance(model.get(id_field), str):
            raise ValueError(f"catalog model lacks string {id_field}")
        model_id = model[id_field]
        if model_id in by_id:
            raise ValueError(f"duplicate model: {model_id}")
        by_id[model_id] = model

    instruction_field = policy["instruction_field"]
    model_messages_field = policy["model_messages_field"]
    if instruction_field != "base_instructions" or model_messages_field != "model_messages":
        raise ValueError("canonical profile declares unsupported patch fields")
    allowed_selectors = set(policy["selector_fields"])
    result = copy.deepcopy(catalog_document)
    result_by_id = {model[id_field]: model for model in result["models"]}
    stale: list[str] = []

    for target in policy["targets"]:
        if not isinstance(target, dict) or not isinstance(target.get("id"), str):
            raise ValueError("profile target lacks an id")
        model_id = target["id"]
        if model_id not in by_id:
            raise ValueError(f"unsupported or missing model: {model_id}")
        source_model = by_id[model_id]
        output_model = result_by_id[model_id]
        efforts = {
            item.get("effort")
            for item in source_model.get("supported_reasoning_levels", [])
            if isinstance(item, dict)
        }
        missing_efforts = sorted(set(target.get("required_reasoning", [])) - efforts)
        if missing_efforts:
            raise ValueError(f"unsupported reasoning for {model_id}: {', '.join(missing_efforts)}")
        for field, expected in target.get("expected", {}).items():
            if source_model.get(field) != expected:
                stale.append(
                    f"{model_id}.{field}: expected {expected!r}, found {source_model.get(field)!r}"
                )

        instructions = resource_bytes(
            profile_path, target, "base_instructions_file", "base_instructions_sha256"
        ).decode("ascii")
        messages = json.loads(
            resource_bytes(
                profile_path, target, "model_messages_file", "model_messages_sha256"
            ).decode("ascii")
        )
        if not isinstance(messages, dict):
            raise ValueError(f"model messages resource must be an object: {model_id}")
        output_model[instruction_field] = instructions
        output_model[model_messages_field] = messages

        selectors = target.get("null_selectors", [])
        if not isinstance(selectors, list) or set(selectors) - allowed_selectors:
            raise ValueError(f"non-allowlisted selector for {model_id}")
        for selector in selectors:
            set_path(output_model, selector, None)
    return result, stale


def verify_target(path: Path, data: bytes, profiled: dict) -> None:
    if path.read_bytes() != data or json.loads(path.read_text(encoding="ascii")) != profiled:
        raise RuntimeError("strict post-write verification failed")


def install_profiled_catalog(
    target: Path,
    data: bytes,
    profiled: dict,
    raw_catalog: bytes,
    backup_dir: Path,
    verifier: Callable[[Path, bytes, dict], None] = verify_target,
) -> str:
    backup_dir.mkdir(parents=True, exist_ok=True)
    backup(backup_dir, raw_catalog, "pristine")
    previous = snapshot(target)
    if previous is not None:
        backup(backup_dir, previous.data, "last-installed")
    if previous is not None and previous.data == data:
        unchanged = snapshot(target)
        if unchanged != previous:
            raise RuntimeError("idempotent target metadata changed unexpectedly")
        return "unchanged"
    try:
        atomic_write(target, data, 0o600)
        verifier(target, data, profiled)
    except BaseException:
        restore(target, previous)
        if snapshot(target) != previous:
            raise RuntimeError("catalog rollback verification failed")
        raise
    return "installed"


def checked_input(path: Path, label: str) -> Path:
    supplied = path.expanduser().absolute()
    if supplied.name.lower() == "models_cache.json":
        raise ValueError(f"refusing {label} named models_cache.json")
    if supplied.is_symlink():
        raise ValueError(f"refusing symlink {label}: {supplied}")
    resolved = supplied.resolve(strict=True)
    if resolved.name.lower() == "models_cache.json":
        raise ValueError(f"refusing {label} alias of models_cache.json")
    return resolved


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, required=True, help="caller-supplied fresh export")
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--backup-dir", type=Path, required=True)
    parser.add_argument("--strict", action="store_true", help="fail stale expected fields")
    parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()

    try:
        catalog_path = checked_input(arguments.catalog, "catalog")
        profile_path = checked_input(arguments.profile, "profile")
        target = arguments.target.expanduser().absolute()
        if target.name.lower() == "models_cache.json":
            raise ValueError("refusing a models_cache.json target")
        if target.is_symlink():
            raise ValueError("refusing a symlink target")
        if target.resolve(strict=False) == catalog_path:
            raise ValueError("catalog input and target must be separate files")
        raw_catalog = catalog_path.read_bytes()
        catalog_document = json.loads(raw_catalog)
        if not isinstance(catalog_document, dict):
            raise ValueError("catalog root must be an object")
        profiled, stale = build(catalog_document, load_profile(profile_path), profile_path)
        data = canonical(profiled)
        print(f"pristine_sha256={sha256(raw_catalog)}")
        print(f"profiled_sha256={sha256(data)}")
        for item in stale:
            print(f"stale: {item}")
        if stale and arguments.strict:
            print("strict stale-model check failed")
            return 1
        if not arguments.apply:
            print("preview only; no files changed; export freshness is caller-supplied")
            return 0
        backup_dir = arguments.backup_dir.expanduser().absolute()
        if backup_dir.is_symlink():
            raise ValueError("refusing a symlink backup directory")
        result = install_profiled_catalog(
            target, data, profiled, raw_catalog, backup_dir.resolve()
        )
        print(f"{result}; profiled catalog target mode is 0600")
        return 0
    except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(f"catalog profiling failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
