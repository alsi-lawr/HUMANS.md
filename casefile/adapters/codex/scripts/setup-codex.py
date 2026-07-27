#!/usr/bin/env python3
"""Deterministically preview, install, or uninstall Casefile integration for Codex."""
from __future__ import annotations

import argparse
import copy
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

try:
    import codex_app_server
except ModuleNotFoundError:
    _app_server_path = Path(__file__).resolve().with_name("codex_app_server.py")
    _app_server_spec = importlib.util.spec_from_file_location(
        "codex_app_server", _app_server_path
    )
    if _app_server_spec is None or _app_server_spec.loader is None:
        raise
    codex_app_server = importlib.util.module_from_spec(_app_server_spec)
    _app_server_spec.loader.exec_module(codex_app_server)


PLUGIN_ID = "casefile@humans-md"
MARKETPLACE = "humans-md"
RECEIPT_SCHEMA = 6
RECEIPT_SCHEMAS = {4, 5, 6}
REQUIRED_MODELS = {
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "gpt-5.3-codex-spark",
}
V1_SELECTOR_MODELS = {"gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"}
MULTI_AGENT_VERSIONS = {"v1", "v2"}
V2_MINIMUM_CODEX_VERSION = (0, 145, 0)
SCALAR_BEGIN = b"# >>> casefile setup scalars >>>\n"
SCALAR_END = b"# <<< casefile setup scalars <<<\n"
TABLE_BEGIN = b"\n# >>> casefile setup tables >>>\n"
TABLE_END = b"# <<< casefile setup tables <<<\n"
LEGACY_PATHS: tuple[str, ...] = ()


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
        "scripts/codex_app_server.py",
        "scripts/resolve-writer-binding.py",
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


def resource(profile: Path, target: dict, path_key: str) -> bytes:
    value = target.get(path_key)
    if not isinstance(value, str) or not value:
        raise SetupError(f"catalog target lacks {path_key}")
    relative = PurePosixPath(value.replace("\\", "/"))
    if relative.is_absolute() or ".." in relative.parts:
        raise SetupError(f"unsafe catalog resource: {value}")
    path = (profile.parent / Path(*relative.parts)).resolve(strict=True)
    if profile.parent.resolve() not in path.parents or not path.is_file() or path.is_symlink():
        raise SetupError(f"unsafe catalog resource: {value}")
    data = path.read_bytes()
    if not data:
        raise SetupError(f"catalog resource is empty: {value}")
    return data


def set_selector(model: dict, dotted: str, value: object = None) -> None:
    current = model
    parts = dotted.split(".")
    for part in parts[:-1]:
        current = current.get(part)
        if not isinstance(current, dict):
            raise SetupError(f"catalog selector is missing: {dotted}")
    if parts[-1] not in current:
        raise SetupError(f"catalog selector is missing: {dotted}")
    current[parts[-1]] = value


def multi_agent_version(value: str) -> str:
    if value not in MULTI_AGENT_VERSIONS:
        raise SetupError(f"unsupported multi-agent version: {value!r}")
    return value


def catalog_override(
    raw: dict, profile_path: Path, version: str
) -> tuple[bytes, list[str], list[str]]:
    version = multi_agent_version(version)
    profile = tomllib.loads(profile_path.read_text(encoding="ascii"))
    policy = profile.get("catalog", {})
    models = raw.get("models")
    if profile.get("schema_version") != 1 or not isinstance(models, list):
        raise SetupError("unsupported catalog or profile schema")
    if policy.get("id_field") != "slug" or policy.get("selector_fields") != [
        "multi_agent_version"
    ]:
        raise SetupError("unsupported catalog policy")
    by_id = {model.get("slug"): model for model in models if isinstance(model, dict)}
    if None in by_id or len(by_id) != len(models):
        raise SetupError("catalog has missing or duplicate model IDs")
    missing = sorted(REQUIRED_MODELS - by_id.keys())
    if missing:
        raise SetupError(f"catalog lacks required models: {', '.join(missing)}")

    result = copy.deepcopy(raw)
    output = {model["slug"]: model for model in result["models"]}
    patched: list[str] = []
    skipped: list[str] = []
    for target in policy.get("targets", []):
        model_id = target.get("id")
        if model_id not in output:
            skipped.append(str(model_id))
            continue
        model = output[model_id]
        model["base_instructions"] = resource(profile_path, target, "base_instructions_file").decode(
            "ascii"
        )
        messages = json.loads(
            resource(profile_path, target, "model_messages_file").decode("ascii")
        )
        if not isinstance(messages, dict):
            raise SetupError(f"invalid model messages: {model_id}")
        model["model_messages"] = messages
        selectors = target.get("null_selectors", [])
        if not isinstance(selectors, list) or set(selectors) - {"multi_agent_version"}:
            raise SetupError(f"unsupported selector for {model_id}")
        for selector in selectors:
            set_selector(model, selector)
        patched.append(model_id)
    if not REQUIRED_MODELS <= set(patched):
        raise SetupError("required models were not patched")
    if version == "v1":
        if any(output[model].get("multi_agent_version") is not None for model in V1_SELECTOR_MODELS):
            raise SetupError("required V1 selectors were not cleared")
    else:
        for model in result["models"]:
            model["multi_agent_version"] = "v2"
            if model["multi_agent_version"] != "v2":
                raise SetupError("required V2 selectors were not assigned")
    return canonical(result), sorted(patched), sorted(skipped)


def marked(begin: bytes, payload: bytes, end: bytes) -> bytes:
    return begin + payload.rstrip(b"\n") + b"\n" + end


def config_candidate(
    current: bytes,
    root: Path,
    catalog: Path,
    version: str,
    binary: Path,
    planning_root: Path,
) -> bytes:
    document = tomllib.loads(current.decode("utf-8")) if current else {}
    conflicts = {
        key
        for key in ("model_catalog_json", "features", "agents")
        if key in document
    }
    if isinstance(document.get("mcp_servers"), dict) and "casefile" in document["mcp_servers"]:
        conflicts.add("mcp_servers.casefile")
    if conflicts:
        raise SetupError(
            "managed config already exists; clean it before setup: " + ", ".join(sorted(conflicts))
        )
    version = multi_agent_version(version)
    fragment = (root / "config/config-fragment.toml.in").read_text(encoding="ascii")
    fragment = fragment.replace("__HUMANS_MD_PLUGIN_ROOT__", str(root).replace("\\", "/"))
    fragment = fragment.replace("__CASEFILE_MULTI_AGENT_V1__", "true" if version == "v1" else "false")
    fragment = fragment.replace("__CASEFILE_MULTI_AGENT_V2__", "false" if version == "v1" else "true")
    fragment = fragment.replace("__CASEFILE_EXECUTABLE__", json.dumps(str(binary)))
    fragment = fragment.replace("__CASEFILE_PLANNING_ROOT__", json.dumps(str(planning_root)))
    scalar_block = marked(
        SCALAR_BEGIN,
        f"model_catalog_json = {json.dumps(str(catalog))}\n".encode("utf-8"),
        SCALAR_END,
    )
    table_block = marked(TABLE_BEGIN, fragment.encode("ascii"), TABLE_END)
    data = scalar_block + current + table_block
    verify_config(data, root, catalog, version, binary, planning_root)
    return data


def unowned_config(data: bytes) -> bytes:
    ranges = []
    for name, begin, end in (
        ("scalars", SCALAR_BEGIN, SCALAR_END),
        ("tables", TABLE_BEGIN, TABLE_END),
    ):
        if data.count(begin) != 1 or data.count(end) != 1:
            raise SetupError(f"managed config block is missing or duplicated: {name}")
        start = data.index(begin)
        stop = data.index(end, start) + len(end)
        ranges.append((start, stop))
    ranges.sort()
    if ranges[0][1] > ranges[1][0]:
        raise SetupError("managed config blocks overlap")
    result = data[: ranges[0][0]] + data[ranges[0][1] : ranges[1][0]] + data[ranges[1][1] :]
    if result:
        tomllib.loads(result.decode("utf-8"))
    return result


def verify_config(
    data: bytes,
    root: Path,
    catalog: Path,
    version: str,
    binary: Path | None = None,
    planning_root: Path | None = None,
) -> None:
    document = tomllib.loads(data.decode("utf-8"))
    version = multi_agent_version(version)
    profiles = tomllib.loads((root / "config/profiles.toml").read_text(encoding="ascii"))
    if document.get("model_catalog_json") != str(catalog):
        raise SetupError("catalog path is incorrect")
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
    matrix_rows = profiles.get("matrix_profiles", [])
    writer_rows = profiles.get("writer_profiles", [])
    override_rows = profiles.get("writer_runtime_overrides", [])
    rows = [*matrix_rows, *writer_rows, *override_rows]
    names = [row.get("profile") for row in rows]
    if any(not isinstance(name, str) or not name for name in names) or len(names) != len(set(names)):
        raise SetupError("role profile names must be non-empty and unique")
    targets = {target.get("id"): target for target in profiles.get("catalog", {}).get("targets", [])}
    expected_v1 = {
        (model, effort)
        for model in V1_SELECTOR_MODELS
        for effort in targets.get(model, {}).get("required_reasoning", [])
    }
    actual_v1 = {
        (row.get("model"), row.get("reasoning")) for row in writer_rows
    }
    if actual_v1 != expected_v1 or any(
        row.get("multi_agent_version") != "v1"
        or row.get("role") != "implementation-writer"
        or set(row.get("strategy_ids", []))
        != {
            "casefile-implement-ticket-batch",
            "casefile-implement-ticket-batch-look-ahead",
            "casefile-implement-pipeline",
        }
        for row in writer_rows
    ):
        raise SetupError("V1 writer profile catalog is incomplete or unsupported")
    if {
        row.get("strategy_id") for row in override_rows
    } != {
        "casefile-implement-ticket-batch",
        "casefile-implement-ticket-batch-look-ahead",
        "casefile-implement-pipeline",
    } or any(
        row.get("multi_agent_version") != "v2"
        or row.get("role") != "implementation-writer"
        or row.get("model_override") is not True
        or row.get("reasoning_override") is not True
        or not isinstance(row.get("fork_turns"), int)
        or row["fork_turns"] <= 0
        for row in override_rows
    ):
        raise SetupError("V2 writer runtime-override catalog is incomplete or unsupported")
    for row in rows:
        relative = PurePosixPath(row["agent_file"].replace("\\", "/"))
        expected = root / Path(*relative.parts)
        actual = agents.get(row["profile"], {}).get("config_file")
        if actual != str(expected) or not expected.is_file():
            raise SetupError(f"role binding is incorrect: {row['profile']}")
        agent = tomllib.loads(expected.read_text(encoding="ascii"))
        if row in override_rows:
            if "model" in agent or "model_reasoning_effort" in agent:
                raise SetupError(f"runtime-override role fixes a model: {row['profile']}")
        elif agent.get("model") != row.get("model") or agent.get(
            "model_reasoning_effort"
        ) != row.get("reasoning"):
            raise SetupError(f"role model binding is incoherent: {row['profile']}")


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


def restore(home: Path, source: Path, entries: list[dict]) -> None:
    for entry in entries:
        relative = Path(*PurePosixPath(entry["path"].replace("\\", "/")).parts)
        path = home / relative
        remove(path)
        if entry["existed"]:
            copy_path(source / relative, path)
        if entry["existed"] and not same_path(source / relative, path):
            raise SetupError(f"restore verification failed: {path}")
        if not entry["existed"] and (path.exists() or path.is_symlink()):
            raise SetupError(f"restore verification failed: {path}")


def managed(home: Path, version: str = "v1", binary: Path | None = None) -> list[Path]:
    version = multi_agent_version(version)
    paths = [
        home / "config.toml",
        home / f"models-casefile-{version}.json",
        *(home / relative for relative in LEGACY_PATHS),
    ]
    if binary is not None:
        paths.append(binary)
    return paths


def pointer(home: Path) -> Path:
    return home / "state/casefile/current.json"


def codex_version(executable: str, environment: dict[str, str]) -> tuple[int, int, int]:
    output = checked([executable, "--version"], environment).strip()
    found = re.search(r"\b(\d+)\.(\d+)\.(\d+)\b", output)
    if found is None:
        raise SetupError(f"Codex version is not parseable: {output or 'no version output'}")
    return tuple(int(value) for value in found.groups())


def require_v2(executable: str, environment: dict[str, str]) -> tuple[int, int, int]:
    version = codex_version(executable, environment)
    if version < V2_MINIMUM_CODEX_VERSION:
        required = ".".join(str(value) for value in V2_MINIMUM_CODEX_VERSION)
        actual = ".".join(str(value) for value in version)
        raise SetupError(f"multi-agent V2 requires Codex {required} or newer; found {actual}")
    return version


def acquire_models(executable: str, home: Path, environment: dict[str, str]) -> dict:
    try:
        return codex_app_server.authenticated_model_catalog(executable, home, environment)
    except codex_app_server.AppServerError as error:
        raise SetupError(f"Codex model availability failed: {error}") from error


def catalog_ids(document: dict, label: str) -> set[str]:
    models = document.get("models")
    if not isinstance(models, list):
        raise SetupError(f"{label} has no model list")
    identifiers: list[str] = []
    for model in models:
        identifier = model.get("slug") if isinstance(model, dict) else None
        if not isinstance(identifier, str) or not identifier:
            raise SetupError(f"{label} has a model without an ID")
        identifiers.append(identifier)
    if len(set(identifiers)) != len(identifiers):
        raise SetupError(f"{label} has duplicate model IDs")
    return set(identifiers)


def require_available_models(projection: dict) -> set[str]:
    identifiers = catalog_ids(projection, "Codex model projection")
    missing = sorted(REQUIRED_MODELS - identifiers)
    if missing:
        raise SetupError(f"Codex lacks required models: {', '.join(missing)}")
    return identifiers


def raw_catalog(path: Path) -> dict:
    if path.is_symlink() or not path.is_file():
        raise SetupError(f"Codex catalog source is missing or unsafe: {path}")
    try:
        data = path.read_bytes()
        value = json.loads(data)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SetupError(f"Codex catalog source is invalid: {path}: {error}") from error
    if not isinstance(value, dict):
        raise SetupError(f"Codex catalog source is not an object: {path}")
    return value


def cross_check_catalog(projection_ids: set[str], raw: dict, label: str) -> None:
    raw_ids = catalog_ids(raw, label)
    if raw_ids != projection_ids:
        missing = sorted(projection_ids - raw_ids)
        extra = sorted(raw_ids - projection_ids)
        details = []
        if missing:
            details.append("missing " + ", ".join(missing))
        if extra:
            details.append("unexpected " + ", ".join(extra))
        raise SetupError(f"{label} differs from Codex model projection: {'; '.join(details)}")


def verify_catalog_selectors(document: dict, version: str, label: str) -> None:
    version = multi_agent_version(version)
    models = document.get("models")
    if not isinstance(models, list):
        raise SetupError(f"{label} has no model list")
    selected = {
        model.get("slug"): model for model in models if isinstance(model, dict)
    }
    if version == "v1":
        for model_id in V1_SELECTOR_MODELS:
            model = selected.get(model_id)
            if not isinstance(model, dict) or model.get("multi_agent_version") is not None:
                raise SetupError(f"{label} did not activate V1 for {model_id}")
    else:
        for model_id, model in selected.items():
            if model.get("multi_agent_version") != "v2":
                raise SetupError(f"{label} did not activate V2 for {model_id}")


def prepare(
    root: Path,
    home: Path,
    executable: str,
    planning_root: Path | str | None = None,
    version: str = "v1",
) -> dict:
    if isinstance(planning_root, str) and planning_root in MULTI_AGENT_VERSIONS:
        version = planning_root
        planning_root = None
    version = multi_agent_version(version)
    root, manifest = plugin_root(root)
    planning = casefile_runtime.planning_root(planning_root or root)
    runtime = casefile_runtime.select(root, manifest["version"])
    binary = casefile_runtime.destination(home, manifest["version"], runtime["target"])
    casefile_runtime.probe(runtime["source"], manifest["version"], planning)
    environment = {**os.environ, "CODEX_HOME": str(home)}
    plugin = discover(executable, environment, manifest["version"])
    if version == "v2":
        require_v2(executable, environment)
    catalog_path = home / f"models-casefile-{version}.json"
    config_path = home / "config.toml"
    current = config_path.read_bytes() if config_path.is_file() else b""
    previous = receipt(home, None) if pointer(home).is_file() else None
    if previous is not None:
        if previous[1].get("plugin_version") == manifest["version"]:
            raise SetupError("this Casefile version is already installed")
        if receipt_multi_agent_version(previous[1]) != version:
            raise SetupError("upgrade must retain the installed multi-agent version")
        current = unowned_config(current)
    # Refuse a managed configuration before asking Codex to refresh its authenticated catalog.
    config = config_candidate(current, root, catalog_path, version, binary, planning)
    acquired = acquire_models(executable, home, environment)
    projection = acquired["projection"]
    projection_ids = require_available_models(projection)
    raw = acquired["raw"]
    cross_check_catalog(projection_ids, raw, "fresh Codex model cache")
    if previous is not None:
        owned = raw_catalog(catalog_path)
        cross_check_catalog(projection_ids, owned, "active Casefile catalog")
        verify_catalog_selectors(owned, version, "active Casefile catalog")
    catalog, patched, skipped = catalog_override(raw, root / "config/profiles.toml", version)
    return {
        "root": root,
        "home": home,
        "executable": executable,
        "environment": environment,
        "version": plugin["version"],
        "multi_agent_version": version,
        "config": config,
        "catalog": catalog,
        "patched": patched,
        "skipped": skipped,
        "legacy": [str(path) for path in managed(home)[3:] if path.exists()],
        "planning_root": planning,
        "runtime": runtime,
        "binary": binary,
        "previous": previous,
    }


def preview(plan: dict) -> dict:
    home = plan["home"]
    return {
        "operation": "install",
        "plugin_version": plan["version"],
        "config": str(home / "config.toml"),
        "catalog": str(home / f"models-casefile-{plan['multi_agent_version']}.json"),
        "multi_agent_version": plan["multi_agent_version"],
        "receipt_root": str(home / "backups/casefile"),
        "patched_models": plan["patched"],
        "skipped_optional_models": plan["skipped"],
        "config_ownership": "marked blocks",
        "restart_required": True,
        "planning_root": str(plan["planning_root"]),
        "casefile_executable": str(plan["binary"]),
        "casefile_target": plan["runtime"]["target"],
    }


def doctor(plan: dict) -> None:
    result = command(
        [
            plan["executable"],
            "--strict-config",
            "doctor",
            "--summary",
            "--ascii",
            "--no-color",
        ],
        plan["environment"],
    )
    if not re.search(r"\[ok\]\s+config\s+loaded", result.stdout + result.stderr):
        raise SetupError(result.stderr.strip() or result.stdout.strip() or "config load failed")


def verify_effective_catalog(plan: dict) -> None:
    version = multi_agent_version(plan.get("multi_agent_version", "v1"))
    acquired = acquire_models(plan["executable"], plan["home"], plan["environment"])
    projection = acquired["projection"]
    projection_ids = require_available_models(projection)
    catalog_path = plan["home"] / f"models-casefile-{version}.json"
    document = raw_catalog(catalog_path)
    cross_check_catalog(projection_ids, document, "written Casefile catalog")
    verify_catalog_selectors(document, version, "effective catalog")


def install(plan: dict) -> dict:
    plan = prepare(
        plan["root"], plan["home"], plan["executable"], plan["planning_root"],
        plan["multi_agent_version"],
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
    paths = managed(home, plan["multi_agent_version"], plan["binary"])
    rollback = snapshot(home, [*paths, pointer(home)], receipt_dir / "rollback")
    if plan["previous"] is None:
        before = snapshot(home, paths, receipt_dir / "before")
    else:
        previous_path, previous_value = plan["previous"]
        before = copy.deepcopy(previous_value["before"])
        before.append({"path": plan["binary"].relative_to(home).as_posix(), "existed": False})
        copy_path(previous_path.parent / "before", receipt_dir / "before")
    try:
        config, catalog = paths[:2]
        casefile_runtime.atomic_copy(plan["runtime"]["source"], plan["binary"])
        if casefile_runtime.sha256(plan["binary"]) != plan["runtime"]["sha256"]:
            raise SetupError("installed Casefile executable hash differs from manifest")
        casefile_runtime.probe(plan["binary"], plan["version"], plan["planning_root"])
        atomic_write(config, plan["config"], 0o600)
        atomic_write(catalog, plan["catalog"], 0o600)
        verify_config(
            config.read_bytes(), plan["root"], catalog, plan["multi_agent_version"],
            plan["binary"], plan["planning_root"],
        )
        if catalog.read_bytes() != plan["catalog"]:
            raise SetupError("written setup bytes differ from preview")
        doctor(plan)
        verify_effective_catalog(plan)
        if config.read_bytes() != plan["config"]:
            raise SetupError("Codex changed config during verification")
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
            "artifact_sha256": plan["runtime"]["sha256"],
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
    if explicit is None:
        selected = read_json(pointer(home))
        path = Path(selected.get("receipt", ""))
    else:
        path = explicit.expanduser().resolve(strict=True)
    path = path.resolve(strict=True)
    root = (home / "backups/casefile").resolve()
    if root not in path.parents or path.name != "receipt.json":
        raise SetupError("receipt is outside the durable backup root")
    value = read_json(path)
    if value.get("status") != "installed" or value.get("schema_version") not in RECEIPT_SCHEMAS:
        raise SetupError("receipt is not an installed receipt")
    inventory = value.get("before")
    if not isinstance(inventory, list) or not inventory:
        raise SetupError("receipt backup inventory is invalid")
    version = receipt_multi_agent_version(value)
    expected = [path.relative_to(home) for path in managed(home, version)]
    owned = value.get("owned_binaries", [])
    if not isinstance(owned, list) or any(not isinstance(item, str) for item in owned):
        raise SetupError("receipt Casefile binary inventory is invalid")
    expected.extend(Path(*PurePosixPath(item.replace("\\", "/")).parts) for item in owned)
    if len(inventory) != len(expected) or len(set(expected)) != len(expected):
        raise SetupError("receipt backup inventory is incomplete")
    for entry, relative in zip(inventory, expected, strict=True):
        if not isinstance(entry, dict) or not isinstance(entry.get("path"), str) or not isinstance(
            entry.get("existed"), bool
        ):
            raise SetupError("receipt backup inventory is invalid")
        normalized = entry["path"].replace("\\", "/")
        if normalized != relative.as_posix():
            raise SetupError("unsafe receipt path")
        if relative not in expected[:2] and (
            relative.is_absolute()
            or ".." in relative.parts
            or relative.parts[:2] != ("casefile", "runtime")
        ):
            raise SetupError("unsafe receipt binary path")
    if not isinstance(value.get("remove_plugin"), bool) or not isinstance(
        value.get("remove_marketplace"), bool
    ):
        raise SetupError("receipt removal policy is invalid")
    return path, value


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


def show_uninstall_diffs(home: Path, path: Path, value: dict) -> None:
    current = managed(home, receipt_multi_agent_version(value))[:2]
    before = {
        Path(*PurePosixPath(item["path"].replace("\\", "/")).parts): item
        for item in value["before"]
    }
    with tempfile.TemporaryDirectory(prefix="humans-md-uninstall-") as temporary:
        temporary = Path(temporary)
        missing = temporary / "missing"
        missing.touch()
        for target in current:
            relative = target.relative_to(home)
            if target == current[0] and target.is_file():
                restored = temporary / relative
                restored.parent.mkdir(parents=True, exist_ok=True)
                restored.write_bytes(unowned_config(target.read_bytes()))
            elif before[relative]["existed"]:
                restored = path.parent / "before" / relative
            else:
                restored = missing
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
    binaries = [
        home / Path(*PurePosixPath(relative.replace("\\", "/")).parts)
        for relative in value.get("owned_binaries", [])
    ]
    current = [*managed(home, receipt_multi_agent_version(value)), *binaries]
    config = current[0]
    rollback_dir = home / "backups/casefile" / (
        "uninstall-" + datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ")
    )
    secure_dir(rollback_dir.parent)
    rollback_paths = current + [
        home / "plugins/cache/casefile",
        pointer(home),
    ]
    rollback = snapshot(home, rollback_paths, rollback_dir / "before")
    environment = {**os.environ, "CODEX_HOME": str(home)}
    try:
        before = value["before"]
        if not before or before[0].get("path") != "config.toml":
            raise SetupError("receipt config inventory is invalid")
        restored_config = unowned_config(config.read_bytes()) if config.is_file() else None
        for target in current:
            snapshot_path = rollback_dir / "before" / target.relative_to(home)
            if not same_path(target, snapshot_path):
                raise SetupError(f"managed file changed after uninstall snapshot: {target}")
        restore(home, path.parent / "before", before[1:])
        if restored_config is None:
            restore(home, path.parent / "before", before[:1])
        elif restored_config or before[0]["existed"]:
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
            raise SetupError("Codex executable was not found")
        if arguments.operation == "install":
            selections = arguments.multi_agent_version or []
            if len(selections) > 1:
                raise SetupError("pass --multi-agent-version at most once")
            selected_version = selections[0] if selections else "v1"
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
