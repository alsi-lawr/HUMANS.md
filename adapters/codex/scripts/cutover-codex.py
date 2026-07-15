#!/usr/bin/env python3
"""Preview or run rollback-verified Codex Casefile install and uninstall transactions."""
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
REQUIRED_RECOVERY_GATES = {"strict_config", "discovery"}
MARKETPLACE_ACTIONS = {"add", "upgrade", "reuse"}
PLUGIN_ACTIONS = {"add", "reuse"}
RECOVERY_MANIFEST = "recovery.json"


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
    for field in (
        "install_ref",
        "codex_executable",
        "marketplace_source",
        "marketplace_name",
        "marketplace_action",
        "plugin_action",
    ):
        if not isinstance(document.get(field), str) or not document[field]:
            errors.append(f"cutover plan {field} is required")
    if document.get("marketplace_action") not in MARKETPLACE_ACTIONS:
        errors.append("marketplace_action must be add, upgrade, or reuse")
    if document.get("plugin_action") not in PLUGIN_ACTIONS:
        errors.append("plugin_action must be add or reuse")
    for field in ("remove_plugin_on_uninstall", "remove_marketplace_on_uninstall"):
        if not isinstance(document.get(field), bool):
            errors.append(f"cutover plan {field} must be boolean")
    if document.get("remove_marketplace_on_uninstall") is True and document.get(
        "remove_plugin_on_uninstall"
    ) is not True:
        errors.append("marketplace removal requires plugin removal")
    marketplace_name = document.get("marketplace_name")
    install_ref = document.get("install_ref")
    if (
        isinstance(marketplace_name, str)
        and marketplace_name
        and isinstance(install_ref, str)
        and not install_ref.endswith(f"@{marketplace_name}")
    ):
        errors.append("install_ref must select the declared marketplace_name")
    marketplace_ref = document.get("marketplace_ref")
    if marketplace_ref is not None and (
        not isinstance(marketplace_ref, str) or not marketplace_ref
    ):
        errors.append("marketplace_ref must be a non-empty string when declared")
    marketplace_source = document.get("marketplace_source")
    if isinstance(marketplace_source, str) and marketplace_source:
        source_path = Path(marketplace_source).expanduser()
        if source_path.is_absolute():
            try:
                source_root = source_path.resolve(strict=True)
                if source_root == plugin_root:
                    errors.append("marketplace_source must not be the installed plugin root")
                if not (source_root / ".agents/plugins/marketplace.json").is_file():
                    errors.append("local marketplace_source lacks .agents/plugins/marketplace.json")
                if marketplace_ref is not None:
                    errors.append("local marketplace_source cannot declare marketplace_ref")
            except OSError as error:
                errors.append(f"invalid local marketplace_source: {error}")
        elif marketplace_source.startswith((".", "~")):
            errors.append("local marketplace_source must be an absolute path")
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
    recovery_gates = document.get("recovery_gates")
    if not isinstance(recovery_gates, list):
        errors.append("recovery_gates must be an array")
        recovery_gates = []
    recovery_kinds: set[str] = set()
    for index, gate in enumerate(recovery_gates):
        if not isinstance(gate, dict):
            errors.append(f"recovery gate {index} must be a table")
            continue
        kind = gate.get("kind")
        if kind not in REQUIRED_RECOVERY_GATES or kind in recovery_kinds:
            errors.append(f"recovery gate {index} kind is missing, duplicate, or unsupported")
        else:
            recovery_kinds.add(kind)
        command = gate.get("command")
        if not isinstance(command, list) or not command or not all(
            isinstance(item, str) and item for item in command
        ):
            errors.append(f"recovery gate {index} command must be a string array")
        expected = gate.get("expected", [])
        if not isinstance(expected, list) or not all(isinstance(item, str) for item in expected):
            errors.append(f"recovery gate {index} expected must be a string array")
    if recovery_kinds != REQUIRED_RECOVERY_GATES:
        errors.append("cutover plan must declare strict-config and discovery recovery gates")
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


def content_sha256(entries: list[dict]) -> str:
    content = []
    for entry in entries:
        value = {"path": entry["path"], "type": entry["type"]}
        if entry["type"] == "file":
            value["sha256"] = entry["sha256"]
        elif entry["type"] == "symlink":
            value["target"] = entry["target"]
        content.append(value)
    return sha256(canonical(content))


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
                "content_sha256": content_sha256(entries),
                "entries": entries,
            }
        )
    return values


def compact_inventory(values: list[dict]) -> list[dict]:
    return [
        {
            "kind": item["kind"],
            "path": item["path"],
            "remove_after_success": item["remove_after_success"],
            "content_sha256": item["content_sha256"],
        }
        for item in values
    ]


def inventory_document(values: list[dict]) -> dict:
    return {
        "managed_paths": [
            {
                "kind": item["kind"],
                "path": item["path"],
                "remove_after_success": item["remove_after_success"],
            }
            for item in values
        ]
    }


def verify_inventory(values: list[dict]) -> None:
    failures = []
    for item in values:
        actual = capture(Path(item["path"]))
        if sha256(canonical(actual)) != item["tree_sha256"]:
            failures.append(item["path"])
    if failures:
        raise CutoverError(f"rollback verification failed: {', '.join(failures)}")


def ensure_external_paths(managed: list[dict], *external: Path) -> None:
    managed_paths = [Path(item["path"]) for item in managed]
    for candidate in external:
        absolute = candidate.expanduser().absolute()
        if any(paths_overlap(absolute, path) for path in managed_paths):
            raise CutoverError(f"transaction output overlaps managed state: {absolute}")
    for index, left in enumerate(external):
        for right in external[index + 1 :]:
            if paths_overlap(left.expanduser().absolute(), right.expanduser().absolute()):
                raise CutoverError(f"transaction outputs overlap: {left} and {right}")


def create_backup(values: list[dict], backup_dir: Path) -> tuple[Path, bytes]:
    if backup_dir.exists():
        if not backup_dir.is_dir() or any(backup_dir.iterdir()):
            raise CutoverError(f"backup directory is not empty: {backup_dir}")
    else:
        backup_dir.mkdir(parents=True)
    objects = backup_dir / "objects"
    objects.mkdir()
    for item in values:
        store_objects(Path(item["path"]), item["entries"], objects)
    inventory_data = canonical({"schema_version": 1, "managed_paths": values})
    atomic_write(backup_dir / "inventory.json", inventory_data)
    return objects, inventory_data


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


def marketplace_install_command(document: dict) -> list[str] | None:
    executable = document["codex_executable"]
    action = document["marketplace_action"]
    if action == "reuse":
        return None
    if action == "upgrade":
        return [
            executable,
            "plugin",
            "marketplace",
            "upgrade",
            document["marketplace_name"],
            "--json",
        ]
    command = [
        executable,
        "plugin",
        "marketplace",
        "add",
        document["marketplace_source"],
    ]
    if document.get("marketplace_ref"):
        command.extend(["--ref", document["marketplace_ref"]])
    command.append("--json")
    return command


def plugin_install_command(document: dict) -> list[str] | None:
    if document["plugin_action"] == "reuse":
        return None
    return [
        document["codex_executable"],
        "plugin",
        "add",
        document["install_ref"],
        "--json",
    ]


def load_json(path: Path, label: str) -> tuple[dict, bytes]:
    try:
        data = path.read_bytes()
        value = json.loads(data)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise CutoverError(f"invalid {label}: {error}") from error
    if not isinstance(value, dict):
        raise CutoverError(f"invalid {label}: root must be an object")
    return value, data


def validate_backup_objects(values: list[dict], objects: Path) -> None:
    if not objects.is_dir():
        raise CutoverError(f"backup objects directory is missing: {objects}")
    for item in values:
        path = absolute_path(item.get("path"), "backup managed path")
        entries = item.get("entries")
        if not isinstance(entries, list) or not entries:
            raise CutoverError(f"backup entries are missing: {path}")
        if item.get("tree_sha256") != sha256(canonical(entries)):
            raise CutoverError(f"backup tree hash mismatch: {path}")
        if item.get("content_sha256") != content_sha256(entries):
            raise CutoverError(f"backup content hash mismatch: {path}")
        for entry in entries:
            if not isinstance(entry, dict) or entry.get("type") not in {
                "missing",
                "directory",
                "file",
                "symlink",
            }:
                raise CutoverError(f"invalid backup entry: {path}")
            relative = entry.get("path")
            if not isinstance(relative, str):
                raise CutoverError(f"invalid backup entry path: {path}")
            pure = PurePosixPath(relative)
            if pure.is_absolute() or ".." in pure.parts:
                raise CutoverError(f"unsafe backup entry path: {relative}")
            if entry["type"] == "file":
                digest = entry.get("sha256")
                target = objects / str(digest)
                if not isinstance(digest, str) or not target.is_file():
                    raise CutoverError(f"missing backup object: {digest}")
                if sha256(target.read_bytes()) != digest:
                    raise CutoverError(f"corrupt backup object: {digest}")


def load_recovery(
    install_record_path: Path,
    install_backup_dir: Path,
) -> tuple[dict, list[dict], Path, str]:
    install_record, install_record_data = load_json(install_record_path, "install record")
    if install_record.get("status") != "success":
        raise CutoverError("install record is not a successful recoverable transaction")
    recovery, recovery_data = load_json(
        install_backup_dir / RECOVERY_MANIFEST, "recovery manifest"
    )
    if recovery.get("schema_version") != 1 or recovery.get("status") != "recoverable":
        raise CutoverError("unsupported recovery manifest")
    if install_record.get("recovery_manifest_sha256") != sha256(recovery_data):
        raise CutoverError("install record does not match the recovery manifest")
    inventory, inventory_data = load_json(
        install_backup_dir / "inventory.json", "backup inventory"
    )
    if recovery.get("backup_inventory_sha256") != sha256(inventory_data):
        raise CutoverError("recovery manifest does not match the backup inventory")
    values = inventory.get("managed_paths")
    if inventory.get("schema_version") != 1 or not isinstance(values, list) or not values:
        raise CutoverError("unsupported backup inventory")
    objects = install_backup_dir / "objects"
    validate_backup_objects(values, objects)
    expected_paths = [
        (item.get("kind"), item.get("path"), item.get("remove_after_success"))
        for item in values
    ]
    recovery_paths = [
        (item.get("kind"), item.get("path"), item.get("remove_after_success"))
        for item in recovery.get("managed_paths", [])
    ]
    if expected_paths != recovery_paths:
        raise CutoverError("recovery manifest managed paths do not match the backup")
    installed_state = recovery.get("installed_state")
    if not isinstance(installed_state, list) or not installed_state:
        raise CutoverError("recovery manifest lacks installed state")
    if recovery.get("marketplace_action") not in MARKETPLACE_ACTIONS:
        raise CutoverError("recovery manifest has an invalid marketplace action")
    if recovery.get("plugin_action") not in PLUGIN_ACTIONS:
        raise CutoverError("recovery manifest has an invalid plugin action")
    for field in ("remove_plugin_on_uninstall", "remove_marketplace_on_uninstall"):
        if not isinstance(recovery.get(field), bool):
            raise CutoverError(f"recovery manifest lacks {field}")
    recovery_gates = recovery.get("recovery_gates")
    if not isinstance(recovery_gates, list):
        raise CutoverError("recovery manifest lacks recovery gates")
    return recovery, values, objects, sha256(install_record_data)


def current_recovery_inventory(recovery: dict) -> list[dict]:
    return inventory({"managed_paths": recovery["managed_paths"]})


def verify_installed_state(recovery: dict, current: list[dict]) -> None:
    expected = {
        (item["kind"], item["path"]): item["content_sha256"]
        for item in recovery["installed_state"]
    }
    actual = {
        (item["kind"], item["path"]): item["content_sha256"] for item in current
    }
    changed = [path for key, path in expected if actual.get((key, path)) != expected[(key, path)]]
    if changed:
        raise CutoverError(
            "managed state changed since installation; refusing destructive recovery: "
            + ", ".join(changed)
        )


def run_cutover(
    document: dict,
    plan_bytes: bytes,
    plugin_root: Path,
    backup_dir: Path,
    record_path: Path,
    runner: Callable[[list[str], dict[str, str]], CommandResult] = subprocess_runner,
) -> dict:
    values = inventory(document)
    ensure_external_paths(document["managed_paths"], backup_dir, record_path)
    objects, inventory_data = create_backup(values, backup_dir)

    environment = dict(os.environ)
    environment["CODEX_HOME"] = document["codex_home"]
    commands: list[dict] = []
    record = {
        "schema_version": 1,
        "status": "running",
        "plan_sha256": sha256(plan_bytes),
        "plugin_root": str(plugin_root),
        "marketplace_source": document["marketplace_source"],
        "marketplace_name": document["marketplace_name"],
        "install_ref": document["install_ref"],
        "backup_inventory_sha256": sha256(inventory_data),
        "commands": commands,
        "removed_paths": [],
        "rollback_verified": False,
    }
    try:
        candidate = Path(document["candidate_config"])
        active = Path(document["active_config"])
        active_mode = (active.stat().st_mode & 0o777) if active.is_file() else 0o600
        marketplace_command = marketplace_install_command(document)
        if marketplace_command is not None:
            commands.append(
                run_command(
                    f"marketplace_{document['marketplace_action']}",
                    marketplace_command,
                    environment,
                    runner,
                )
            )
        plugin_command = plugin_install_command(document)
        if plugin_command is not None:
            commands.append(run_command("plugin_add", plugin_command, environment, runner))
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
        installed_state = compact_inventory(inventory(document))
        recovery = {
            "schema_version": 1,
            "status": "recoverable",
            "plan_sha256": sha256(plan_bytes),
            "plugin_root": str(plugin_root),
            "backup_inventory_sha256": sha256(inventory_data),
            "codex_executable": document["codex_executable"],
            "codex_home": document["codex_home"],
            "install_ref": document["install_ref"],
            "marketplace_source": document["marketplace_source"],
            "marketplace_ref": document.get("marketplace_ref"),
            "marketplace_name": document["marketplace_name"],
            "marketplace_action": document["marketplace_action"],
            "plugin_action": document["plugin_action"],
            "remove_plugin_on_uninstall": document["remove_plugin_on_uninstall"],
            "remove_marketplace_on_uninstall": document[
                "remove_marketplace_on_uninstall"
            ],
            "managed_paths": inventory_document(values)["managed_paths"],
            "installed_state": installed_state,
            "recovery_gates": document["recovery_gates"],
        }
        recovery_data = canonical(recovery)
        atomic_write(backup_dir / RECOVERY_MANIFEST, recovery_data)
        record["recovery_manifest_sha256"] = sha256(recovery_data)
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
        (backup_dir / RECOVERY_MANIFEST).unlink(missing_ok=True)
        record["status"] = "failed"
        record["failure"] = str(error)
        if rollback_error is not None:
            record["rollback_failure"] = str(rollback_error)
        atomic_write(record_path, canonical(record))
        if rollback_error is not None:
            raise CutoverError(f"cutover failed and rollback failed: {rollback_error}") from error
        raise CutoverError(f"cutover failed; rollback verified: {error}") from error


def run_uninstall(
    install_record_path: Path,
    install_backup_dir: Path,
    rollback_backup_dir: Path,
    record_path: Path,
    runner: Callable[[list[str], dict[str, str]], CommandResult] = subprocess_runner,
) -> dict:
    recovery, previous, previous_objects, install_record_sha256 = load_recovery(
        install_record_path, install_backup_dir
    )
    ensure_external_paths(
        recovery["managed_paths"],
        install_backup_dir,
        install_record_path,
        rollback_backup_dir,
        record_path,
    )
    current = current_recovery_inventory(recovery)
    verify_installed_state(recovery, current)
    rollback_objects, rollback_inventory_data = create_backup(current, rollback_backup_dir)

    environment = dict(os.environ)
    environment["CODEX_HOME"] = recovery["codex_home"]
    commands: list[dict] = []
    record = {
        "schema_version": 1,
        "status": "running",
        "install_record_sha256": install_record_sha256,
        "install_recovery_manifest_sha256": sha256(
            (install_backup_dir / RECOVERY_MANIFEST).read_bytes()
        ),
        "rollback_inventory_sha256": sha256(rollback_inventory_data),
        "commands": commands,
        "rollback_verified": False,
    }
    try:
        executable = recovery["codex_executable"]
        if (
            recovery["remove_plugin_on_uninstall"]
            and recovery["plugin_action"] == "add"
        ):
            commands.append(
                run_command(
                    "plugin_remove",
                    [executable, "plugin", "remove", recovery["install_ref"], "--json"],
                    environment,
                    runner,
                )
            )
        if (
            recovery["remove_marketplace_on_uninstall"]
            and recovery["marketplace_action"] == "add"
        ):
            commands.append(
                run_command(
                    "marketplace_remove",
                    [
                        executable,
                        "plugin",
                        "marketplace",
                        "remove",
                        recovery["marketplace_name"],
                        "--json",
                    ],
                    environment,
                    runner,
                )
            )
        for item in previous:
            restore_capture(Path(item["path"]), item["entries"], previous_objects)
        verify_inventory(previous)
        if (
            recovery["remove_plugin_on_uninstall"]
            and recovery["plugin_action"] != "add"
        ):
            commands.append(
                run_command(
                    "plugin_remove_after_restore",
                    [executable, "plugin", "remove", recovery["install_ref"], "--json"],
                    environment,
                    runner,
                )
            )
        if (
            recovery["remove_marketplace_on_uninstall"]
            and recovery["marketplace_action"] != "add"
        ):
            commands.append(
                run_command(
                    "marketplace_remove_after_restore",
                    [
                        executable,
                        "plugin",
                        "marketplace",
                        "remove",
                        recovery["marketplace_name"],
                        "--json",
                    ],
                    environment,
                    runner,
                )
            )
        for gate in recovery["recovery_gates"]:
            commands.append(
                run_command(
                    f"recovery_{gate['kind']}",
                    gate["command"],
                    environment,
                    runner,
                    gate.get("expected", []),
                )
            )
        record["status"] = "success"
        atomic_write(record_path, canonical(record))
        return record
    except BaseException as error:
        rollback_error: BaseException | None = None
        try:
            for item in current:
                restore_capture(Path(item["path"]), item["entries"], rollback_objects)
            verify_inventory(current)
            record["rollback_verified"] = True
        except BaseException as caught:
            rollback_error = caught
        record["status"] = "failed"
        record["failure"] = str(error)
        if rollback_error is not None:
            record["rollback_failure"] = str(rollback_error)
        atomic_write(record_path, canonical(record))
        if rollback_error is not None:
            raise CutoverError(f"uninstall failed and rollback failed: {rollback_error}") from error
        raise CutoverError(f"uninstall failed; rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    install = commands.add_parser("install", help="install or reconfigure transactionally")
    install.add_argument("--plan", type=Path, required=True)
    install.add_argument("--plugin-root", type=Path, required=True)
    install.add_argument("--backup-dir", type=Path, required=True)
    install.add_argument("--record", type=Path, required=True)
    install.add_argument("--apply", action="store_true")
    uninstall = commands.add_parser("uninstall", help="restore a successful install backup")
    uninstall.add_argument("--install-record", type=Path, required=True)
    uninstall.add_argument("--install-backup-dir", type=Path, required=True)
    uninstall.add_argument("--rollback-backup-dir", type=Path, required=True)
    uninstall.add_argument("--record", type=Path, required=True)
    uninstall.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()
    try:
        if arguments.operation == "install":
            plan, plan_bytes = load_plan(arguments.plan.resolve(strict=True))
            plugin_root = arguments.plugin_root.resolve(strict=True)
            errors = validate_plan(plan, plugin_root)
            if errors:
                raise CutoverError("invalid cutover plan:\n- " + "\n- ".join(errors))
            preview = inventory(plan)
            print(f"plugin_root: {plugin_root}")
            print(f"marketplace_source: {plan['marketplace_source']}")
            print(f"marketplace_action: {plan['marketplace_action']}")
            print(f"plugin_action: {plan['plugin_action']}")
            print(
                "uninstall removal: "
                f"plugin={plan['remove_plugin_on_uninstall']} "
                f"marketplace={plan['remove_marketplace_on_uninstall']}"
            )
            print(f"plan_sha256: {sha256(plan_bytes)}")
            for item in preview:
                print(
                    f"{item['kind']}: {item['path']} tree_sha256={item['tree_sha256']} "
                    f"remove_after_success={item['remove_after_success']}"
                )
            marketplace_command = marketplace_install_command(plan)
            if marketplace_command is not None:
                print(f"marketplace command: {json.dumps(marketplace_command)}")
            plugin_command = plugin_install_command(plan)
            if plugin_command is not None:
                print(f"plugin command: {json.dumps(plugin_command)}")
            for gate in plan["gates"]:
                print(f"gate {gate['kind']}: {json.dumps(gate['command'])}")
            for gate in plan["recovery_gates"]:
                print(f"recovery gate {gate['kind']}: {json.dumps(gate['command'])}")
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
            print("Codex install succeeded; the selected backup is recoverable by uninstall")
            return 0

        install_record = arguments.install_record.expanduser().resolve(strict=True)
        install_backup = arguments.install_backup_dir.expanduser().resolve(strict=True)
        recovery, _previous, _objects, _record_sha = load_recovery(
            install_record, install_backup
        )
        current = current_recovery_inventory(recovery)
        verify_installed_state(recovery, current)
        print(f"install_record: {install_record}")
        print(f"install_backup_dir: {install_backup}")
        for item in current:
            print(
                f"restore {item['kind']}: {item['path']} "
                f"current_content_sha256={item['content_sha256']}"
            )
        if recovery["remove_plugin_on_uninstall"]:
            print(
                f"remove plugin: {recovery['install_ref']} "
                f"({'before' if recovery['plugin_action'] == 'add' else 'after'} restore)"
            )
        if recovery["remove_marketplace_on_uninstall"]:
            print(
                f"remove marketplace: {recovery['marketplace_name']} "
                f"({'before' if recovery['marketplace_action'] == 'add' else 'after'} restore)"
            )
        for gate in recovery["recovery_gates"]:
            print(f"recovery gate {gate['kind']}: {json.dumps(gate['command'])}")
        if not arguments.apply:
            print("preview only; no plugin removal or backup restoration ran")
            return 0
        run_uninstall(
            install_record,
            install_backup,
            arguments.rollback_backup_dir.expanduser().absolute(),
            arguments.record.expanduser().absolute(),
        )
        print("Codex uninstall succeeded; pre-install state was restored and verified")
        return 0
    except (OSError, CutoverError, UnicodeError, tomllib.TOMLDecodeError) as error:
        print(f"Codex cutover refused or failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
