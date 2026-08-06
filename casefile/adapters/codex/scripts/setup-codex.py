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
    import list_codex_models
except ModuleNotFoundError:
    _lister_path = Path(__file__).resolve().with_name("list-codex-models.py")
    _lister_spec = importlib.util.spec_from_file_location(
        "list_codex_models", _lister_path
    )
    if _lister_spec is None or _lister_spec.loader is None:
        raise
    list_codex_models = importlib.util.module_from_spec(_lister_spec)
    _lister_spec.loader.exec_module(list_codex_models)


PLUGIN_ID = "casefile@humans-md"
MARKETPLACE = "humans-md"
RECEIPT_SCHEMA = 6
REQUIRED_MODELS = {
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "gpt-5.3-codex-spark",
}
V1_SELECTOR_MODELS = {"gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"}
MULTI_AGENT_VERSIONS = {"v1", "v2"}
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
        "model_catalog_json",
        "features.multi_agent",
        "features.multi_agent_v2",
        "agents.max_depth",
        "agents.max_threads",
    ),
    "features": ("multi_agent", "multi_agent_v2"),
    "agents": ("max_depth", "max_threads"),
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
        current = current[part]
    current[parts[-1]] = value


def multi_agent_version(value: str) -> str:
    if value not in MULTI_AGENT_VERSIONS:
        raise SetupError(f"unsupported multi-agent version: {value!r}")
    return value


def carried_models(profile_path: Path) -> set[str]:
    """Models the packaged catalog carries."""
    profile = tomllib.loads(profile_path.read_text(encoding="ascii"))
    return {
        target["id"]
        for target in profile.get("catalog", {}).get("targets", [])
        if isinstance(target.get("id"), str)
    }


def pinned_models(profile_path: Path) -> set[str]:
    """Models this repository requires that upstream may no longer list."""
    profile = tomllib.loads(profile_path.read_text(encoding="ascii"))
    return {
        target["id"]
        for target in profile.get("catalog", {}).get("targets", [])
        if target.get("pinned") is True and isinstance(target.get("id"), str)
    }


def synthesised_model(target: dict, profile_path: Path) -> dict:
    """Build a catalog entry for a pinned model from its declared target."""
    expected = target.get("expected", {})
    display_name = expected.get("display_name")
    visibility = expected.get("visibility")
    efforts = target.get("required_reasoning", [])
    if not isinstance(display_name, str) or not isinstance(visibility, str):
        raise SetupError(f"pinned target lacks expected fields: {target.get('id')}")
    if not isinstance(efforts, list) or not efforts:
        raise SetupError(f"pinned target lacks required reasoning: {target.get('id')}")
    return {
        "slug": target["id"],
        "display_name": display_name,
        "visibility": visibility,
        "supported_reasoning_levels": [{"effort": effort} for effort in efforts],
        # Present so the selector pass can clear or assign it; value set per runtime below.
        "multi_agent_version": None,
    }


def catalog_override(profile_path: Path, version: str) -> tuple[bytes, list[str]]:
    version = multi_agent_version(version)
    profile = tomllib.loads(profile_path.read_text(encoding="ascii"))
    policy = profile.get("catalog", {})
    if profile.get("schema_version") != 1:
        raise SetupError("unsupported catalog or profile schema")
    if policy.get("id_field") != "slug" or policy.get("selector_fields") != [
        "multi_agent_version"
    ]:
        raise SetupError("unsupported catalog policy")
    targets = policy.get("targets", [])
    identifiers = [target.get("id") for target in targets]
    if None in identifiers or len(set(identifiers)) != len(identifiers):
        raise SetupError("catalog has missing or duplicate model IDs")
    missing = sorted(REQUIRED_MODELS - set(identifiers))
    if missing:
        raise SetupError(f"catalog lacks required models: {', '.join(missing)}")

    result = {"models": [synthesised_model(target, profile_path) for target in targets]}
    output = {model["slug"]: model for model in result["models"]}
    patched: list[str] = []
    for target in targets:
        model_id = target.get("id")
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
    if version == "v2":
        for model in result["models"]:
            model["multi_agent_version"] = "v2"
    return canonical(result), sorted(patched)


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


def drop_owned_lines(data: bytes) -> bytes:
    output = []
    table = ""
    for line in data.splitlines(keepends=True):
        header = re.fullmatch(rb"\s*\[\[?([^\]\r\n]+)\]\]?\s*(?:#.*)?\r?\n?", line)
        if header:
            table = header.group(1).decode("utf-8").strip()
        assignment = re.match(rb"\s*([A-Za-z0-9_.-]+)\s*=", line)
        if assignment and assignment.group(1).decode("ascii") in OWNED_KEYS.get(table, ()):
            continue
        output.append(line)
    return b"".join(output)


def config_candidate(
    current: bytes,
    root: Path,
    catalog: Path,
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
    if index is not None:
        payload = "".join(
            f"{name} = {owned['agents'][name]}\n" for name in ("max_depth", "max_threads")
        )
        current = insert_after_line(
            current, index, marked(AGENT_BEGIN, payload.encode("ascii"), AGENT_END)
        )
        fragment = fragment.replace("\n[agents]\nmax_depth = 2\nmax_threads = 6\n", "\n", 1)

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
    for begin, end in (
        (SCALAR_BEGIN, SCALAR_END),
        (FEATURE_BEGIN, FEATURE_END),
        (AGENT_BEGIN, AGENT_END),
    ):
        while begin in data:
            start = data.index(begin)
            stop = data.find(end, start)
            if stop < 0:
                raise SetupError("managed config marker is unbalanced")
            data = data[:start] + data[stop + len(end) :]
    result = remove_owned_tables(data)
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
    paths = [home / "config.toml", home / f"models-casefile-{version}.json"]
    if binary is not None:
        paths.append(binary)
    return paths


def pointer(home: Path) -> Path:
    return home / "state/casefile/current.json"


def acquire_models(executable: str, profile_path: Path) -> dict:
    try:
        return list_codex_models.listing(executable, profile_path)
    except list_codex_models.ProjectionError as error:
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


def require_available_models(
    projection: dict, pinned: set[str] | None = None, carried: set[str] | None = None
) -> set[str]:
    identifiers = catalog_ids(projection, "Codex model projection")
    missing = sorted(REQUIRED_MODELS - identifiers - (pinned or set()))
    if missing:
        raise SetupError(f"Codex lacks required models: {', '.join(missing)}")
    # Refuse rather than silently install against a model set this catalog does not cover.
    if carried is not None:
        unsupported = sorted(identifiers - carried)
        if unsupported:
            raise SetupError(
                "Codex offers models the packaged catalog does not carry: "
                + ", ".join(unsupported)
            )
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


def cross_check_catalog(expected_ids: set[str], raw: dict, label: str) -> None:
    raw_ids = catalog_ids(raw, label)
    missing = sorted(expected_ids - raw_ids)
    extra = sorted(raw_ids - expected_ids)
    if missing or extra:
        details = []
        if missing:
            details.append("missing " + ", ".join(missing))
        if extra:
            details.append("unexpected " + ", ".join(extra))
        raise SetupError(f"{label} differs from the packaged catalog: {'; '.join(details)}")


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
    planning_root: Path | None = None,
    version: str = "v1",
) -> dict:
    version = multi_agent_version(version)
    root, manifest = plugin_root(root)
    planning = casefile_runtime.planning_root(planning_root or root)
    runtime = casefile_runtime.select(root, manifest["version"])
    binary = casefile_runtime.destination(home, manifest["version"], runtime["target"])
    casefile_runtime.probe(runtime["source"], manifest["version"], planning)
    environment = {**os.environ, "CODEX_HOME": str(home)}
    plugin = discover(executable, environment, manifest["version"])
    catalog_path = home / f"models-casefile-{version}.json"
    config_path = home / "config.toml"
    current = config_path.read_bytes() if config_path.is_file() else b""
    previous = receipt(home, None) if pointer(home).is_file() else None
    if previous is not None:
        current = unowned_config(current)
    # Build and verify the candidate before asking Codex for its model list.
    config = config_candidate(current, root, catalog_path, version, binary, planning)
    profiles_path = root / "config/profiles.toml"
    projection = acquire_models(executable, profiles_path)
    require_available_models(
        projection, pinned_models(profiles_path), carried_models(profiles_path)
    )
    catalog, patched = catalog_override(profiles_path, version)
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
        "config_ownership": "marked key and table blocks",
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
    profiles_path = plan["root"] / "config/profiles.toml"
    projection = acquire_models(plan["executable"], profiles_path)
    require_available_models(
        projection, pinned_models(profiles_path), carried_models(profiles_path)
    )
    catalog_path = plan["home"] / f"models-casefile-{version}.json"
    document = raw_catalog(catalog_path)
    cross_check_catalog(carried_models(profiles_path), document, "written Casefile catalog")
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
            raise SetupError("Codex executable was not found")  # kept: nothing can run without it
        if arguments.operation == "install":
            selections = arguments.multi_agent_version or []
            selected_version = selections[-1] if selections else "v1"
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
