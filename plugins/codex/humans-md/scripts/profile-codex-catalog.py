#!/usr/bin/env python3
"""Guardedly profile an explicit fresh Codex model-catalog export."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import tempfile
import tomllib
from pathlib import Path


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(document: object) -> bytes:
    return (json.dumps(document, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


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
    if parts[-1] not in current:
        raise ValueError(f"declared selector is missing: {dotted}")
    current[parts[-1]] = value


def load_profile(path: Path) -> dict:
    profile = tomllib.loads(path.read_text(encoding="utf-8"))
    if profile.get("schema_version") != 1 or profile.get("adapter") != "codex":
        raise ValueError("unsupported canonical profile schema")
    catalog = profile.get("catalog")
    if not isinstance(catalog, dict):
        raise ValueError("profile has no catalog policy")
    for key in ("id_field", "instruction_fields", "model_message_fields", "selector_fields", "targets"):
        if key not in catalog:
            raise ValueError(f"catalog policy missing {key}")
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

    allowed_instructions = set(policy["instruction_fields"])
    allowed_messages = set(policy["model_message_fields"])
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
                stale.append(f"{model_id}.{field}: expected {expected!r}, found {source_model.get(field)!r}")

        instruction_file = target.get("instruction_file")
        if instruction_file is not None:
            if "base_instructions" not in allowed_instructions:
                raise ValueError("base_instructions is not allowlisted")
            instruction_path = (profile_path.parent / instruction_file).resolve()
            if profile_path.parent.resolve() not in instruction_path.parents:
                raise ValueError("instruction file escapes the profile directory")
            text = instruction_path.read_text(encoding="ascii")
            if not text:
                raise ValueError(f"empty instruction file: {instruction_path}")
            output_model["base_instructions"] = text

        message_patch = target.get("model_messages", {})
        if not isinstance(message_patch, dict) or set(message_patch) - allowed_messages:
            raise ValueError(f"non-allowlisted model-message field for {model_id}")
        if message_patch:
            if not isinstance(output_model.get("model_messages"), dict):
                raise ValueError(f"model_messages missing for {model_id}")
            output_model["model_messages"].update(message_patch)

        selectors = target.get("null_selectors", [])
        if not isinstance(selectors, list) or set(selectors) - allowed_selectors:
            raise ValueError(f"non-allowlisted selector for {model_id}")
        for selector in selectors:
            get_path(output_model, selector)
            set_path(output_model, selector, None)
    return result, stale


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, required=True, help="explicit fresh export")
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--backup-dir", type=Path, required=True)
    parser.add_argument("--strict", action="store_true", help="fail stale expected fields")
    parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()

    catalog_path = arguments.catalog.resolve(strict=True)
    if catalog_path.name == "models_cache.json":
        raise SystemExit("refusing models_cache.json; provide an explicit fresh export")
    profile_path = arguments.profile.resolve(strict=True)
    raw_catalog = catalog_path.read_bytes()
    catalog_document = json.loads(raw_catalog)
    if not isinstance(catalog_document, dict):
        raise SystemExit("catalog root must be an object")
    profiled, stale = build(catalog_document, load_profile(profile_path), profile_path)
    data = canonical(profiled)
    target = arguments.target.expanduser().absolute()
    if target.name == "models_cache.json":
        raise SystemExit("refusing a models_cache.json target")
    print(f"pristine_sha256={sha256(raw_catalog)}")
    print(f"profiled_sha256={sha256(data)}")
    for item in stale:
        print(f"stale: {item}")
    if stale and arguments.strict:
        print("strict stale-model check failed")
        return 1
    if not arguments.apply:
        print("preview only; no files changed")
        return 0

    backup_dir = arguments.backup_dir.resolve()
    backup_dir.mkdir(parents=True, exist_ok=True)
    backup(backup_dir, raw_catalog, "pristine")
    previous = target.read_bytes() if target.exists() else None
    previous_mtime = target.stat().st_mtime_ns if target.exists() else None
    if previous is not None:
        backup(backup_dir, previous, "last-installed")
    if previous == data:
        if target.stat().st_mtime_ns != previous_mtime:
            raise RuntimeError("idempotent target mtime changed unexpectedly")
        print("unchanged")
        return 0
    try:
        atomic_write(target, data)
        if target.read_bytes() != data or json.loads(target.read_text(encoding="ascii")) != profiled:
            raise RuntimeError("strict post-write verification failed")
    except BaseException:
        if previous is None:
            target.unlink(missing_ok=True)
        else:
            atomic_write(target, previous)
        raise
    print("profiled catalog installed with mode 0600")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
