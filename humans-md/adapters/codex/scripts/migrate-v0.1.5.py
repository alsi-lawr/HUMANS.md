#!/usr/bin/env python3
"""Restore a supported humans-md 0.1.5 Codex receipt, then reseed core 0.2.0."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import importlib.util
import json
import os
import subprocess
import tempfile
from pathlib import Path


def lifecycle():
    path = Path(__file__).with_name("setup-codex.py")
    spec = importlib.util.spec_from_file_location("core_setup", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load core lifecycle")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


core = lifecycle()
LEGACY_SCHEMA = {2, 3}
LEGACY_PATHS = (
    "config.toml", "AGENTS.md", "models-humans-md-v1.json",
    "skills/investigation-atomic", "skills/investigation-inspector-tree", "skills/investigation-review-atomic",
    "skills/investigation-review-dialogue", "skills/investigation-review-two-stage", "skills/investigation-solo",
    "skills/ticket-batch-subagent-pipeline", "skills/ticket-scratch-closeout", "skills/ticketed-repository-investigation",
    "skills/git-contribution", "skills/readme-generator", "skills/skill-generator", "skills/skill-packaging",
    "skills/casefile-workflow", "skills/casefile-investigate-solo", "skills/casefile-investigate-atomic",
    "skills/casefile-investigate-inspector-tree", "skills/casefile-review-atomic", "skills/casefile-review-dialogue",
    "skills/casefile-review-two-stage", "skills/casefile-implement-ticket-batch", "skills/casefile-switch-strategy",
    "skills/casefile-closeout", "skills/casefile-codex-setup", "skills/casefile-codex-cutover",
    "skills/casefile-codex-catalog-profile", "skills/casefile-codex-uninstall", "agents/inspector.toml",
    "agents/detective.toml", "agents/dialogue-review-chair.toml", "agents/dialogue-review-challenger.toml",
    "agents/atomic-ticket-reviewer.toml", "agents/verification-reviewer.toml", "agents/implementation-writer.toml",
    "planning-workflow", "casefile-workflow", "models-sol-v1.json", "models-sol-v1-bak.json",
)
SCALAR_BEGIN = b"# >>> humans-md setup scalars >>>\n"
SCALAR_END = b"# <<< humans-md setup scalars <<<\n"
TABLE_BEGIN = b"\n# >>> humans-md setup tables >>>\n"
TABLE_END = b"# <<< humans-md setup tables <<<\n"


class MigrationError(RuntimeError):
    pass


def legacy_receipt(home: Path) -> tuple[Path, dict]:
    try:
        selected = json.loads(core.pointer(home).read_bytes())
        path = Path(selected["receipt"]).resolve(strict=True)
        value = json.loads(path.read_bytes())
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        raise MigrationError(f"no supported v0.1.5 receipt: {error}") from error
    if core.backup_root(home).resolve() not in path.parents or path.name != "receipt.json":
        raise MigrationError("legacy receipt is outside the durable backup root")
    if value.get("schema_version") not in LEGACY_SCHEMA or value.get("status") != "installed" or value.get("plugin_version") != "0.1.5":
        raise MigrationError("receipt is not a supported humans-md 0.1.5 installation")
    if value.get("remove_plugin") is not True or value.get("remove_marketplace") is not True:
        raise MigrationError("legacy receipt removal policy is not supported")
    core.safe_inventory(home, value.get("before"), LEGACY_PATHS)
    return path, value


def path_fingerprint(path: Path) -> str:
    digest = hashlib.sha256()
    def add(current: Path, relative: str) -> None:
        if current.is_symlink():
            digest.update(f"L {relative} ".encode("utf-8")); digest.update(os.readlink(current).encode("utf-8")); return
        if current.is_file():
            digest.update(f"F {relative} ".encode("utf-8")); digest.update(current.read_bytes()); return
        if current.is_dir():
            digest.update(f"D {relative}\n".encode("utf-8"))
            for child in sorted(current.iterdir(), key=lambda item: item.name): add(child, f"{relative}/{child.name}")
            return
        digest.update(f"M {relative}\n".encode("utf-8"))
    add(path, ".")
    return digest.hexdigest()


def approval_fingerprint(home: Path, receipt_path: Path) -> str:
    values = {relative: path_fingerprint(home / relative) for relative in LEGACY_PATHS}
    values["state/humans-md/current.json"] = path_fingerprint(core.pointer(home))
    values["legacy-receipt"] = path_fingerprint(receipt_path)
    values["legacy-before"] = path_fingerprint(receipt_path.parent / "before")
    return hashlib.sha256(core.canonical(values)).hexdigest()

def unowned_config(data: bytes) -> bytes:
    ranges = []
    for name, begin, end in (("scalars", SCALAR_BEGIN, SCALAR_END), ("tables", TABLE_BEGIN, TABLE_END)):
        if data.count(begin) != 1 or data.count(end) != 1:
            raise MigrationError(f"legacy managed config block is missing or duplicated: {name}")
        start = data.index(begin)
        stop = data.index(end, start) + len(end)
        ranges.append((start, stop))
    ranges.sort()
    if ranges[0][1] > ranges[1][0]:
        raise MigrationError("legacy managed config blocks overlap")
    result = data[:ranges[0][0]] + data[ranges[0][1]:ranges[1][0]] + data[ranges[1][1]:]
    if result:
        import tomllib
        tomllib.loads(result.decode("utf-8"))
    return result


def diff(left: Path, right: Path) -> None:
    result = subprocess.run(["git", "diff", "--no-index", "--", str(left), str(right)], capture_output=True, text=True, encoding="utf-8", errors="strict")
    if result.returncode not in (0, 1):
        raise MigrationError(result.stderr.strip() or "git diff --no-index failed")
    if result.stdout:
        print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")


def show_diffs(home: Path, receipt_path: Path, receipt: dict) -> None:
    before = {entry["path"]: entry for entry in receipt["before"]}
    with tempfile.TemporaryDirectory(prefix="humans-md-migration-") as temporary:
        missing = Path(temporary) / "missing"
        missing.touch()
        for relative in LEGACY_PATHS:
            current = home / relative
            baseline = receipt_path.parent / "before" / relative if before[relative]["existed"] else missing
            if relative == "config.toml" and current.is_file():
                candidate = Path(temporary) / "config.toml"
                candidate.write_bytes(unowned_config(current.read_bytes()))
                baseline = candidate
            diff(current if current.exists() or current.is_symlink() else missing, baseline)


def preview(home: Path, plugin_root: Path, executable: str) -> dict:
    receipt_path, receipt = legacy_receipt(home)
    plan = core.prepare(plugin_root, home, executable)
    return {"operation": "migrate-v0.1.5-to-v0.2.0", "legacy_receipt": str(receipt_path), "restore_snapshot": str(receipt_path.parent / "before"), "legacy_managed_path_count": len(LEGACY_PATHS), "fresh_core_setup": core.preview(plan), "approval_fingerprint": approval_fingerprint(home, receipt_path), "marketplace_preserved": True, "install_siblings": False}, receipt_path, receipt, plan


def restore_legacy_baseline(home: Path, receipt_path: Path, receipt: dict) -> None:
    inventory = core.safe_inventory(home, receipt["before"], LEGACY_PATHS)
    config = home / "config.toml"
    current_unowned = unowned_config(config.read_bytes()) if config.is_file() else None
    core.restore(home, receipt_path.parent / "before", inventory[1:])
    if current_unowned is None:
        core.restore(home, receipt_path.parent / "before", inventory[:1])
    elif current_unowned or inventory[0]["existed"]:
        core.atomic_write(config, current_unowned)
    else:
        core.remove(config)


def apply(home: Path, plugin_root: Path, executable: str, approval: str) -> dict:
    reviewed, receipt_path, receipt, plan = preview(home, plugin_root, executable)
    if approval != reviewed["approval_fingerprint"]:
        show_diffs(home, receipt_path, receipt)
        raise MigrationError("stale approval: managed state changed; review the recomputed diff")
    # Re-read all mutable state immediately before writing; the approval binds every target.
    reviewed, receipt_path, receipt, plan = preview(home, plugin_root, executable)
    if approval != reviewed["approval_fingerprint"]:
        show_diffs(home, receipt_path, receipt)
        raise MigrationError("stale approval: managed state changed; review the recomputed diff")
    rollback_dir = Path(tempfile.mkdtemp(prefix="migration-" + datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-"), dir=core.backup_root(home)))
    rollback_paths = [home / item for item in LEGACY_PATHS] + [core.pointer(home)]
    rollback = core.snapshot(home, rollback_paths, rollback_dir / "before")
    fresh_receipt: Path | None = None
    try:
        restore_legacy_baseline(home, receipt_path, receipt)
        core.pointer(home).unlink(missing_ok=True)
        result = core.install(plan)
        fresh_receipt = Path(result["receipt"])
        core.atomic_write(rollback_dir / "receipt.json", core.canonical({"status": "migrated", "legacy_receipt": str(receipt_path), "fresh_receipt": str(fresh_receipt)}))
        return {"status": "migrated", "fresh_receipt": str(fresh_receipt), "marketplace_preserved": True, "siblings_installed": False}
    except BaseException as error:
        core.restore(home, rollback_dir / "before", rollback)
        if fresh_receipt is not None:
            core.remove(fresh_receipt.parent)
        core.atomic_write(rollback_dir / "failure.json", core.canonical({"status": "failed", "error": str(error), "rollback_verified": True}))
        raise MigrationError(f"migration failed; legacy state rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugin-root", type=Path, required=True)
    parser.add_argument("--codex-home", type=Path, default=Path(os.environ.get("CODEX_HOME", "~/.codex")))
    parser.add_argument("--codex-executable", default="codex")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval")
    arguments = parser.parse_args()
    try:
        home = arguments.codex_home.expanduser().resolve(strict=True)
        plan, receipt_path, receipt, _ = preview(home, arguments.plugin_root, arguments.codex_executable)
        print(json.dumps(plan, indent=2, sort_keys=True))
        show_diffs(home, receipt_path, receipt)
        if not arguments.apply:
            print("preview only; no files changed")
            return 0
        if not arguments.approval:
            raise MigrationError("--approval must equal the preview approval_fingerprint")
        print(json.dumps(apply(home, arguments.plugin_root, arguments.codex_executable, arguments.approval), indent=2, sort_keys=True))
        return 0
    except (OSError, UnicodeError, ValueError, MigrationError, core.SetupError) as error:
        print(f"migration failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
