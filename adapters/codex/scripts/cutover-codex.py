#!/usr/bin/env python3
"""Preview or run an explicit rollback-verified Codex Casefile cutover transaction."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Callable


REQUIRED_PATH_KINDS = {
    "active_config",
    "direct_skills",
    "direct_agents",
    "workflow_resources",
    "marketplace_state",
}
REMOVABLE_PATH_KINDS = {"direct_skills", "direct_agents", "workflow_resources"}
REQUIRED_GATES = {
    "strict_config",
    "discovery",
    "v1_runtime",
    "root_profile",
    "inspector_profile",
}
FRESH_GATES = {"v1_runtime", "root_profile", "inspector_profile"}


class CutoverError(RuntimeError):
    pass


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode(
        "ascii"
    )


def atomic_write(path: Path, data: bytes, mode: int = 0o600) -> None:
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
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def absolute_path(value: object, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise CutoverError(f"{field} must be an absolute path")
    pure = PurePosixPath(value)
    if not pure.is_absolute() or ".." in pure.parts:
        raise CutoverError(f"{field} must be an absolute path without parent traversal")
    path = Path(value)
    if path in {Path("/"), Path.home()}:
        raise CutoverError(f"{field} is too broad to manage transactionally")
    return path


def paths_overlap(left: Path, right: Path) -> bool:
    return left == right or left in right.parents or right in left.parents


def load_plan(path: Path) -> tuple[dict, bytes]:
    data = path.read_bytes()
    if not data:
        raise CutoverError(f"empty cutover plan: {path}")
    try:
        return tomllib.loads(data.decode("utf-8")), data
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise CutoverError(f"invalid cutover plan: {error}") from error


def validate_plan(document: dict, plugin_root: Path) -> list[str]:
    errors: list[str] = []
    if document.get("schema_version") != 1:
        errors.append("cutover plan schema_version must be 1")
    for field in ("install_ref", "codex_executable"):
        if not isinstance(document.get(field), str) or not document[field]:
            errors.append(f"cutover plan {field} is required")
    try:
        candidate = absolute_path(document.get("candidate_config"), "candidate_config")
        active = absolute_path(document.get("active_config"), "active_config")
        codex_home = absolute_path(document.get("codex_home"), "codex_home")
        if candidate.is_symlink() or not candidate.is_file():
            errors.append("candidate_config must be an existing regular file")
        if active == candidate:
            errors.append("candidate and active config paths must differ")
    except CutoverError as error:
        errors.append(str(error))
        candidate = active = codex_home = Path("/invalid")

    managed = document.get("managed_paths")
    if not isinstance(managed, list) or not managed:
        errors.append("managed_paths must be a non-empty array")
        managed = []
    kinds: set[str] = set()
    paths: list[Path] = []
    active_matches = 0
    for index, item in enumerate(managed):
        if not isinstance(item, dict):
            errors.append(f"managed path {index} must be a table")
            continue
        kind = item.get("kind")
        if kind not in REQUIRED_PATH_KINDS:
            errors.append(f"managed path {index} has invalid kind")
        else:
            kinds.add(kind)
        try:
            path = absolute_path(item.get("path"), f"managed_paths[{index}].path")
            for previous in paths:
                if paths_overlap(path, previous):
                    errors.append(f"managed paths overlap: {previous} and {path}")
            paths.append(path)
            if kind == "active_config" and path == active:
                active_matches += 1
        except CutoverError as error:
            errors.append(str(error))
        remove = item.get("remove_after_success")
        if not isinstance(remove, bool):
            errors.append(f"managed path {index} remove_after_success must be boolean")
        elif remove and kind not in REMOVABLE_PATH_KINDS:
            errors.append(f"managed path {index} cannot be removed after success")
    if kinds != REQUIRED_PATH_KINDS:
        errors.append("managed_paths must inventory config, direct copies, workflow, and marketplace state")
    if active_matches != 1:
        errors.append("active_config must match exactly one managed active_config path")
    if not any(path == codex_home or codex_home in path.parents for path in paths):
        errors.append("managed_paths must include Codex-home marketplace state")

    gates = document.get("gates")
    if not isinstance(gates, list):
        errors.append("gates must be an array")
        gates = []
    gate_kinds: set[str] = set()
    for index, gate in enumerate(gates):
        if not isinstance(gate, dict):
            errors.append(f"gate {index} must be a table")
            continue
        kind = gate.get("kind")
        if kind not in REQUIRED_GATES or kind in gate_kinds:
            errors.append(f"gate {index} kind is missing, duplicate, or unsupported")
        else:
            gate_kinds.add(kind)
        command = gate.get("command")
        if not isinstance(command, list) or not command or not all(
            isinstance(item, str) and item for item in command
        ):
            errors.append(f"gate {index} command must be a string array")
        expected = gate.get("expected", [])
        if not isinstance(expected, list) or not all(isinstance(item, str) for item in expected):
            errors.append(f"gate {index} expected must be a string array")
        if kind in FRESH_GATES:
            if gate.get("fresh_process") is not True:
                errors.append(f"gate {kind} must declare fresh_process = true")
            if not expected:
                errors.append(f"gate {kind} must declare expected probe values")
    if gate_kinds != REQUIRED_GATES:
        errors.append("cutover plan must declare every strict, discovery, V1, root, and inspector gate")
    if not (plugin_root / ".codex-plugin/plugin.json").is_file():
        errors.append("plugin_root is not a packaged Codex plugin")
    return errors


def entry_for(path: Path, relative: str) -> dict:
    stat_result = path.lstat()
    mode = stat.S_IMODE(stat_result.st_mode)
    if path.is_symlink():
        return {"path": relative, "type": "symlink", "target": os.readlink(path)}
    if path.is_dir():
        return {
            "path": relative,
            "type": "directory",
            "mode": mode,
            "mtime_ns": stat_result.st_mtime_ns,
        }
    if path.is_file():
        data = path.read_bytes()
        return {
            "path": relative,
            "type": "file",
            "mode": mode,
            "mtime_ns": stat_result.st_mtime_ns,
            "sha256": sha256(data),
            "size": len(data),
        }
    raise CutoverError(f"unsupported managed entry: {path}")


def capture(path: Path) -> list[dict]:
    if not path.exists() and not path.is_symlink():
        return [{"path": ".", "type": "missing"}]
    entries = [entry_for(path, ".")]
    if path.is_dir() and not path.is_symlink():
        for candidate in sorted(path.rglob("*"), key=lambda item: item.relative_to(path).as_posix()):
            entries.append(entry_for(candidate, candidate.relative_to(path).as_posix()))
    return entries


def store_objects(path: Path, entries: list[dict], objects: Path) -> None:
    for entry in entries:
        if entry["type"] != "file":
            continue
        source = path if entry["path"] == "." else path / entry["path"]
        data = source.read_bytes()
        if sha256(data) != entry["sha256"]:
            raise CutoverError(f"managed file changed during backup: {source}")
        target = objects / entry["sha256"]
        if target.exists() and target.read_bytes() != data:
            raise CutoverError(f"backup object collision: {target}")
        if not target.exists():
            atomic_write(target, data)


def remove_path(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink(missing_ok=True)
    elif path.is_dir():
        shutil.rmtree(path)


def restore_capture(path: Path, entries: list[dict], objects: Path) -> None:
    remove_path(path)
    if entries == [{"path": ".", "type": "missing"}]:
        return
    directories = [entry for entry in entries if entry["type"] == "directory"]
    for entry in sorted(directories, key=lambda item: len(PurePosixPath(item["path"]).parts)):
        target = path if entry["path"] == "." else path / entry["path"]
        target.mkdir(parents=True, exist_ok=True)
    for entry in entries:
        target = path if entry["path"] == "." else path / entry["path"]
        if entry["type"] == "file":
            target.parent.mkdir(parents=True, exist_ok=True)
            data = (objects / entry["sha256"]).read_bytes()
            atomic_write(target, data, entry["mode"])
            os.utime(target, ns=(entry["mtime_ns"], entry["mtime_ns"]))
        elif entry["type"] == "symlink":
            target.parent.mkdir(parents=True, exist_ok=True)
            target.symlink_to(entry["target"])
    for entry in sorted(
        directories, key=lambda item: len(PurePosixPath(item["path"]).parts), reverse=True
    ):
        target = path if entry["path"] == "." else path / entry["path"]
        target.chmod(entry["mode"])
        os.utime(target, ns=(entry["mtime_ns"], entry["mtime_ns"]))


def inventory(document: dict) -> list[dict]:
    values = []
    for item in document["managed_paths"]:
        path = Path(item["path"])
        entries = capture(path)
        values.append(
            {
                "kind": item["kind"],
                "path": str(path),
                "remove_after_success": item["remove_after_success"],
                "tree_sha256": sha256(canonical(entries)),
                "entries": entries,
            }
        )
    return values


def verify_inventory(values: list[dict]) -> None:
    failures = []
    for item in values:
        actual = capture(Path(item["path"]))
        if sha256(canonical(actual)) != item["tree_sha256"]:
            failures.append(item["path"])
    if failures:
        raise CutoverError(f"rollback verification failed: {', '.join(failures)}")


def subprocess_runner(command: list[str], environment: dict[str, str]) -> CommandResult:
    result = subprocess.run(
        command,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    return CommandResult(result.returncode, result.stdout, result.stderr)


def run_command(
    label: str,
    command: list[str],
    environment: dict[str, str],
    runner: Callable[[list[str], dict[str, str]], CommandResult],
    expected: list[str] | None = None,
) -> dict:
    result = runner(command, environment)
    combined = result.stdout + result.stderr
    if result.returncode:
        raise CutoverError(f"{label} failed with exit code {result.returncode}")
    missing = [item for item in expected or [] if item not in combined]
    if missing:
        raise CutoverError(f"{label} omitted expected probe values: {', '.join(missing)}")
    return {
        "label": label,
        "command": command,
        "stdout_sha256": sha256(result.stdout.encode("utf-8")),
        "stderr_sha256": sha256(result.stderr.encode("utf-8")),
    }


def run_cutover(
    document: dict,
    plan_bytes: bytes,
    plugin_root: Path,
    backup_dir: Path,
    record_path: Path,
    runner: Callable[[list[str], dict[str, str]], CommandResult] = subprocess_runner,
) -> dict:
    values = inventory(document)
    if backup_dir.exists() and any(backup_dir.iterdir()):
        raise CutoverError(f"backup directory is not empty: {backup_dir}")
    backup_dir.mkdir(parents=True, exist_ok=True)
    objects = backup_dir / "objects"
    objects.mkdir()
    for item in values:
        store_objects(Path(item["path"]), item["entries"], objects)
    inventory_data = canonical({"schema_version": 1, "managed_paths": values})
    atomic_write(backup_dir / "inventory.json", inventory_data)

    environment = dict(os.environ)
    environment["CODEX_HOME"] = document["codex_home"]
    commands: list[dict] = []
    record = {
        "schema_version": 1,
        "status": "running",
        "plan_sha256": sha256(plan_bytes),
        "plugin_root": str(plugin_root),
        "backup_inventory_sha256": sha256(inventory_data),
        "commands": commands,
        "removed_paths": [],
        "rollback_verified": False,
    }
    try:
        candidate = Path(document["candidate_config"])
        active = Path(document["active_config"])
        active_mode = (active.stat().st_mode & 0o777) if active.is_file() else 0o600
        executable = document["codex_executable"]
        commands.append(
            run_command(
                "marketplace_add",
                [executable, "plugin", "marketplace", "add", str(plugin_root), "--json"],
                environment,
                runner,
            )
        )
        commands.append(
            run_command(
                "plugin_add",
                [executable, "plugin", "add", document["install_ref"], "--json"],
                environment,
                runner,
            )
        )
        atomic_write(active, candidate.read_bytes(), active_mode)
        for gate in document["gates"]:
            commands.append(
                run_command(
                    gate["kind"], gate["command"], environment, runner, gate.get("expected", [])
                )
            )
        for item in values:
            if item["remove_after_success"]:
                path = Path(item["path"])
                remove_path(path)
                if path.exists() or path.is_symlink():
                    raise CutoverError(f"selective old-copy removal failed: {path}")
                record["removed_paths"].append(str(path))
        record["status"] = "success"
        atomic_write(record_path, canonical(record))
        return record
    except BaseException as error:
        rollback_error: BaseException | None = None
        try:
            for item in values:
                restore_capture(Path(item["path"]), item["entries"], objects)
            verify_inventory(values)
            record["rollback_verified"] = True
        except BaseException as caught:
            rollback_error = caught
        record["status"] = "failed"
        record["failure"] = str(error)
        if rollback_error is not None:
            record["rollback_failure"] = str(rollback_error)
        atomic_write(record_path, canonical(record))
        if rollback_error is not None:
            raise CutoverError(f"cutover failed and rollback failed: {rollback_error}") from error
        raise CutoverError(f"cutover failed; rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--plugin-root", type=Path, required=True)
    parser.add_argument("--backup-dir", type=Path, required=True)
    parser.add_argument("--record", type=Path, required=True)
    parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()
    try:
        plan, plan_bytes = load_plan(arguments.plan.resolve(strict=True))
        plugin_root = arguments.plugin_root.resolve(strict=True)
        errors = validate_plan(plan, plugin_root)
        if errors:
            raise CutoverError("invalid cutover plan:\n- " + "\n- ".join(errors))
        preview = inventory(plan)
        print(f"plugin_root: {plugin_root}")
        print(f"plan_sha256: {sha256(plan_bytes)}")
        for item in preview:
            print(
                f"{item['kind']}: {item['path']} tree_sha256={item['tree_sha256']} "
                f"remove_after_success={item['remove_after_success']}"
            )
        for gate in plan["gates"]:
            print(f"gate {gate['kind']}: {json.dumps(gate['command'])}")
        if not arguments.apply:
            print("preview only; no marketplace, configuration, probe, or removal action ran")
            return 0
        run_cutover(
            plan,
            plan_bytes,
            plugin_root,
            arguments.backup_dir.expanduser().absolute(),
            arguments.record.expanduser().absolute(),
        )
        print("Codex cutover succeeded and selective old-copy removal was verified")
        return 0
    except (OSError, CutoverError, UnicodeError, tomllib.TOMLDecodeError) as error:
        print(f"Codex cutover refused or failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
