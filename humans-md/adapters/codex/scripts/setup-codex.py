#!/usr/bin/env python3
"""Preview, install, or remove the humans-md standing contract for Codex."""
from __future__ import annotations

import argparse
import datetime
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path, PurePosixPath

PLUGIN_ID = "humans-md@humans-md"
MARKETPLACE = "humans-md"
RECEIPT_SCHEMA = 4
MANAGED = ("AGENTS.md",)


class SetupError(RuntimeError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("ascii")


def atomic_write(path: Path, data: bytes, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            fchmod = getattr(os, "fchmod", None)
            if fchmod is not None:
                fchmod(stream.fileno(), mode)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        if os.name == "posix":
            os.chmod(temporary_path, mode)
        os.replace(temporary_path, path)
    except BaseException:
        try:
            os.close(descriptor)
        except OSError:
            pass
        temporary_path.unlink(missing_ok=True)
        raise


def remove(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink(missing_ok=True)
    elif path.is_dir():
        shutil.rmtree(path)


def copy_path(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_symlink():
        target.symlink_to(os.readlink(source))
    elif source.is_dir():
        shutil.copytree(source, target, symlinks=True, copy_function=shutil.copy2)
    else:
        shutil.copy2(source, target)


def same_path(first: Path, second: Path) -> bool:
    if first.is_symlink() or second.is_symlink():
        return first.is_symlink() and second.is_symlink() and os.readlink(first) == os.readlink(second)
    if first.is_file() or second.is_file():
        return first.is_file() and second.is_file() and first.read_bytes() == second.read_bytes()
    return not first.exists() and not second.exists()


def snapshot(home: Path, paths: list[Path], destination: Path) -> list[dict]:
    destination.mkdir(parents=True, exist_ok=True)
    os.chmod(destination, 0o700)
    entries = []
    for path in paths:
        relative = path.relative_to(home)
        existed = path.exists() or path.is_symlink()
        entries.append({"path": relative.as_posix(), "existed": existed})
        if existed:
            copy_path(path, destination / relative)
    return entries


def safe_inventory(home: Path, entries: object, expected: tuple[str, ...]) -> list[dict]:
    if not isinstance(entries, list) or len(entries) != len(expected):
        raise SetupError("receipt backup inventory is invalid")
    result: list[dict] = []
    for entry, relative in zip(entries, expected, strict=True):
        if not isinstance(entry, dict) or entry.get("path") != relative or not isinstance(entry.get("existed"), bool):
            raise SetupError("unsafe receipt path")
        result.append(entry)
    return result


def restore(home: Path, source: Path, entries: list[dict]) -> None:
    for entry in entries:
        relative = Path(*PurePosixPath(entry["path"]).parts)
        target = home / relative
        remove(target)
        if entry["existed"]:
            copy_path(source / relative, target)
        if entry["existed"] != (target.exists() or target.is_symlink()) or (entry["existed"] and not same_path(source / relative, target)):
            raise SetupError(f"restore verification failed: {target}")


def pointer(home: Path) -> Path:
    return home / "state/humans-md/current.json"


def backup_root(home: Path) -> Path:
    return home / "backups/humans-md"


def checked_json(args: list[str], environment: dict[str, str]) -> dict:
    result = subprocess.run(args, env=environment, capture_output=True, text=True, encoding="utf-8", errors="strict")
    if result.returncode:
        raise SetupError(result.stderr.strip() or result.stdout.strip() or "Codex command failed")
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SetupError("Codex command returned invalid JSON") from error
    if not isinstance(value, dict):
        raise SetupError("Codex command returned a non-object")
    return value


def plugin_root(path: Path) -> tuple[Path, dict]:
    root = path.expanduser().resolve(strict=True)
    try:
        manifest = json.loads((root / ".codex-plugin/plugin.json").read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid installed plugin: {error}") from error
    if manifest.get("name") != "humans-md" or not isinstance(manifest.get("version"), str):
        raise SetupError("installed plugin identity is not humans-md")
    if not (root / "templates/AGENTS.md").is_file():
        raise SetupError("installed plugin lacks templates/AGENTS.md")
    return root, manifest


def prepare(root: Path, home: Path, executable: str) -> dict:
    root, manifest = plugin_root(root)
    environment = {**os.environ, "CODEX_HOME": str(home)}
    plugins = checked_json([executable, "plugin", "list", "--json"], environment)
    installed = next((item for item in plugins.get("installed", []) if item.get("pluginId") == PLUGIN_ID), None)
    if not isinstance(installed, dict) or not installed.get("installed") or not installed.get("enabled"):
        raise SetupError("humans-md must be installed and enabled before setup")
    if installed.get("version") != manifest["version"]:
        raise SetupError("installed plugin version differs from package")
    markets = checked_json([executable, "plugin", "marketplace", "list", "--json"], environment)
    if not any(item.get("name") == MARKETPLACE for item in markets.get("marketplaces", [])):
        raise SetupError("humans-md marketplace is not configured")
    contract = (root / "templates/AGENTS.md").read_bytes()
    contract.decode("ascii")
    return {"root": root, "home": home, "executable": executable, "environment": environment, "version": manifest["version"], "contract": contract}


def preview(plan: dict) -> dict:
    return {"operation": "install", "plugin_version": plan["version"], "contract": str(plan["home"] / "AGENTS.md"), "receipt_root": str(backup_root(plan["home"])), "contract_only": True, "restart_required": True}


def install(plan: dict) -> dict:
    plan = prepare(plan["root"], plan["home"], plan["executable"])
    home = plan["home"]
    if pointer(home).exists():
        raise SetupError("an active humans-md receipt already exists; uninstall it before reinstalling")
    backup_root(home).mkdir(parents=True, exist_ok=True)
    receipt_dir = Path(tempfile.mkdtemp(prefix=datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-"), dir=backup_root(home)))
    os.chmod(receipt_dir, 0o700)
    paths = [home / item for item in MANAGED]
    before = snapshot(home, paths, receipt_dir / "before")
    try:
        atomic_write(paths[0], plan["contract"])
        if paths[0].read_bytes() != plan["contract"]:
            raise SetupError("written contract differs from preview")
        receipt = {"schema_version": RECEIPT_SCHEMA, "status": "installed", "plugin_version": plan["version"], "before": before, "remove_plugin": True, "remove_marketplace": False}
        receipt_path = receipt_dir / "receipt.json"
        atomic_write(receipt_path, canonical(receipt))
        pointer(home).parent.mkdir(parents=True, exist_ok=True)
        atomic_write(pointer(home), canonical({"receipt": str(receipt_path)}))
        return {"status": "installed", "receipt": str(receipt_path), "restart_required": True}
    except BaseException as error:
        restore(home, receipt_dir / "before", before)
        pointer(home).unlink(missing_ok=True)
        atomic_write(receipt_dir / "failure.json", canonical({"status": "failed", "error": str(error), "rollback_verified": True}))
        raise SetupError(f"setup failed; rollback verified: {error}") from error


def receipt(home: Path) -> tuple[Path, dict]:
    try:
        selected = json.loads(pointer(home).read_bytes())
        path = Path(selected["receipt"]).resolve(strict=True)
        value = json.loads(path.read_bytes())
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid receipt: {error}") from error
    if backup_root(home).resolve() not in path.parents or path.name != "receipt.json":
        raise SetupError("receipt is outside the durable backup root")
    if value.get("schema_version") != RECEIPT_SCHEMA or value.get("status") != "installed":
        raise SetupError("receipt is not a current humans-md receipt")
    safe_inventory(home, value.get("before"), MANAGED)
    if value.get("remove_plugin") is not True or value.get("remove_marketplace") is not False:
        raise SetupError("receipt removal policy is invalid")
    return path, value


def show_uninstall_diffs(home: Path, path: Path, value: dict) -> None:
    with tempfile.TemporaryDirectory(prefix="humans-md-uninstall-") as temporary:
        missing = Path(temporary) / "missing"
        missing.touch()
        for entry in safe_inventory(home, value["before"], MANAGED):
            target = home / entry["path"]
            baseline = path.parent / "before" / entry["path"] if entry["existed"] else missing
            existing = target if target.exists() or target.is_symlink() else missing
            result = subprocess.run(["git", "diff", "--no-index", "--", str(existing), str(baseline)], capture_output=True, text=True, encoding="utf-8", errors="strict")
            if result.returncode not in (0, 1):
                raise SetupError(result.stderr.strip() or "git diff --no-index failed")
            if result.stdout:
                print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")


def uninstall(home: Path, executable: str, path: Path, value: dict) -> dict:
    paths = [home / item for item in MANAGED]
    rollback_dir = backup_root(home) / ("uninstall-" + datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ"))
    rollback = snapshot(home, paths + [pointer(home)], rollback_dir / "before")
    try:
        restore(home, path.parent / "before", safe_inventory(home, value["before"], MANAGED))
        environment = {**os.environ, "CODEX_HOME": str(home)}
        checked_json([executable, "plugin", "remove", PLUGIN_ID, "--json"], environment)
        pointer(home).unlink(missing_ok=True)
        atomic_write(rollback_dir / "receipt.json", canonical({"status": "uninstalled", "install_receipt": str(path)}))
        return {"status": "uninstalled", "install_receipt": str(path), "marketplace_preserved": True}
    except BaseException as error:
        restore(home, rollback_dir / "before", rollback)
        atomic_write(rollback_dir / "failure.json", canonical({"status": "failed", "error": str(error), "rollback_verified": True}))
        raise SetupError(f"uninstall failed; rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    for name in ("install", "uninstall"):
        command = commands.add_parser(name)
        command.add_argument("--codex-home", type=Path, default=Path(os.environ.get("CODEX_HOME", "~/.codex")))
        command.add_argument("--codex-executable", default=shutil.which("codex"))
        command.add_argument("--apply", action="store_true")
        if name == "install":
            command.add_argument("--plugin-root", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        home = arguments.codex_home.expanduser().resolve(strict=True)
        if not arguments.codex_executable:
            raise SetupError("Codex executable was not found")
        if arguments.operation == "install":
            plan = prepare(arguments.plugin_root, home, arguments.codex_executable)
            print(json.dumps(preview(plan), indent=2, sort_keys=True))
            if arguments.apply:
                print(json.dumps(install(plan), indent=2, sort_keys=True))
        else:
            path, value = receipt(home)
            print(json.dumps({"operation": "uninstall", "receipt": str(path), "remove_plugin": True, "remove_marketplace": False, "review": "git diffs for managed files follow"}, indent=2, sort_keys=True))
            show_uninstall_diffs(home, path, value)
            if arguments.apply:
                print(json.dumps(uninstall(home, arguments.codex_executable, path, value), indent=2, sort_keys=True))
        if not arguments.apply:
            print("preview only; no files changed")
        return 0
    except (OSError, UnicodeError, ValueError, SetupError) as error:
        print(f"{arguments.operation} failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
