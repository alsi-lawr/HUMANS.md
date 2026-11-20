#!/usr/bin/env python3
"""Deterministically preview, install, or uninstall Casefile integration for Codex."""
from __future__ import annotations

import argparse
import datetime
import importlib.util
import json
import os
import re
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

PLUGIN_ID = "casefile@humans-md"
MARKETPLACE = "humans-md"
RECEIPT_SCHEMA = 6
MULTI_AGENT_VERSIONS = {"v1", "v2"}
LEGACY_CATALOGS = {f"models-casefile-{version}.json" for version in MULTI_AGENT_VERSIONS}
CATALOG_KEY = "model_catalog_json"
CATALOG_ASSIGNMENT = re.compile(
    rb'''(?m)^[ \t]*(?:model_catalog_json|"model_catalog_json"|'model_catalog_json')[ \t]*=[^\n]*(?:\n|$)'''
)
MINIMUM_AGENT_THREADS = 6
SCALAR_BEGIN = b"# >>> casefile setup scalars >>>\n"
SCALAR_END = b"# <<< casefile setup scalars <<<\n"
TABLE_BEGIN = b"\n# >>> casefile setup tables >>>\n"
TABLE_END = b"# <<< casefile setup tables <<<\n"
FEATURE_BEGIN = b"# >>> casefile setup feature keys >>>\n"
FEATURE_END = b"# <<< casefile setup feature keys <<<\n"
AGENT_BEGIN = b"# >>> casefile setup agent keys >>>\n"
AGENT_END = b"# <<< casefile setup agent keys <<<\n"
OWNED_KEYS = {
    "": (
        "features.multi_agent",
        "features.multi_agent_v2",
        "agents.max_depth",
    ),
    "features": ("multi_agent", "multi_agent_v2"),
    "agents": ("max_depth",),
}


class SetupError(RuntimeError):
    pass


def canonical(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode(
        "ascii"
    )


def secure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)
    os.chmod(path, 0o700)


def atomic_write(path: Path, data: bytes, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            fchmod = getattr(os, "fchmod", None)
            if fchmod is not None:
                fchmod(stream.fileno(), 0o600)
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


def command(args: list[str], environment: dict[str, str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args, env=environment, capture_output=True, text=True, encoding="utf-8", errors="strict"
    )


def checked(args: list[str], environment: dict[str, str]) -> str:
    result = command(args, environment)
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic output"
        raise SetupError(f"command failed ({result.returncode}): {' '.join(args)}: {detail}")
    return result.stdout


def checked_json(args: list[str], environment: dict[str, str]) -> dict:
    try:
        value = json.loads(checked(args, environment))
    except json.JSONDecodeError as error:
        raise SetupError(f"command returned invalid JSON: {' '.join(args)}") from error
    if not isinstance(value, dict):
        raise SetupError(f"command returned a non-object: {' '.join(args)}")
    return value


def plugin_root(path: Path) -> tuple[Path, dict]:
    root = path.expanduser().resolve(strict=True)
    try:
        manifest = json.loads((root / ".codex-plugin/plugin.json").read_text(encoding="ascii"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid installed plugin: {error}") from error
    if manifest.get("name") != "casefile" or not isinstance(manifest.get("version"), str):
        raise SetupError("installed plugin identity is not casefile")
    for relative in (
        "config/config-fragment.toml.in",
        "config/profiles.toml",
        "runtime/artifacts.json",
        "scripts/casefile_runtime.py",
        "scripts/list-codex-models.py",
        "scripts/offer-role-models.py",
        "scripts/resolve-role-assignment.py",
    ):
        if not (root / relative).is_file():
            raise SetupError(f"installed plugin lacks {relative}")
    return root, manifest


def discover(executable: str, environment: dict[str, str], version: str) -> dict:
    plugins = checked_json([executable, "plugin", "list", "--json"], environment)
    plugin = next(
        (item for item in plugins.get("installed", []) if item.get("pluginId") == PLUGIN_ID),
        None,
    )
    if not isinstance(plugin, dict) or not plugin.get("installed") or not plugin.get("enabled"):
        raise SetupError("casefile must be installed and enabled before setup")
    if plugin.get("version") != version:
        raise SetupError(
            f"installed Casefile version {plugin.get('version')!r} differs from package {version!r}"
        )
    markets = checked_json(
        [executable, "plugin", "marketplace", "list", "--json"], environment
    )
    if not any(item.get("name") == MARKETPLACE for item in markets.get("marketplaces", [])):
        raise SetupError("humans-md marketplace is not configured")
    return plugin


def multi_agent_version(value: str) -> str:
    if value not in MULTI_AGENT_VERSIONS:
        raise SetupError(f"unsupported multi-agent version: {value!r}")
    return value


def marked(begin: bytes, payload: bytes, end: bytes) -> bytes:
    return begin + payload.rstrip(b"\n") + b"\n" + end


def table_header_index(current: bytes, name: str) -> int | None:
    for index, line in enumerate(current.splitlines(keepends=True)):
        try:
            if tomllib.loads(line.decode("utf-8")) == {name: {}}:
                return index
        except (UnicodeError, tomllib.TOMLDecodeError):
            continue
    return None


def insert_after_line(current: bytes, index: int, payload: bytes) -> bytes:
    lines = current.splitlines(keepends=True)
    prefix = b"".join(lines[: index + 1])
    if prefix and not prefix.endswith((b"\n", b"\r")):
        prefix += b"\n"
    return prefix + payload + b"".join(lines[index + 1 :])


def drop_owned_lines(data: bytes, table: str = "") -> bytes:
    output = []
    for line in data.splitlines(keepends=True):
        header = re.fullmatch(rb"\s*\[\[?([^\]\r\n]+)\]\]?\s*(?:#.*)?\r?\n?", line)
        if header:
            table = header.group(1).decode("utf-8").strip()
        assignment = re.match(rb"\s*([A-Za-z0-9_.-]+)\s*=", line)
        if assignment and assignment.group(1).decode("ascii") in OWNED_KEYS.get(table, ()):
            continue
        output.append(line)
    return b"".join(output)


def ensure_thread_capacity(current: bytes) -> bytes:
    document = tomllib.loads(current.decode("utf-8"))
    threads = document.get("agents", {}).get("max_threads")
    if threads is not None:
        if isinstance(threads, bool) or not isinstance(threads, int) or threads < MINIMUM_AGENT_THREADS:
            raise SetupError(
                f"agents.max_threads must be an integer of at least {MINIMUM_AGENT_THREADS}; "
                "change the host configuration before installing Casefile"
            )
        return current
    index = table_header_index(current, "agents")
    if index is not None:
        return insert_after_line(current, index, f"max_threads = {MINIMUM_AGENT_THREADS}\n".encode("ascii"))
    return f"agents.max_threads = {MINIMUM_AGENT_THREADS}\n".encode("ascii") + current


def config_candidate(
    current: bytes,
    root: Path,
    version: str,
    binary: Path,
    planning_root: Path,
) -> bytes:
    if current:
        tomllib.loads(current.decode("utf-8"))
    version = multi_agent_version(version)
    fragment = (root / "config/config-fragment.toml.in").read_text(encoding="ascii")
    fragment = fragment.replace("__HUMANS_MD_PLUGIN_ROOT__", str(root).replace("\\", "/"))
    fragment = fragment.replace("__CASEFILE_MULTI_AGENT_V1__", "true" if version == "v1" else "false")
    fragment = fragment.replace("__CASEFILE_MULTI_AGENT_V2__", "false" if version == "v1" else "true")
    fragment = fragment.replace("__CASEFILE_EXECUTABLE__", json.dumps(str(binary)))
    fragment = fragment.replace("__CASEFILE_PLANNING_ROOT__", json.dumps(str(planning_root)))
    owned = tomllib.loads(fragment)
    # Installing authorises setup to set the exact keys it owns; prior values give way.
    current = drop_owned_lines(remove_owned_tables(current))
    current = ensure_thread_capacity(current)

    index = table_header_index(current, "features")
    if index is not None:
        payload = "".join(
            f"{name} = {'true' if value else 'false'}\n"
            for name, value in owned["features"].items()
        )
        current = insert_after_line(
            current, index, marked(FEATURE_BEGIN, payload.encode("ascii"), FEATURE_END)
        )
        _, fragment = fragment.split("\n[mcp_servers.casefile]\n", 1)
        fragment = "[mcp_servers.casefile]\n" + fragment

    index = table_header_index(current, "agents")
    agent_scalars = b""
    if index is not None:
        payload = f"max_depth = {owned['agents']['max_depth']}\n"
        current = insert_after_line(
            current, index, marked(AGENT_BEGIN, payload.encode("ascii"), AGENT_END)
        )
    else:
        agent_scalars = f"agents.max_depth = {owned['agents']['max_depth']}\n".encode("ascii")
    fragment = fragment.replace("\n[agents]\nmax_depth = 2\n", "\n", 1)

    scalar_block = marked(SCALAR_BEGIN, agent_scalars, SCALAR_END) if agent_scalars else b""
    table_block = marked(TABLE_BEGIN, fragment.encode("ascii"), TABLE_END)
    data = scalar_block + current + table_block
    verify_config(data, root, version, binary, planning_root)
    return data


def unowned_config(data: bytes) -> bytes:
    for begin, end, table in (
        (SCALAR_BEGIN, SCALAR_END, ""),
        (FEATURE_BEGIN, FEATURE_END, "features"),
        (AGENT_BEGIN, AGENT_END, "agents"),
    ):
        while begin in data:
            start = data.index(begin)
            stop = data.find(end, start)
            if stop < 0:
                raise SetupError("managed config marker is unbalanced")
            payload = drop_owned_lines(data[start + len(begin) : stop], table)
            data = data[:start] + payload + data[stop + len(end) :]
    result = drop_owned_lines(remove_owned_tables(data))
    if result:
        tomllib.loads(result.decode("utf-8"))
    return result


def remove_owned_tables(data: bytes) -> bytes:
    output = []
    owned = False
    for line in data.splitlines(keepends=True):
        stripped = line.strip()
        if stripped == TABLE_BEGIN.strip():
            if output and not output[-1].strip():
                output.pop()
            continue
        if stripped == TABLE_END.strip():
            continue
        match = re.fullmatch(rb"\s*\[\[?([^\]\r\n]+)\]\]?\s*(?:#.*)?\r?\n?", line)
        if match:
            name = match.group(1).decode("utf-8")
            owned = (
                name == "mcp_servers.casefile"
                or name.startswith("mcp_servers.casefile.")
                or name.startswith("agents.casefile-")
            )
        if not owned:
            output.append(line)
    return b"".join(output)


def verify_config(
    data: bytes,
    root: Path,
    version: str,
    binary: Path | None = None,
    planning_root: Path | None = None,
) -> None:
    document = tomllib.loads(data.decode("utf-8"))
    version = multi_agent_version(version)
    profiles = tomllib.loads((root / "config/profiles.toml").read_text(encoding="ascii"))
    features = document.get("features", {})
    expected_features = {"multi_agent": version == "v1", "multi_agent_v2": version == "v2"}
    if {name: features.get(name) for name in expected_features} != expected_features:
        raise SetupError(f"{version.upper()} feature flags are incorrect")
    if binary is not None and planning_root is not None:
        server = document.get("mcp_servers", {}).get("casefile", {})
        if server.get("command") != str(binary) or server.get("args") != [
            "mcp-package", "--planning-root", str(planning_root)
        ]:
            raise SetupError("Casefile MCP binding is incorrect")
    agents = document.get("agents", {})
    rows = profiles.get("matrix_profiles", [])
    names = [row.get("profile") for row in rows]
    if not names or any(not isinstance(name, str) or not name for name in names) or len(names) != len(set(names)):
        raise SetupError("role profile names must be present and unique")
    matrix_roles = set()
    for source in (root / "matrices").glob("*.toml"):
        matrix = tomllib.loads(source.read_text(encoding="ascii"))
        for worker in matrix.get("workers", []):
            matrix_roles.add((matrix["strategy_id"], worker["role"], worker["platform_profile"]))
    if matrix_roles != {(row["strategy_id"], row["role"], row["profile"]) for row in rows}:
        raise SetupError("Casefile role profiles differ from the packaged strategies")
    if {name for name in agents if name.startswith("casefile-")} != set(names):
        raise SetupError("registered Casefile roles differ from the packaged profiles")
    selection_fields = {"model", "model_reasoning_effort"}
    for row in rows:
        name = row["profile"]
        relative = PurePosixPath(row["agent_file"].replace("\\", "/"))
        expected = root / Path(*relative.parts)
        registration = agents[name]
        actual = registration.get("config_file")
        if (
            not isinstance(actual, str)
            or Path(actual) != expected
            or not Path(actual).is_file()
        ):
            raise SetupError(f"role binding is incorrect: {name}")
        agent = tomllib.loads(expected.read_text(encoding="ascii"))
        if selection_fields.intersection(registration) or selection_fields.intersection(agent):
            raise SetupError(f"Casefile role fixes a model or effort: {name}")
        instructions = agent.get("developer_instructions")
        if not isinstance(instructions, str) or not instructions.strip():
            raise SetupError(f"Casefile role lacks developer instructions: {name}")


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
    if first.is_dir() or second.is_dir():
        if not first.is_dir() or not second.is_dir():
            return False
        first_children = sorted(item.relative_to(first) for item in first.rglob("*"))
        second_children = sorted(item.relative_to(second) for item in second.rglob("*"))
        return first_children == second_children and all(
            same_path(first / child, second / child) for child in first_children
        )
    return not first.exists() and not second.exists()


def snapshot(home: Path, paths: list[Path], destination: Path) -> list[dict]:
    secure_dir(destination)
    entries = []
    for path in paths:
        relative = path.relative_to(home)
        existed = path.exists() or path.is_symlink()
        entries.append({"path": relative.as_posix(), "existed": existed})
        if existed:
            copy_path(path, destination / relative)
    return entries


def receipt_relative(entry: dict) -> Path:
    return Path(*PurePosixPath(entry["path"].replace("\\", "/")).parts)


def require_unchanged(home: Path, paths: list[Path], backup: Path, operation: str) -> None:
    for target in paths:
        if not same_path(target, backup / target.relative_to(home)):
            raise SetupError(f"managed file changed after {operation} snapshot: {target}")


def restore(home: Path, source: Path, entries: list[dict]) -> None:
    for entry in entries:
        relative = receipt_relative(entry)
        path = home / relative
        remove(path)
        if entry["existed"]:
            copy_path(source / relative, path)
        if entry["existed"] and not same_path(source / relative, path):
            raise SetupError(f"restore verification failed: {path}")
        if not entry["existed"] and (path.exists() or path.is_symlink()):
            raise SetupError(f"restore verification failed: {path}")


def catalog_path(home: Path, value: object) -> Path | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return home / Path(value).expanduser()
    except (RuntimeError, ValueError):
        return None


def registered_agent_catalogs(home: Path, document: dict) -> list[Path | None]:
    paths = []
    agents = document.get("agents", {})
    if not isinstance(agents, dict):
        raise ValueError("registered agents are invalid")
    for agent in agents.values():
        if not isinstance(agent, dict) or "config_file" not in agent:
            continue
        source = catalog_path(home, agent["config_file"])
        if source is None or not source.is_file():
            raise ValueError("registered agent config is unavailable")
        definition = tomllib.loads(source.read_text(encoding="utf-8"))
        paths.append(catalog_path(source.parent, definition.get(CATALOG_KEY)))
    return paths


def catalog_references(home: Path, document: dict, active: Path | None) -> set[Path] | None:
    profiles_to_check = []
    profiles = document.get("profiles", {})
    if isinstance(profiles, dict):
        profiles_to_check.extend(profile for profile in profiles.values() if isinstance(profile, dict))
    try:
        for source in home.glob("*.config.toml"):
            if not source.is_file():
                return None
            profiles_to_check.append(tomllib.loads(source.read_text(encoding="utf-8")))
        paths = [active, *registered_agent_catalogs(home, document)]
        for profile in profiles_to_check:
            paths.append(catalog_path(home, profile.get(CATALOG_KEY)))
            paths.extend(registered_agent_catalogs(home, profile))
        return {path.resolve() for path in paths if path is not None}
    except (OSError, RuntimeError, ValueError):
        return None


def detach_catalog_setting(data: bytes, document: dict) -> tuple[bytes, bytes]:
    remainder = {name: value for name, value in document.items() if name != CATALOG_KEY}
    for match in CATALOG_ASSIGNMENT.finditer(data):
        candidate = data[:match.start()] + data[match.end():]
        try:
            if tomllib.loads(candidate.decode("utf-8")) == remainder:
                return candidate, match[0]
        except tomllib.TOMLDecodeError:
            continue
    return data, b""


def legacy_catalog_recovery(home: Path, config: bytes, previous: tuple[Path, dict] | None) -> tuple[bytes, list[dict]]:
    if previous is None:
        return config, []
    receipt_path, receipt_data = previous
    tracked = {entry["path"]: entry for entry in receipt_data["before"] if entry["path"] in LEGACY_CATALOGS}
    if not tracked:
        return config, []
    home = home.resolve()
    document = tomllib.loads(config.decode("utf-8"))
    active = catalog_path(home, document.get(CATALOG_KEY))
    selected = f"models-casefile-{receipt_multi_agent_version(receipt_data)}.json"
    if (
        selected in tracked
        and active == home / selected
        and not active.is_symlink()
        and (active.is_file() or not active.exists())
    ):
        detached, setting = detach_catalog_setting(config, document)
        if setting:
            original = receipt_path.parent / "before/config.toml"
            if any(entry["path"] == "config.toml" and entry["existed"] for entry in receipt_data["before"]):
                before = original.read_bytes()
                before_document = tomllib.loads(before.decode("utf-8"))
                if CATALOG_KEY in before_document:
                    _, original_setting = detach_catalog_setting(before, before_document)
                    if not original_setting:
                        raise SetupError("original model catalog setting cannot be restored from the receipt")
                    detached = original_setting.rstrip(b"\r\n") + b"\n" + detached
            config = detached
            active = None
    surviving = tomllib.loads(unowned_config(config).decode("utf-8"))
    references = catalog_references(home, surviving, active)
    if references is None:
        return config, []
    recovery = []
    for name, entry in tracked.items():
        path = home / name
        if path in references or path.is_symlink() or (path.exists() and not path.is_file()):
            continue
        if path.is_file() or entry["existed"]:
            recovery.append(entry)
    return config, recovery


def pointer(home: Path) -> Path:
    return home / "state/casefile/current.json"


def active_runtime(config: bytes) -> str:
    features = tomllib.loads(config.decode("utf-8")).get("features", {})
    if isinstance(features, dict):
        v1, v2 = features.get("multi_agent", False), features.get("multi_agent_v2", False)
        if v1 is True and v2 is False:
            return "v1"
        if v1 is False and v2 is True:
            return "v2"
    raise SetupError("active multi-agent runtime is ambiguous; pass --multi-agent-version v1 or v2")


def prepare(
    root: Path,
    home: Path,
    executable: str,
    planning_root: Path | None = None,
    version: str | None = None,
) -> dict:
    previous = receipt(home, None) if pointer(home).is_file() else None
    config_path = home / "config.toml"
    current = config_path.read_bytes() if config_path.is_file() else b""
    requested_version = version
    if version is None:
        version = active_runtime(current) if previous else "v1"
    version = multi_agent_version(version)
    root, manifest = plugin_root(root)
    planning = casefile_runtime.planning_root(planning_root or root)
    runtime = casefile_runtime.select(root, manifest["version"])
    binary = casefile_runtime.destination(home, manifest["version"], runtime["target"])
    casefile_runtime.probe(runtime["source"], manifest["version"], planning)
    environment = {**os.environ, "CODEX_HOME": str(home)}
    plugin = discover(executable, environment, manifest["version"])
    recovered_config, catalog_recovery = legacy_catalog_recovery(home, current, previous)
    config = recovered_config
    if previous is not None:
        config = unowned_config(config)
    config = config_candidate(config, root, version, binary, planning)
    return {
        "root": root,
        "home": home,
        "executable": executable,
        "environment": environment,
        "version": plugin["version"],
        "multi_agent_version": version,
        "requested_multi_agent_version": requested_version,
        "current_config": current,
        "recovered_config": recovered_config,
        "config": config,
        "catalog_recovery": catalog_recovery,
        "planning_root": planning,
        "runtime": runtime,
        "binary": binary,
        "previous": previous,
    }


def preview(plan: dict) -> dict:
    home = plan["home"]
    previous = tomllib.loads(plan["current_config"].decode("utf-8")).get(CATALOG_KEY)
    updated = tomllib.loads(plan["config"].decode("utf-8"))
    selected = updated.get(CATALOG_KEY)
    return {
        "operation": "install",
        "plugin_version": plan["version"],
        "config": str(home / "config.toml"),
        "multi_agent_version": plan["multi_agent_version"],
        "receipt_root": str(home / "backups/casefile"),
        "legacy_catalog_recovery": [
            {"path": str(home / entry["path"]), "action": "restore" if entry["existed"] else "remove"}
            for entry in plan["catalog_recovery"]
        ],
        **({"legacy_catalog_setting": {"from": previous, "to": selected}} if previous != selected else {}),
        "minimum_agent_threads": MINIMUM_AGENT_THREADS,
        "agent_threads": updated["agents"]["max_threads"],
        "config_ownership": "marked key and table blocks",
        "restart_required": True,
        "planning_root": str(plan["planning_root"]),
        "casefile_executable": str(plan["binary"]),
        "casefile_target": plan["runtime"]["target"],
    }


def doctor(plan: dict) -> None:
    result = command(
        [plan["executable"], "--strict-config", "doctor", "--json"],
        plan["environment"],
    )
    try:
        check = json.loads(result.stdout)["checks"]["config.load"]
        status = check["status"]
        summary = check["summary"]
    except (ValueError, KeyError, TypeError) as error:
        raise SetupError(result.stderr.strip() or "Codex doctor did not report config loading") from error
    if status == "ok":
        return
    if status == "warning" and summary == "config loaded" and check.get("details", {}).get("config.toml parse") == "ok":
        return
    raise SetupError(f"Codex config load failed: {summary}")


def install(plan: dict) -> dict:
    plan = prepare(
        plan["root"], plan["home"], plan["executable"], plan["planning_root"],
        plan["requested_multi_agent_version"],
    )
    home = plan["home"]
    secure_dir(home / "backups/casefile")
    receipt_dir = Path(
        tempfile.mkdtemp(
            prefix=datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ-"),
            dir=home / "backups/casefile",
        )
    )
    secure_dir(receipt_dir)
    config = home / "config.toml"
    paths = [config, plan["binary"]]
    retiring = [home / entry["path"] for entry in plan["catalog_recovery"]]
    rollback_paths = [*paths, *retiring, pointer(home)]
    rollback = snapshot(home, rollback_paths, receipt_dir / "rollback")
    if plan["previous"] is None:
        before = snapshot(home, paths, receipt_dir / "before")
    else:
        previous_path, previous_value = plan["previous"]
        before = [entry for entry in previous_value["before"] if entry["path"] not in LEGACY_CATALOGS]
        for entry in before:
            if entry["existed"]:
                relative = receipt_relative(entry)
                copy_path(previous_path.parent / "before" / relative, receipt_dir / "before" / relative)
        recorded = {entry["path"] for entry in before}
        newly_owned = [path for path in paths if path.relative_to(home).as_posix() not in recorded]
        before.extend(snapshot(home, newly_owned, receipt_dir / "before"))
    if (config.read_bytes() if config.is_file() else b"") != plan["current_config"]:
        raise SetupError("Codex configuration changed during setup; preview again")
    require_unchanged(home, rollback_paths, receipt_dir / "rollback", "setup")
    if legacy_catalog_recovery(home, plan["current_config"], plan["previous"]) != (
        plan["recovered_config"], plan["catalog_recovery"],
    ):
        raise SetupError("Codex catalog references changed during setup; preview again")
    try:
        casefile_runtime.atomic_copy(plan["runtime"]["source"], plan["binary"])
        casefile_runtime.require_landed(plan["binary"])
        casefile_runtime.probe(plan["binary"], plan["version"], plan["planning_root"])
        atomic_write(config, plan["config"], 0o600)
        if retiring:
            restore(home, plan["previous"][0].parent / "before", plan["catalog_recovery"])
        verify_config(
            config.read_bytes(), plan["root"], plan["multi_agent_version"],
            plan["binary"], plan["planning_root"],
        )
        doctor(plan)
        verify_config(
            config.read_bytes(), plan["root"], plan["multi_agent_version"],
            plan["binary"], plan["planning_root"],
        )
        receipt = {
            "schema_version": RECEIPT_SCHEMA,
            "status": "installed",
            "install_id": receipt_dir.name,
            "plugin_version": plan["version"],
            "multi_agent_version": plan["multi_agent_version"],
            "before": before,
            "remove_plugin": True,
            "remove_marketplace": False,
            "casefile_binary": str(plan["binary"].relative_to(home)),
            "owned_binaries": [
                *(
                    plan["previous"][1].get("owned_binaries", [])
                    if plan["previous"] is not None
                    else []
                ),
                str(plan["binary"].relative_to(home)),
            ],
            "planning_root": str(plan["planning_root"]),
        }
        receipt_data = canonical(receipt)
        receipt_path = receipt_dir / "receipt.json"
        atomic_write(receipt_path, receipt_data)
        secure_dir(pointer(home).parent)
        atomic_write(pointer(home), canonical({"receipt": str(receipt_path)}))
        return {"status": "installed", "receipt": str(receipt_path), "multi_agent_version": plan["multi_agent_version"], "restart_required": True}
    except BaseException as error:
        restore(home, receipt_dir / "rollback", rollback)
        atomic_write(
            receipt_dir / "failure.json",
            canonical({"status": "failed", "error": str(error), "rollback_verified": True}),
        )
        raise SetupError(f"setup failed; rollback verified: {error}") from error


def read_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_bytes())
    except (OSError, json.JSONDecodeError) as error:
        raise SetupError(f"invalid receipt: {error}") from error
    if not isinstance(value, dict):
        raise SetupError("invalid receipt object")
    return value


def receipt_multi_agent_version(value: dict) -> str:
    return multi_agent_version(value.get("multi_agent_version", "v1"))


def receipt(home: Path, explicit: Path | None) -> tuple[Path, dict]:
    """Read our own receipt. Confine it to the backup root; trust what we wrote."""
    if explicit is None:
        path = Path(read_json(pointer(home)).get("receipt", ""))
    else:
        path = explicit.expanduser()
    path = path.resolve(strict=True)
    root = (home / "backups/casefile").resolve()
    if root not in path.parents or path.name != "receipt.json":
        raise SetupError("receipt is outside the durable backup root")
    value = read_json(path)
    if value.get("status") != "installed":
        raise SetupError("receipt is not an installed receipt")
    if explicit is not None:
        require_current_receipt(home, path)
    return path, value


def require_current_receipt(home: Path, path: Path) -> None:
    if pointer(home).is_file() and receipt(home, None)[0] != path.resolve():
        raise SetupError("uninstall receipt is stale; use the active Casefile receipt")


def uninstall_preview(path: Path, value: dict) -> dict:
    return {
        "operation": "uninstall",
        "receipt": str(path),
        "restore_snapshot": str(path.parent / "before"),
        "managed_path_count": len(value["before"]),
        "multi_agent_version": receipt_multi_agent_version(value),
        "preserve_unowned_config": True,
        "remove_plugin": value["remove_plugin"],
        "remove_marketplace": value["remove_marketplace"],
        "review": "git diffs for changed managed files follow",
    }


def uninstall_recovery(home: Path, path: Path, value: dict, current: bytes | None) -> tuple[bytes | None, list[dict]]:
    require_current_receipt(home, path)
    before = value["before"]
    if not before or before[0].get("path") != "config.toml":
        raise SetupError("receipt config inventory is invalid")
    previous = (path, value) if pointer(home).is_file() else None
    config, catalogs = legacy_catalog_recovery(home, current or b"", previous)
    entries = [entry for entry in before[1:] if entry["path"] not in LEGACY_CATALOGS]
    config_path = home / "config.toml"
    if current is None and before[0]["existed"] and not config_path.exists() and not config_path.is_symlink():
        original = (path.parent / "before/config.toml").read_text(encoding="utf-8")
        tomllib.loads(original)
        entries.insert(0, before[0])
    return unowned_config(config) if current is not None else None, [*entries, *catalogs]


def show_uninstall_diffs(home: Path, path: Path, value: dict) -> None:
    config = home / "config.toml"
    current = config.read_bytes() if config.is_file() else None
    restored_config, recovery = uninstall_recovery(home, path, value, current)
    with tempfile.TemporaryDirectory(prefix="humans-md-uninstall-") as temporary:
        temporary = Path(temporary)
        missing = temporary / "missing"
        missing.touch()
        targets = []
        if restored_config is not None:
            restored = temporary / "config.toml"
            restored.write_bytes(restored_config)
            targets.append((config, restored))
        for entry in recovery:
            if entry["path"] == "config.toml" or entry["path"] in LEGACY_CATALOGS:
                relative = receipt_relative(entry)
                original = path.parent / "before" / relative if entry["existed"] else missing
                targets.append((home / relative, original))
        for target, restored in targets:
            existing = target if target.exists() or target.is_symlink() else missing
            result = subprocess.run(
                ["git", "diff", "--no-index", "--", str(existing), str(restored)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="strict",
            )
            if result.returncode not in (0, 1):
                raise SetupError(result.stderr.strip() or "git diff --no-index failed")
            if result.stdout:
                print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")


def uninstall(home: Path, executable: str, path: Path, value: dict) -> dict:
    config = home / "config.toml"
    initial_config = config.read_bytes() if config.is_file() else None
    restored_config, recovery = uninstall_recovery(home, path, value, initial_config)
    binaries = [
        home / Path(*PurePosixPath(relative.replace("\\", "/")).parts)
        for relative in value.get("owned_binaries", [])
    ]
    recovery_paths = [home / receipt_relative(entry) for entry in recovery]
    current = list(dict.fromkeys([config, *binaries, *recovery_paths]))
    backup_root = home / "backups/casefile"
    secure_dir(backup_root)
    rollback_dir = Path(tempfile.mkdtemp(
        prefix=datetime.datetime.now(datetime.UTC).strftime("uninstall-%Y%m%dT%H%M%SZ-"),
        dir=backup_root,
    ))
    rollback_paths = current + [
        home / "plugins/cache/casefile",
        pointer(home),
    ]
    rollback = snapshot(home, rollback_paths, rollback_dir / "before")
    environment = {**os.environ, "CODEX_HOME": str(home)}
    if (config.read_bytes() if config.is_file() else None) != initial_config:
        raise SetupError(f"managed file changed after uninstall snapshot: {config}")
    require_unchanged(home, rollback_paths, rollback_dir / "before", "uninstall")
    if uninstall_recovery(home, path, value, initial_config) != (restored_config, recovery):
        raise SetupError("Codex catalog references changed during uninstall; preview again")
    try:
        restore(home, path.parent / "before", recovery)
        if restored_config is not None:
            if restored_config or value["before"][0]["existed"]:
                atomic_write(config, restored_config, 0o600)
            else:
                remove(config)
        if value["remove_plugin"]:
            checked([executable, "plugin", "remove", PLUGIN_ID, "--json"], environment)
        pointer(home).unlink(missing_ok=True)
        result = {"status": "uninstalled", "install_receipt": str(path), "multi_agent_version": receipt_multi_agent_version(value)}
        atomic_write(rollback_dir / "receipt.json", canonical(result))
        return result
    except BaseException as error:
        restore(home, rollback_dir / "before", rollback)
        atomic_write(
            rollback_dir / "failure.json",
            canonical({"status": "failed", "error": str(error), "rollback_verified": True}),
        )
        raise SetupError(f"uninstall failed; rollback verified: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="operation", required=True)
    install_parser = subparsers.add_parser("install")
    install_parser.add_argument("--plugin-root", type=Path, required=True)
    install_parser.add_argument("--planning-root", type=Path, required=True)
    default_home = Path(os.environ.get("CODEX_HOME", "~/.codex"))
    install_parser.add_argument("--codex-home", type=Path, default=default_home)
    install_parser.add_argument("--codex-executable", default=shutil.which("codex"))
    install_parser.add_argument("--multi-agent-version", choices=("v1", "v2"), action="append")
    install_parser.add_argument("--apply", action="store_true")
    uninstall_parser = subparsers.add_parser("uninstall")
    uninstall_parser.add_argument("--codex-home", type=Path, default=default_home)
    uninstall_parser.add_argument("--codex-executable", default=shutil.which("codex"))
    uninstall_parser.add_argument("--receipt", type=Path)
    uninstall_parser.add_argument("--apply", action="store_true")
    arguments = parser.parse_args()
    try:
        home = arguments.codex_home.expanduser().resolve(strict=True)
        if not arguments.codex_executable:
            raise SetupError("Codex executable was not found")  # kept: nothing can run without it
        if arguments.operation == "install":
            selections = arguments.multi_agent_version or []
            selected_version = selections[-1] if selections else None
            plan = prepare(
                arguments.plugin_root, home, arguments.codex_executable,
                arguments.planning_root, selected_version,
            )
            print(json.dumps(preview(plan), indent=2, sort_keys=True))
            if not arguments.apply:
                print("preview only; no files changed")
                return 0
            print(json.dumps(install(plan), indent=2, sort_keys=True))
            return 0
        receipt_path, value = receipt(home, arguments.receipt)
        print(json.dumps(uninstall_preview(receipt_path, value), indent=2, sort_keys=True))
        show_uninstall_diffs(home, receipt_path, value)
        if not arguments.apply:
            print("preview only; no files changed")
            return 0
        print(json.dumps(uninstall(home, arguments.codex_executable, receipt_path, value), indent=2))
        return 0
    except (OSError, UnicodeError, ValueError, SetupError, tomllib.TOMLDecodeError) as error:
        print(f"{arguments.operation} failed: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
