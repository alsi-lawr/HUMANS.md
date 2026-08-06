#!/usr/bin/env python3
"""Preview, bind, or uninstall the packaged Casefile MCP runtime for Claude Code."""
from __future__ import annotations

import argparse
import datetime
import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path, PurePosixPath

try:
    import casefile_runtime
except ModuleNotFoundError:
    _runtime_path = Path(__file__).resolve().parents[2] / "shared/casefile_runtime.py"
    _runtime_spec = importlib.util.spec_from_file_location("casefile_runtime", _runtime_path)
    if _runtime_spec is None or _runtime_spec.loader is None:
        raise
    casefile_runtime = importlib.util.module_from_spec(_runtime_spec)
    _runtime_spec.loader.exec_module(casefile_runtime)


RECEIPT_SCHEMA = 1
DEPTH_KEY = "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH"
SERVER = "casefile"


class SetupError(RuntimeError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def checked(arguments: list[str], environment: dict[str, str]) -> str:
    result = subprocess.run(arguments, env=environment, capture_output=True, text=True)
    if result.returncode:
        raise SetupError(result.stderr.strip() or result.stdout.strip() or "Claude command failed")
    return result.stdout


def pointer(home: Path) -> Path:
    return home / "casefile/state/current.json"


def user_config(home: Path) -> Path:
    return home / ".claude.json"


def settings_file(home: Path) -> Path:
    return home / "settings.json"


def spawn_depth(root: Path) -> int:
    """Deepest nesting any shipped matrix declares."""
    depths = []
    for path in sorted((root / "matrices").glob("*.toml")):
        matrix = tomllib.loads(path.read_text(encoding="ascii"))
        depth = matrix.get("limits", {}).get("max_depth")
        if not isinstance(depth, int) or depth < 0:
            raise SetupError(f"matrix {path.name} declares no usable max_depth")
        depths.append(depth)
    if not depths:
        raise SetupError("plugin ships no strategy matrices")
    return max(depths)


def read_settings(home: Path) -> dict:
    path = settings_file(home)
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise SetupError("Claude settings.json is unsafe")
    if not path.exists():
        return {}
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (ValueError, UnicodeDecodeError) as error:
        raise SetupError(f"Claude settings.json is not valid JSON: {error}") from error
    if not isinstance(document, dict):
        raise SetupError("Claude settings.json must be a JSON object")
    return document


def write_spawn_depth(home: Path, depth: int | None) -> object:
    """Set or clear the depth ceiling. Returns the prior value, or None when absent."""
    document = read_settings(home)
    environment = document.get("env")
    if not isinstance(environment, dict):
        environment = {}
        document["env"] = environment
    prior = environment.get(DEPTH_KEY)
    if depth is None:
        environment.pop(DEPTH_KEY, None)
        if not environment:
            document.pop("env", None)
    else:
        environment[DEPTH_KEY] = str(depth)
    atomic_write(
        settings_file(home),
        (json.dumps(document, indent=2, ensure_ascii=True) + "\n").encode("ascii"),
    )
    return prior


def plugin(root: Path) -> tuple[Path, dict]:
    root = root.expanduser().resolve(strict=True)
    try:
        manifest = json.loads((root / ".claude-plugin/plugin.json").read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid Casefile plugin: {error}") from error
    if manifest.get("name") != "casefile" or not isinstance(manifest.get("version"), str):
        raise SetupError("installed plugin identity is not casefile")
    return root, manifest


def current_binding(executable: str, environment: dict[str, str]) -> str | None:
    result = subprocess.run(
        [executable, "mcp", "get", SERVER], env=environment, capture_output=True, text=True
    )
    if result.returncode:
        return None
    return result.stdout + result.stderr


def read_receipt(home: Path) -> tuple[Path, dict] | None:
    if not pointer(home).is_file():
        return None
    try:
        selection = json.loads(pointer(home).read_text(encoding="ascii"))
        path = Path(selection["receipt"]).resolve(strict=True)
        value = json.loads(path.read_text(encoding="ascii"))
    except (OSError, KeyError, UnicodeError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid Claude Casefile receipt: {error}") from error
    receipts = (home / "casefile/receipts").resolve()
    if receipts not in path.parents or value.get("schema_version") != RECEIPT_SCHEMA:
        raise SetupError("unsafe or unsupported Claude Casefile receipt")
    if value.get("status") != "installed" or not isinstance(value.get("owned_binaries"), list):
        raise SetupError("invalid Claude Casefile receipt state")
    if not isinstance(value.get("binary"), str) or not isinstance(value.get("planning_root"), str):
        raise SetupError("invalid Claude Casefile receipt binding")
    try:
        Path(value["binary"]).relative_to(home)
    except ValueError as error:
        raise SetupError("Claude Casefile receipt binary is outside its config directory") from error
    if not Path(value["planning_root"]).is_absolute():
        raise SetupError("Claude Casefile receipt planning root is not absolute")
    return path, value


def prepare(
    root: Path, home: Path, executable: str, planning: Path, overwrite: bool = False
) -> dict:
    root, manifest = plugin(root)
    planning = casefile_runtime.planning_root(planning)
    selected = casefile_runtime.select(root, manifest["version"])
    destination = casefile_runtime.destination(home, manifest["version"], selected["target"])
    if destination.is_symlink() or (destination.exists() and not overwrite):
        raise SetupError("the versioned Casefile executable path is already occupied")
    casefile_runtime.probe(selected["source"], manifest["version"], planning)
    environment = {**os.environ, "CLAUDE_CONFIG_DIR": str(home)}
    active = read_receipt(home)
    binding = current_binding(executable, environment)
    if active is None and binding is not None:
        raise SetupError("an unowned Claude MCP server named casefile already exists")
    if active is not None:
        _, previous = active
        if previous.get("plugin_version") == manifest["version"] and not overwrite:
            raise SetupError("this Casefile version is already installed")
        if binding is None or previous.get("binary") not in binding or previous.get("planning_root") not in binding:
            raise SetupError("the existing Casefile binding differs from its receipt")
    return {
        "root": root, "home": home, "executable": executable, "environment": environment,
        "version": manifest["version"], "planning_root": planning, "selected": selected,
        "binary": destination, "previous": active, "overwrite": overwrite,
        "spawn_depth": spawn_depth(root),
    }


def preview(plan: dict) -> dict:
    return {
        "operation": "install", "plugin_version": plan["version"],
        "casefile_target": plan["selected"]["target"], "casefile_executable": str(plan["binary"]),
        "planning_root": str(plan["planning_root"]), "mcp_server": SERVER,
        "scope": "user", "runtime_prerequisites": [], "apply_required": True,
        "subagent_spawn_depth": plan["spawn_depth"],
        "settings_target": str(settings_file(plan["home"])),
    }


def register(plan: dict) -> None:
    checked([
        plan["executable"], "mcp", "add", "--scope", "user", SERVER, "--",
        str(plan["binary"]), "mcp-package", "--planning-root", str(plan["planning_root"]),
    ], plan["environment"])
    binding = current_binding(plan["executable"], plan["environment"])
    if binding is None or str(plan["binary"]) not in binding or str(plan["planning_root"]) not in binding:
        raise SetupError("Claude did not retain the exact Casefile MCP binding")


def install(plan: dict) -> dict:
    receipt_dir = plan["home"] / "casefile/receipts" / datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-%f")
    receipt_dir.mkdir(parents=True)
    previous = plan["previous"]
    previous_value = previous[1] if previous is not None else None
    binaries = list(previous_value.get("owned_binaries", [])) if previous_value else []
    copied = False
    registration_started = False
    depth_applied = False
    depth_before = None
    config = user_config(plan["home"])
    if config.is_symlink() or (config.exists() and not config.is_file()):
        raise SetupError("Claude user configuration is unsafe")
    config_before = config.read_bytes() if config.is_file() else None
    pointer_before = pointer(plan["home"]).read_bytes() if pointer(plan["home"]).is_file() else None
    try:
        casefile_runtime.atomic_copy(plan["selected"]["source"], plan["binary"])
        copied = True
        if casefile_runtime.sha256(plan["binary"]) != plan["selected"]["sha256"]:
            raise SetupError("installed Casefile executable hash mismatch")
        casefile_runtime.probe(plan["binary"], plan["version"], plan["planning_root"])
        registration_started = True
        register(plan)
        depth_before = write_spawn_depth(plan["home"], plan["spawn_depth"])
        depth_applied = True
        if previous_value is not None:
            depth_before = previous_value.get("subagent_spawn_depth_before")
        relative = plan["binary"].relative_to(plan["home"]).as_posix()
        if relative not in binaries:
            binaries.append(relative)
        receipt = {
            "schema_version": RECEIPT_SCHEMA, "status": "installed",
            "plugin_version": plan["version"], "binary": str(plan["binary"]),
            "planning_root": str(plan["planning_root"]), "artifact_sha256": plan["selected"]["sha256"],
            "owned_binaries": binaries,
            "subagent_spawn_depth": plan["spawn_depth"],
            "subagent_spawn_depth_before": depth_before,
        }
        receipt_path = receipt_dir / "receipt.json"
        atomic_write(receipt_path, canonical(receipt))
        atomic_write(pointer(plan["home"]), canonical({"receipt": str(receipt_path)}))
        return {"status": "installed", "receipt": str(receipt_path), "restart_required": True}
    except BaseException as error:
        rollback_error = None
        try:
            if depth_applied:
                write_spawn_depth(plan["home"], depth_before)
            if registration_started:
                if config_before is None:
                    config.unlink(missing_ok=True)
                else:
                    atomic_write(config, config_before)
            binding = current_binding(plan["executable"], plan["environment"])
            if previous_value is None:
                if binding is not None:
                    raise SetupError("fresh-install binding remains after configuration restore")
            elif (
                binding is None
                or previous_value["binary"] not in binding
                or previous_value["planning_root"] not in binding
            ):
                raise SetupError("previous binding was not restored")
        except BaseException as rollback_failure:
            rollback_error = rollback_failure
        if rollback_error is None and copied:
            plan["binary"].unlink(missing_ok=True)
        pointer_after = pointer(plan["home"])
        pointer_matches = (
            not pointer_after.exists()
            if pointer_before is None
            else pointer_after.is_file() and pointer_after.read_bytes() == pointer_before
        )
        rollback_verified = rollback_error is None and pointer_matches
        atomic_write(receipt_dir / "failure.json", canonical({
            "status": "failed", "error": str(error),
            "rollback_verified": rollback_verified,
            "rollback_error": None if rollback_error is None else str(rollback_error),
            "binding_present": current_binding(plan["executable"], plan["environment"]) is not None,
            "binary_present": plan["binary"].exists(),
            "pointer_present": pointer(plan["home"]).exists(),
        }))
        if not rollback_verified:
            raise SetupError(
                f"Claude setup failed and rollback is unverified: {error}; {rollback_error}"
            ) from error
        raise SetupError(f"Claude setup failed; rollback verified: {error}") from error


def uninstall(home: Path, executable: str, apply: bool) -> dict:
    selected = read_receipt(home)
    if selected is None:
        raise SetupError("no active Claude Casefile receipt exists")
    path, value = selected
    environment = {**os.environ, "CLAUDE_CONFIG_DIR": str(home)}
    binding = current_binding(executable, environment)
    if binding is None or value["binary"] not in binding or value["planning_root"] not in binding:
        raise SetupError("Claude Casefile binding changed after setup")
    owned_paths = []
    for relative in value.get("owned_binaries", []):
        if not isinstance(relative, str):
            raise SetupError("invalid owned binary inventory")
        pure = PurePosixPath(relative)
        if pure.is_absolute() or ".." in pure.parts or pure.parts[:2] != ("casefile", "runtime"):
            raise SetupError("unsafe owned binary path")
        binary = home / Path(*pure.parts)
        if binary.is_symlink() or not binary.is_file():
            raise SetupError("receipt-owned Casefile binary is missing or unsafe")
        owned_paths.append(binary)
    result = {"operation":"uninstall","receipt":str(path),"owned_binaries":value["owned_binaries"],"preserve_unrelated_state":True}
    print(json.dumps(result, indent=2, sort_keys=True))
    if not apply:
        return {"status":"preview"}
    with tempfile.TemporaryDirectory(prefix="casefile-claude-uninstall-", dir=home / "casefile") as temporary:
        backup = Path(temporary)
        for index, binary in enumerate(owned_paths):
            shutil.copyfile(binary, backup / str(index))
        try:
            checked([executable, "mcp", "remove", "--scope", "user", SERVER], environment)
            recorded = value.get("subagent_spawn_depth_before")
            write_spawn_depth(home, int(recorded) if isinstance(recorded, str) else None)
            for binary in owned_paths:
                binary.unlink()
            pointer(home).unlink()
            return {"status":"uninstalled","receipt":str(path)}
        except BaseException as error:
            for index, binary in enumerate(owned_paths):
                casefile_runtime.atomic_copy(backup / str(index), binary)
            rollback = {
                "executable": executable, "environment": environment,
                "binary": Path(value["binary"]), "planning_root": Path(value["planning_root"]),
            }
            register(rollback)
            if not pointer(home).exists():
                atomic_write(pointer(home), canonical({"receipt": str(path)}))
            raise SetupError(f"Claude uninstall failed; rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    install_parser = commands.add_parser("install")
    install_parser.add_argument("--plugin-root", type=Path, required=True)
    install_parser.add_argument("--planning-root", type=Path, required=True)
    install_parser.add_argument("--claude-config-dir", type=Path, default=Path(os.environ.get("CLAUDE_CONFIG_DIR", "~/.claude")))
    install_parser.add_argument("--claude-executable", default=shutil.which("claude"))
    install_parser.add_argument("--apply", action="store_true")
    install_parser.add_argument(
        "--overwrite",
        action="store_true",
        help="reinstall over an existing receipt, preserving its pre-Casefile backup",
    )
    uninstall_parser = commands.add_parser("uninstall")
    uninstall_parser.add_argument("--claude-config-dir", type=Path, default=Path(os.environ.get("CLAUDE_CONFIG_DIR", "~/.claude")))
    uninstall_parser.add_argument("--claude-executable", default=shutil.which("claude"))
    uninstall_parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    try:
        home = args.claude_config_dir.expanduser().resolve(strict=True)
        if not args.claude_executable:
            raise SetupError("Claude executable was not found")
        if args.operation == "install":
            plan = prepare(
                args.plugin_root,
                home,
                args.claude_executable,
                args.planning_root,
                args.overwrite,
            )
            print(json.dumps(preview(plan), indent=2, sort_keys=True))
            if args.apply:
                print(json.dumps(install(plan), indent=2, sort_keys=True))
            else:
                print("preview only; no files changed")
        else:
            print(json.dumps(uninstall(home, args.claude_executable, args.apply), indent=2, sort_keys=True))
        return 0
    except (OSError, UnicodeError, ValueError, SetupError, casefile_runtime.RuntimeError) as error:
        print(f"{args.operation} failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
